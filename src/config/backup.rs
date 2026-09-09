// src/config/backup.rs
use super::model::AppConfig;
use super::vault::{decrypt_with_passphrase, encrypt_with_passphrase, VaultEnvelope, VaultError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupExportRequest {
    pub passphrase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupBundle {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub is_encrypted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_envelope: Option<VaultEnvelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plaintext_config: Option<AppConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupRestoreRequest {
    pub bundle: BackupBundle,
    pub passphrase: Option<String>,
}

pub fn create_backup_bundle(config: &AppConfig, passphrase: Option<&str>) -> Result<BackupBundle, VaultError> {
    if let Some(pass) = passphrase {
        let json_bytes = serde_json::to_vec(config)
            .map_err(|e| VaultError::EncryptionFailed(e.to_string()))?;
        let envelope = encrypt_with_passphrase(&json_bytes, pass)?;
        Ok(BackupBundle {
            version: 1,
            created_at: Utc::now(),
            is_encrypted: true,
            vault_envelope: Some(envelope),
            plaintext_config: None,
        })
    } else {
        Ok(BackupBundle {
            version: 1,
            created_at: Utc::now(),
            is_encrypted: false,
            vault_envelope: None,
            plaintext_config: Some(config.clone()),
        })
    }
}

pub fn restore_backup_bundle(bundle: &BackupBundle, passphrase: Option<&str>) -> Result<AppConfig, VaultError> {
    if bundle.is_encrypted {
        let envelope = bundle.vault_envelope.as_ref().ok_or(VaultError::InvalidEnvelope)?;
        let pass = passphrase.ok_or_else(|| VaultError::DecryptionFailed("Passphrase required for encrypted backup".to_string()))?;
        let plaintext_bytes = decrypt_with_passphrase(envelope, pass)?;
        let config: AppConfig = serde_json::from_slice(&plaintext_bytes)
            .map_err(|e| VaultError::DecryptionFailed(format!("Invalid JSON in backup payload: {}", e)))?;
        Ok(config)
    } else {
        bundle.plaintext_config.clone().ok_or_else(|| VaultError::DecryptionFailed("Missing plaintext config in unencrypted backup".to_string()))
    }
}
