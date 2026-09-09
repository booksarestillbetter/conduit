// src/auth/jwt.rs
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,       // User ID
    pub username: String,
    pub is_admin: bool,
    #[serde(default = "default_token_version")]
    pub token_version: i64,
    pub exp: usize,        // Expiration timestamp
    pub iat: usize,        // Issued at
}

fn default_token_version() -> i64 {
    1
}

pub fn create_jwt(user_id: &str, username: &str, is_admin: bool, token_version: i64, secret: &str, expires_in_days: i64) -> anyhow::Result<String> {
    let now = Utc::now();
    let exp = (now + Duration::days(expires_in_days)).timestamp() as usize;
    let iat = now.timestamp() as usize;

    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        is_admin,
        token_version,
        exp,
        iat,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(token)
}

pub fn verify_jwt(token: &str, secret: &str) -> anyhow::Result<Claims> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;

    Ok(token_data.claims)
}
