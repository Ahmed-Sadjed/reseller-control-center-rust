use actix_web::{web, HttpResponse};
use redis::AsyncCommands;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    config::Settings,
    error::ApiError,
    middleware::AuthUser,
    models::User,
    utils::{
        crypto::verify_password,
        jwt::{decode_token, generate_tokens},
    },
};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh: String,
}

#[derive(Debug, Deserialize)]
pub struct LogoutRequest {
    pub refresh: String,
    pub access: Option<String>,
}

pub async fn login(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    req: web::Json<LoginRequest>,
) -> Result<HttpResponse, ApiError> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, password_hash, role, credit_balance, is_active, uuid, date_joined \
         FROM users WHERE (username = $1 OR email = $1) AND is_active = true",
    )
    .bind(&req.username)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or(ApiError::Unauthorized)?;

    if !verify_password(&req.password, &user.password_hash).unwrap_or(false) {
        return Err(ApiError::Unauthorized);
    }

    let tokens = generate_tokens(
        user.id,
        &user.email,
        &user.role,
        &settings.jwt_secret,
        settings.jwt_access_ttl_minutes,
        settings.jwt_refresh_ttl_days,
    )
    .map_err(|e| ApiError::Internal(format!("token generation: {e}")))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "access": tokens.access,
        "refresh": tokens.refresh,
        "user": {
            "id": user.id,
            "username": user.username,
            "email": user.email,
            "role": user.role,
            "credit_balance": user.credit_balance,
        },
    })))
}

pub async fn refresh(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    redis_conn: web::Data<redis::aio::MultiplexedConnection>,
    req: web::Json<RefreshRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims =
        decode_token(&req.refresh, &settings.jwt_secret).map_err(|_| ApiError::Unauthorized)?;

    let mut conn = redis_conn.get_ref().clone();
    let blacklisted: Option<String> = conn
        .get(format!("blacklist:{}", claims.jti))
        .await
        .map_err(|_| ApiError::Unauthorized)?;
    if blacklisted.is_some() {
        return Err(ApiError::Unauthorized);
    }

    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)?;
    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, password_hash, role, credit_balance, is_active, uuid, date_joined \
         FROM users WHERE id = $1 AND is_active = true",
    )
    .bind(user_id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or(ApiError::Unauthorized)?;

    // Rotate: blacklist the presented refresh token, issue a new pair.
    conn.set_ex::<_, _, ()>(
        format!("blacklist:{}", claims.jti),
        "1",
        settings.jwt_refresh_ttl_days as u64 * 86400,
    )
    .await
    .map_err(|_| ApiError::Unauthorized)?;

    let tokens = generate_tokens(
        user.id,
        &user.email,
        &user.role,
        &settings.jwt_secret,
        settings.jwt_access_ttl_minutes,
        settings.jwt_refresh_ttl_days,
    )
    .map_err(|e| ApiError::Internal(format!("token generation: {e}")))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "access": tokens.access,
        "refresh": tokens.refresh,
    })))
}

pub async fn logout(
    settings: web::Data<Settings>,
    redis_conn: web::Data<redis::aio::MultiplexedConnection>,
    req: web::Json<LogoutRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims =
        decode_token(&req.refresh, &settings.jwt_secret).map_err(|_| ApiError::Unauthorized)?;

    let mut conn = redis_conn.get_ref().clone();
    conn.set_ex::<_, _, ()>(
        format!("blacklist:{}", claims.jti),
        "1",
        settings.jwt_refresh_ttl_days as u64 * 86400,
    )
    .await
    .map_err(|_| ApiError::Unauthorized)?;

    if let Some(access) = &req.access {
        if let Ok(access_claims) = decode_token(access, &settings.jwt_secret) {
            conn.set_ex::<_, _, ()>(
                format!("blacklist:{}", access_claims.jti),
                "1",
                settings.jwt_refresh_ttl_days as u64 * 86400,
            )
            .await
            .map_err(|_| ApiError::Unauthorized)?;
        }
    }

    Ok(HttpResponse::NoContent().finish())
}

pub async fn me(user: AuthUser) -> Result<HttpResponse, ApiError> {
    let data = serde_json::json!({
        "id": user.0.id,
        "username": user.0.username,
        "email": user.0.email,
        "role": user.0.role,
        "credit_balance": user.0.credit_balance,
    });
    Ok(HttpResponse::Ok().json(data))
}
