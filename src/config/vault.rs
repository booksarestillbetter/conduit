// src/config/vault.rs
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("Key derivation failed: {0}")]
    KeyDerivationFailed(String),
    #[error("Invalid vault envelope")]
    InvalidEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VaultEnvelope {
    pub version: u32,
    pub salt_b64: String,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
}

/// Derive a 256-bit key from a passphrase and 16-byte salt using Argon2id.
pub fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], VaultError> {
    let mut key = [0u8; 32];
    let argon2 = Argon2::default();
    
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| VaultError::KeyDerivationFailed(e.to_string()))?;
        
    Ok(key)
}

/// Encrypt raw plaintext bytes into a `VaultEnvelope` using AES-256-GCM and Argon2id passphrase.
pub fn encrypt_with_passphrase(plaintext: &[u8], passphrase: &str) -> Result<VaultEnvelope, VaultError> {
    let mut salt = [0u8; 16];
    rand::rng().fill_bytes(&mut salt);

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| VaultError::EncryptionFailed(e.to_string()))?;

    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| VaultError::EncryptionFailed(e.to_string()))?;

    Ok(VaultEnvelope {
        version: 1,
        salt_b64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, salt),
        nonce_b64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, nonce_bytes),
        ciphertext_b64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, ciphertext),
    })
}

/// Decrypt a `VaultEnvelope` using AES-256-GCM and Argon2id passphrase.
pub fn decrypt_with_passphrase(envelope: &VaultEnvelope, passphrase: &str) -> Result<Vec<u8>, VaultError> {
    let salt = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &envelope.salt_b64)
        .map_err(|_| VaultError::InvalidEnvelope)?;
    let nonce_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &envelope.nonce_b64)
        .map_err(|_| VaultError::InvalidEnvelope)?;
    let ciphertext = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &envelope.ciphertext_b64)
        .map_err(|_| VaultError::InvalidEnvelope)?;

    if nonce_bytes.len() != 12 {
        return Err(VaultError::InvalidEnvelope);
    }

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| VaultError::DecryptionFailed(e.to_string()))?;

    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_slice())
        .map_err(|e| VaultError::DecryptionFailed(e.to_string()))?;

    Ok(plaintext)
}
