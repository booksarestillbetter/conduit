// src/auth/mod.rs
pub mod jwt;
pub mod middleware;
pub mod mobile;
pub mod rate_limit;
pub mod totp;
pub mod ws_ticket;

pub use jwt::*;
pub use middleware::*;
pub use mobile::*;
pub use rate_limit::RateLimiter;
pub use totp::*;
pub use ws_ticket::WsTicketStore;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use sha2::{Digest, Sha256};

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Argon2 hash error: {}", e))?
        .to_string();
    Ok(hash)
}

pub fn verify_password(password: &str, password_hash: &str) -> bool {
    let parsed_hash = match PasswordHash::new(password_hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

pub const DUMMY_ARGON2_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$WJ1l83Y487w/q9y3z9N2A7dF7T/Z6wP9Y2dF8G5h4kI";

pub async fn verify_password_async(password: String, password_hash: String) -> bool {
    tokio::task::spawn_blocking(move || verify_password(&password, &password_hash))
        .await
        .unwrap_or(false)
}

pub fn hash_api_token(raw_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    format!("{:x}", hasher.finalize())
}
