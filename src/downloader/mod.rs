#[allow(unused_imports)]
pub mod traits {
    pub use fetcher_core::traits::*;
}
#[allow(unused_imports)]
pub use fetcher_core::traits::TorrentClientTrait;
#[allow(unused_imports)]
pub use fetcher_core::registry::{FetcherDriverFactory, FetcherRegistry};

#[cfg(feature = "transmission")]
#[allow(unused_imports)]
pub use fetcher_transmission::{TransmissionAdapter, TransmissionClient, TransmissionDriverFactory};

#[cfg(feature = "qbittorrent")]
#[allow(unused_imports)]
pub use fetcher_qbittorrent::{QBittorrentClient, QBittorrentDriverFactory};

#[cfg(feature = "deluge")]
#[allow(unused_imports)]
pub use fetcher_deluge::{DelugeClient, DelugeDriverFactory};

#[cfg(feature = "synapse")]
#[allow(unused_imports)]
pub use fetcher_synapse::{SynapseClient, SynapseDriverFactory};

use crate::config::{FetcherNodeConfig, RetrieverClientType};
use std::sync::Arc;

/// A stub adapter returned when a configured backend has been disabled at compile time.
#[allow(dead_code)]
pub struct DisabledClientAdapter {
    config: FetcherNodeConfig,
    backend_name: &'static str,
}

#[allow(dead_code)]
impl DisabledClientAdapter {
    pub fn new(config: FetcherNodeConfig, backend_name: &'static str) -> Self {
        Self { config, backend_name }
    }
}

#[async_trait::async_trait]
impl TorrentClientTrait for DisabledClientAdapter {
    fn node_name(&self) -> &str {
        &self.config.name
    }
    fn config(&self) -> &FetcherNodeConfig {
        &self.config
    }
    fn client_type(&self) -> RetrieverClientType {
        self.config.client_type
    }
    async fn get_torrents(&self, _ids: Option<Vec<i64>>) -> anyhow::Result<Vec<crate::fetcher::models::Torrent>> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn get_torrent_details(&self, _id: i64) -> anyhow::Result<crate::fetcher::models::Torrent> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn get_torrent_details_by_hash(&self, _hash: &str) -> anyhow::Result<crate::fetcher::models::Torrent> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn add_torrent(&self, _magnet_or_url: Option<&str>, _metainfo_b64: Option<&str>, _download_dir: Option<&str>, _paused: bool) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn start_torrents(&self, _ids: &[i64], _now: bool) -> anyhow::Result<()> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn stop_torrents(&self, _ids: &[i64]) -> anyhow::Result<()> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn verify_torrents(&self, _ids: &[i64]) -> anyhow::Result<()> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn reannounce_torrents(&self, _ids: &[i64]) -> anyhow::Result<()> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn remove_torrents(&self, _ids: &[i64], _delete_local_data: bool) -> anyhow::Result<()> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn set_location(&self, _ids: &[i64], _location: &str, _move_data: bool) -> anyhow::Result<()> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn queue_move(&self, _ids: &[i64], _direction: &str) -> anyhow::Result<()> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn set_sequential_download(&self, _ids: &[i64], _enabled: bool) -> anyhow::Result<()> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn rename_path(&self, _id: i64, _path: &str, _new_name: &str) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn set_turtle_mode(&self, _enabled: bool) -> anyhow::Result<()> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn update_blocklist(&self) -> anyhow::Result<i64> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn get_free_space(&self, _path: &str) -> anyhow::Result<i64> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn get_session(&self) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn set_session(&self, _settings: serde_json::Value) -> anyhow::Result<()> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn get_session_stats(&self) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn test_port(&self) -> anyhow::Result<bool> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
    async fn replace_trackers(&self, _id: i64, _tracker_list: &str, _old_url: &str, _new_url: &str) -> anyhow::Result<()> {
        anyhow::bail!("Backend '{}' is disabled in this build of Conduit", self.backend_name)
    }
}

pub fn create_client(config: FetcherNodeConfig) -> Arc<dyn TorrentClientTrait> {
    match config.client_type {
        #[cfg(feature = "transmission")]
        RetrieverClientType::Transmission => Arc::new(TransmissionAdapter::new(config)),
        #[cfg(not(feature = "transmission"))]
        RetrieverClientType::Transmission => Arc::new(DisabledClientAdapter::new(config, "transmission")),

        #[cfg(feature = "qbittorrent")]
        RetrieverClientType::QBittorrent => Arc::new(QBittorrentClient::new(config)),
        #[cfg(not(feature = "qbittorrent"))]
        RetrieverClientType::QBittorrent => Arc::new(DisabledClientAdapter::new(config, "qbittorrent")),

        #[cfg(feature = "deluge")]
        RetrieverClientType::Deluge => Arc::new(DelugeClient::new(config)),
        #[cfg(not(feature = "deluge"))]
        RetrieverClientType::Deluge => Arc::new(DisabledClientAdapter::new(config, "deluge")),

        #[cfg(feature = "synapse")]
        RetrieverClientType::Synapse => Arc::new(SynapseClient::new(config)),
        #[cfg(not(feature = "synapse"))]
        RetrieverClientType::Synapse => Arc::new(DisabledClientAdapter::new(config, "synapse")),
    }
}
