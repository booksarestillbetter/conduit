// src/engines/backup.rs
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tracing::{error, info, warn};
use utoipa::ToSchema;

use crate::config::model::AppConfig;
use crate::config::{create_backup_bundle, ConfigManager};
use crate::db::Database;
use crate::engines::registry::{ensure_registered, EngineRegistry, TaskHandle};
use crate::notify::NotificationManager;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OffsiteBackupResult {
    pub target_type: String,
    pub location: String,
    pub bytes: usize,
    pub pruned_files: usize,
    pub timestamp: DateTime<Utc>,
}

pub async fn run_offsite_backup_once(config: &AppConfig) -> anyhow::Result<OffsiteBackupResult> {
    let bcfg = &config.offsite_backup;
    if bcfg.passphrase.trim().is_empty() {
        anyhow::bail!("Off-site backup passphrase must be configured to encrypt backups");
    }
    let dest = bcfg.destination.trim();
    if dest.is_empty() {
        anyhow::bail!("Off-site backup destination path or URL is empty");
    }

    let bundle = create_backup_bundle(config, Some(&bcfg.passphrase))
        .map_err(|e| anyhow::anyhow!("Failed to encrypt backup bundle: {}", e))?;
    let payload = serde_json::to_vec_pretty(&bundle)?;
    let byte_len = payload.len();
    let now = Utc::now();
    let filename = format!("conduit-backup-{}.conduit-backup.json", now.format("%Y%m%dT%H%M%SZ"));

    let target_type = bcfg.target_type.trim().to_ascii_lowercase();
    match target_type.as_str() {
        "local" | "" => {
            let dir = PathBuf::from(dest);
            tokio::fs::create_dir_all(&dir).await?;
            let out_path = dir.join(&filename);
            tokio::fs::write(&out_path, &payload).await?;

            let mut pruned = 0usize;
            let retain = bcfg.retain_count.max(1);
            let mut entries = tokio::fs::read_dir(&dir).await?;
            let mut backup_files: Vec<PathBuf> = Vec::new();
            while let Some(entry) = entries.next_entry().await? {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("conduit-backup-") && name.ends_with(".conduit-backup.json") {
                    backup_files.push(entry.path());
                }
            }
            backup_files.sort();
            backup_files.reverse();
            for old_file in backup_files.into_iter().skip(retain) {
                if tokio::fs::remove_file(&old_file).await.is_ok() {
                    pruned += 1;
                }
            }

            Ok(OffsiteBackupResult {
                target_type: "local".to_string(),
                location: out_path.display().to_string(),
                bytes: byte_len,
                pruned_files: pruned,
                timestamp: now,
            })
        }
        "webdav" | "s3_http" => {
            if !dest.starts_with("http://") && !dest.starts_with("https://") {
                anyhow::bail!("HTTP/WebDAV backup destination must start with http:// or https://");
            }
            let upload_url = if dest.contains('?') {
                dest.to_string()
            } else {
                format!("{}/{}", dest.trim_end_matches('/'), filename)
            };

            let mut req = crate::api::shared_http_client()
                .put(&upload_url)
                .header("Content-Type", "application/json")
                .body(payload);

            if !bcfg.auth_username.trim().is_empty() {
                req = req.basic_auth(bcfg.auth_username.trim(), Some(&bcfg.auth_secret));
            } else if !bcfg.auth_secret.trim().is_empty() {
                req = req.bearer_auth(bcfg.auth_secret.trim());
            }

            let resp = req.send().await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("Off-site backup upload failed ({}): {}", status, body);
            }

            Ok(OffsiteBackupResult {
                target_type,
                location: upload_url,
                bytes: byte_len,
                pruned_files: 0,
                timestamp: now,
            })
        }
        other => anyhow::bail!("Unsupported off-site backup target_type: {}", other),
    }
}

pub async fn run_backup_loop(
    config_mgr: ConfigManager,
    db: Database,
    _notifiers: Arc<NotificationManager>,
    registry: EngineRegistry,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    ensure_registered(&registry, "offsite_backup");
    let handle = TaskHandle::new(registry, "offsite_backup");

    loop {
        let cfg = config_mgr.get().await;
        if cfg.offsite_backup.enabled {
            match run_offsite_backup_once(&cfg).await {
                Ok(res) => {
                    info!(
                        "Encrypted off-site backup completed ({}, {} bytes, pruned {}) -> {}",
                        res.target_type, res.bytes, res.pruned_files, res.location
                    );
                    let _ = db.log_event(
                        "offsite_backup",
                        "info",
                        &format!(
                            "Encrypted backup saved ({}, {} bytes, pruned {})",
                            res.target_type, res.bytes, res.pruned_files
                        ),
                        None,
                    );
                    handle.tick();
                }
                Err(e) => {
                    let msg = format!("Automated encrypted off-site backup failed: {}", e);
                    error!("{}", msg);
                    let _ = db.log_event("offsite_backup", "error", &msg, None);
                    handle.tick_err(e.to_string());
                    NotificationManager::dispatch(
                        &cfg.notifications,
                        "system",
                        None,
                        "Off-site Backup Failed",
                        &msg,
                        Some("#ef4444"),
                    )
                    .await;
                    warn!("Next off-site backup attempt will run after configured interval");
                }
            }
        } else {
            handle.tick();
        }

        let secs = cfg.offsite_backup.interval_hours.max(1) * 3600;
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(secs)) => {}
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::restore_backup_bundle;

    #[tokio::test]
    async fn test_run_offsite_backup_once_local_encrypts_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = AppConfig::default();
        cfg.offsite_backup.enabled = true;
        cfg.offsite_backup.target_type = "local".to_string();
        cfg.offsite_backup.destination = dir.path().display().to_string();
        cfg.offsite_backup.passphrase = "super-secret-passphrase".to_string();
        cfg.offsite_backup.retain_count = 1;

        // Pre-seed two older backup files so retention pruning is exercised
        std::fs::write(dir.path().join("conduit-backup-20200101T000000Z.conduit-backup.json"), b"{}").unwrap();
        std::fs::write(dir.path().join("conduit-backup-20200102T000000Z.conduit-backup.json"), b"{}").unwrap();

        let res = run_offsite_backup_once(&cfg).await.unwrap();
        assert_eq!(res.target_type, "local");
        assert_eq!(res.pruned_files, 2);
        assert!(res.bytes > 0);

        let raw = std::fs::read(&res.location).unwrap();
        let bundle: crate::config::BackupBundle = serde_json::from_slice(&raw).unwrap();
        let restored = restore_backup_bundle(&bundle, Some("super-secret-passphrase")).unwrap();
        assert_eq!(restored.offsite_backup.target_type, "local");
    }
}
