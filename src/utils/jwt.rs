use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub role: String,
    pub jti: String,
    pub exp: usize,
    pub iat: usize,
}

pub struct TokenPair {
    pub access: String,
    pub refresh: String,
}

pub fn generate_tokens(
    user_id: Uuid,
    email: &str,
    role: &str,
    secret: &str,
    access_ttl_minutes: i64,
    refresh_ttl_days: i64,
) -> Result<TokenPair, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let access_exp = now + Duration::minutes(access_ttl_minutes);
    let refresh_exp = now + Duration::days(refresh_ttl_days);

    let access = encode(
        &Header::default(),
        &Claims {
            sub: user_id.to_string(),
            email: email.to_string(),
            role: role.to_string(),
            jti: Uuid::new_v4().to_string(),
            exp: access_exp.timestamp() as usize,
            iat: now.timestamp() as usize,
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    let refresh = encode(
        &Header::default(),
        &Claims {
            sub: user_id.to_string(),
            email: email.to_string(),
            role: role.to_string(),
            jti: Uuid::new_v4().to_string(),
            exp: refresh_exp.timestamp() as usize,
            iat: now.timestamp() as usize,
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(TokenPair { access, refresh })
}

pub fn decode_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test_secret_key_0123456789abcdef";

    #[test]
    fn tokens_roundtrip_with_correct_claims() {
        let user_id = Uuid::new_v4();
        let pair = generate_tokens(user_id, "admin@example.com", "ADMIN", SECRET, 60, 7).unwrap();

        let access = decode_token(&pair.access, SECRET).unwrap();
        assert_eq!(access.sub, user_id.to_string());
        assert_eq!(access.email, "admin@example.com");
        assert_eq!(access.role, "ADMIN");
        assert!(access.exp > access.iat);
        assert!(!access.jti.is_empty());

        let refresh = decode_token(&pair.refresh, SECRET).unwrap();
        assert_eq!(refresh.sub, user_id.to_string());
        assert_ne!(access.jti, refresh.jti, "access and refresh must have distinct jti");
    }

    #[test]
    fn wrong_secret_rejected() {
        let pair = generate_tokens(Uuid::new_v4(), "a@b.c", "RESELLER", SECRET, 60, 7).unwrap();
        assert!(decode_token(&pair.access, "another_secret_0123456789abcdef").is_err());
    }

    #[test]
    fn tampered_token_rejected() {
        let pair = generate_tokens(Uuid::new_v4(), "a@b.c", "RESELLER", SECRET, 60, 7).unwrap();
        let mut chars: Vec<char> = pair.access.chars().collect();
        let n = chars.len();
        chars[n - 3] = if chars[n - 3] == 'a' { 'b' } else { 'a' };
        let tampered: String = chars.into_iter().collect();
        assert_ne!(tampered, pair.access);
        assert!(decode_token(&tampered, SECRET).is_err());
    }

    #[test]
    fn expired_token_rejected() {
        let user_id = Uuid::new_v4();
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            email: "a@b.c".to_string(),
            role: "RESELLER".to_string(),
            jti: Uuid::new_v4().to_string(),
            exp: (now - Duration::minutes(5)).timestamp() as usize,
            iat: (now - Duration::hours(1)).timestamp() as usize,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        assert!(decode_token(&token, SECRET).is_err());
    }
}
