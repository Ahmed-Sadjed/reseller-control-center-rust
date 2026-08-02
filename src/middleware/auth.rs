use actix_web::{dev::Payload, http::header, web, FromRequest, HttpRequest};
use redis::AsyncCommands;
use sqlx::PgPool;
use std::{future::Future, pin::Pin};
use uuid::Uuid;

use crate::{
    config::Settings,
    error::ApiError,
    models::User,
    utils::jwt::decode_token,
};

pub struct AuthUser(pub User);

fn bearer_token(req: &HttpRequest) -> Option<&str> {
    let header = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    header.strip_prefix("Bearer ")
}

impl FromRequest for AuthUser {
    type Error = ApiError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let settings = req
            .app_data::<web::Data<Settings>>()
            .map(web::Data::clone)
            .unwrap_or_else(|| web::Data::new(Settings::from_env()));
        let pool = match req.app_data::<web::Data<PgPool>>() {
            Some(p) => p.clone(),
            None => {
                return Box::pin(async {
                    Err(ApiError::Internal("db pool not configured".into()))
                });
            }
        };
        let redis_conn = match req.app_data::<web::Data<redis::aio::MultiplexedConnection>>() {
            Some(r) => r.clone(),
            None => {
                return Box::pin(async {
                    Err(ApiError::Internal("redis not configured".into()))
                });
            }
        };

        let token = match bearer_token(req) {
            Some(t) => t.to_string(),
            None => {
                return Box::pin(async { Err(ApiError::Unauthorized) });
            }
        };

        Box::pin(async move {
            let claims = decode_token(&token, &settings.jwt_secret)
                .map_err(|_| ApiError::Unauthorized)?;

            // Check Redis blacklist: key `blacklist:{jti}` => token was logged out.
            let mut conn = redis_conn.get_ref().clone();
            let blacklisted: Option<String> = conn
                .get(format!("blacklist:{}", claims.jti))
                .await
                .map_err(|_| ApiError::Unauthorized)?;
            if blacklisted.is_some() {
                return Err(ApiError::Unauthorized);
            }

            let user_id = Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)?;

            // User lookup: Redis cache `user:{id}` (60s) with DB fallback, so
            // authenticated requests don't hammer the connection pool.
            let cache_key = format!("user:{user_id}");
            let user: User = match conn
                .get::<_, Option<String>>(cache_key.clone())
                .await
            {
                Ok(Some(body)) => match serde_json::from_str::<User>(&body) {
                    Ok(u) if u.is_active => u,
                    _ => load_user(pool.get_ref(), user_id).await?,
                },
                _ => load_user(pool.get_ref(), user_id).await?,
            };
            if let Ok(body) = serde_json::to_string(&user) {
                let _ = conn.set_ex::<_, _, ()>(cache_key, body, 60).await;
            }

            Ok(AuthUser(user))
        })
    }
}

async fn load_user(pool: &PgPool, user_id: Uuid) -> Result<User, ApiError> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, email, password_hash, role, credit_balance, is_active, uuid, date_joined \
         FROM users WHERE id = $1 AND is_active = true",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Unauthorized)?
    .ok_or(ApiError::Unauthorized)
}
