//! Redis-backed request throttling, mirroring the old Django DRF throttles:
//! anonymous 30/hour per IP, authenticated 100/minute per user, and an
//! additional purchase scope of 5/minute per user on order creation.
//!
//! Buckets are fixed windows implemented with INCR + EXPIRE: the first
//! request in a window creates the key with a TTL, every request increments
//! it, and once the count exceeds the limit the request is rejected with
//! 429 until the window expires. Redis failures fail open (throttling must
//! never take the storefront down).

use std::{
    future::{ready, Future, Ready},
    pin::Pin,
    rc::Rc,
};

use actix_web::{
    body::{BoxBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    http::{header, header::HeaderValue, Method},
    web, Error, HttpResponse,
};

use crate::config::Settings;
use crate::utils::jwt::decode_token;

pub struct RateLimit;

impl<S, B> Transform<S, ServiceRequest> for RateLimit
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Transform = RateLimitMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RateLimitMiddleware {
            service: Rc::new(service),
        }))
    }
}

pub struct RateLimitMiddleware<S> {
    service: Rc<S>,
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

impl<S, B> Service<ServiceRequest> for RateLimitMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let path = req.path().to_string();
        let method = req.method().clone();
        let settings = req.app_data::<web::Data<Settings>>().cloned();
        let redis_conn = req
            .app_data::<web::Data<redis::aio::MultiplexedConnection>>()
            .cloned();
        let auth_header = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let xff = req
            .headers()
            .get(header::X_FORWARDED_FOR)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
            .map(|s| s.trim().to_string());
        let peer = req.peer_addr().map(|a| a.ip().to_string());
        let service = self.service.clone();

        Box::pin(async move {
            if let Some(buckets) = throttle_buckets(
                &path,
                &method,
                settings.as_deref().map(|s| s.as_ref()),
                auth_header.as_deref(),
                xff.as_deref(),
                peer.as_deref(),
            ) {
                if let Some(conn) = redis_conn {
                    let mut conn = conn.get_ref().clone();
                    for (key, limit) in buckets {
                        let window_secs = window_for(&key);
                        let count: u64 = redis::cmd("INCR")
                            .arg(&key)
                            .query_async(&mut conn)
                            .await
                            .unwrap_or(0);
                        if count == 1 {
                            // first request in the window: start the TTL
                            let _: redis::RedisResult<()> = redis::cmd("EXPIRE")
                                .arg(&key)
                                .arg(window_secs)
                                .query_async(&mut conn)
                                .await;
                        }
                        let over = count > limit;
                        if over {
                            let ttl: i64 = redis::cmd("TTL")
                                .arg(&key)
                                .query_async(&mut conn)
                                .await
                                .unwrap_or(window_secs);
                            let msg = format!(
                                "Request was throttled. Expected available in {ttl} seconds."
                            );
                            let throttled: ServiceResponse<BoxBody> = req.into_response(
                                HttpResponse::TooManyRequests()
                                    .insert_header((
                                        header::RETRY_AFTER,
                                        HeaderValue::from_str(&ttl.to_string()).unwrap(),
                                    ))
                                    .json(serde_json::json!({ "detail": msg })),
                            );
                            return Ok(throttled);
                        }
                    }
                }
            }
            service.call(req).await.map(|res| res.map_into_boxed_body())
        })
    }
}

/// Which throttles apply to this request, if any. Only /api/* paths are
/// throttled (matching Django, which left /health and /media out). CORS
/// preflights are exempt. Mirroring DRF: an authenticated purchase is
/// checked against BOTH the user bucket and the purchase bucket.
fn throttle_buckets(
    path: &str,
    method: &Method,
    settings: Option<&Settings>,
    auth_header: Option<&str>,
    xff: Option<&str>,
    peer: Option<&str>,
) -> Option<Vec<(String, u64)>> {
    if method == Method::OPTIONS || !path.starts_with("/api/") {
        return None;
    }
    // the storefront calls Django-style trailing-slash paths (/api/purchase/),
    // so match the canonical slash-less form for scope decisions
    let path = path.trim_end_matches('/');
    let settings = settings?;

    // authenticated requests: identify the user from the JWT
    if let Some(bearer) = auth_header.and_then(|s| s.strip_prefix("Bearer ")) {
        if let Ok(claims) = decode_token(bearer, &settings.jwt_secret) {
            let mut buckets =
                vec![(format!("rl:user:{}", claims.sub), settings.rate_limit_user)];
            // purchase scope: extra 5/minute on order creation
            if method == Method::POST && (path == "/api/orders" || path == "/api/purchase") {
                buckets.push((format!("rl:purchase:{}", claims.sub), settings.rate_limit_purchase));
            }
            return Some(buckets);
        }
    }

    // anonymous: per-IP (X-Forwarded-For first hop, then socket peer)
    let ip = xff
        .map(|s| s.to_string())
        .or_else(|| peer.map(|s| s.to_string()))?;
    Some(vec![(format!("rl:anon:{ip}"), settings.rate_limit_anon)])
}

/// Window size (seconds) for a bucket key. Anon buckets are hourly, user and
/// purchase buckets are per-minute.
fn window_for(key: &str) -> i64 {
    if key.starts_with("rl:anon:") {
        3600
    } else {
        60
    }
}
