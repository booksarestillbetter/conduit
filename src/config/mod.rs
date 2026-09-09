// src/config/mod.rs
pub mod backup;
pub mod model;
pub mod vault;

pub use backup::*;
pub use model::*;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Clone)]
pub struct ConfigManager {
    config_path: PathBuf,
    config: Arc<RwLock<AppConfig>>,
}

impl ConfigManager {
    pub async fn load_or_init<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let config_path = path.as_ref().to_path_buf();
        if let Some(parent) = config_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut config = if config_path.exists() {
            info!("Loading configuration from {}", config_path.display());
            let content = tokio::fs::read_to_string(&config_path).await?;
            let parsed: AppConfig = serde_json::from_str(&content)
                .unwrap_or_else(|err| {
                    warn!("Failed to parse config file ({}). Using default config.", err);
                    AppConfig::default()
                });
            parsed
        } else {
            info!("No existing configuration found at {}. Creating default.", config_path.display());
            let default_cfg = AppConfig::default();
            let json = serde_json::to_string_pretty(&default_cfg)?;
            tokio::fs::write(&config_path, json).await?;
            default_cfg
        };

        // Conduit's stable plex.tv device identity — generated once and persisted immediately so
        // it survives restarts and is reused for every OAuth/direct-server call from here on.
        if config.plex.client_identifier.is_empty() {
            config.plex.client_identifier = uuid::Uuid::new_v4().to_string();
            let json = serde_json::to_string_pretty(&config)?;
            tokio::fs::write(&config_path, json).await?;
        }

        // One-time migration from the pre-0.11 single Mattermost/Discord/webhook slots to the
        // multi-target notification model. categories: vec![] means "every category" here,
        // preserving the old "every enabled channel gets every enabled event type" behavior
        // exactly. Runs once — after this, the legacy fields below are inert.
        if config.notifications.targets.is_empty() {
            let mut migrated = false;
            if let Some(mm) = config.notifications.mattermost.clone() {
                config.notifications.targets.push(NotificationTarget {
                    id: "mattermost-legacy".to_string(),
                    name: "Mattermost".to_string(),
                    enabled: mm.enabled,
                    channel: NotificationChannel::Mattermost(mm),
                    categories: Vec::new(),
                    zone_id: None,
                });
                migrated = true;
            }
            if let Some(dc) = config.notifications.discord.clone() {
                config.notifications.targets.push(NotificationTarget {
                    id: "discord-legacy".to_string(),
                    name: "Discord".to_string(),
                    enabled: dc.enabled,
                    channel: NotificationChannel::Discord(dc),
                    categories: Vec::new(),
                    zone_id: None,
                });
                migrated = true;
            }
            if let Some(wh) = config.notifications.generic_webhook.clone() {
                config.notifications.targets.push(NotificationTarget {
                    id: "webhook-legacy".to_string(),
                    name: "Webhook".to_string(),
                    enabled: wh.enabled,
                    channel: NotificationChannel::Webhook(wh),
                    categories: Vec::new(),
                    zone_id: None,
                });
                migrated = true;
            }
            if migrated {
                info!("Migrated legacy Mattermost/Discord/Webhook notification config into the new multi-target model");
                let json = serde_json::to_string_pretty(&config)?;
                tokio::fs::write(&config_path, json).await?;
            }
        }

        Ok(Self {
            config_path,
            config: Arc::new(RwLock::new(config)),
        })
    }

    pub async fn get(&self) -> AppConfig {
        self.config.read().await.clone()
    }

    pub async fn update(&self, new_config: AppConfig) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(&new_config)?;
        tokio::fs::write(&self.config_path, json).await?;
        let mut write_guard = self.config.write().await;
        *write_guard = new_config;
        info!("Configuration updated and saved to disk.");
        Ok(())
    }
}
