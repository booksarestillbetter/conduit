use async_trait::async_trait;
use crate::config::{FetcherNodeConfig, RetrieverClientType};
use crate::models::{NativeCircuitBreakerStatus, NodeCapabilities, Torrent};

#[async_trait]
pub trait TorrentClientTrait: Send + Sync {
    fn node_name(&self) -> &str;
    fn config(&self) -> &FetcherNodeConfig;
    fn client_type(&self) -> RetrieverClientType;

    /// Detects optional native features this specific daemon instance supports (e.g. a
    /// built-in tracker circuit breaker). Default: none — only backends that can actually
    /// support a given feature need to override this.
    async fn get_capabilities(&self) -> anyhow::Result<NodeCapabilities> {
        Ok(NodeCapabilities::default())
    }

    /// Lists this node's own native tracker circuit breaker status, for backends where
    /// `get_capabilities().native_tracker_circuit_breaker` is true. Default: empty — only
    /// meaningful once a backend actually has a native breaker to report.
    async fn list_circuit_breakers(&self) -> anyhow::Result<Vec<NativeCircuitBreakerStatus>> {
        Ok(Vec::new())
    }

    /// Forces a tracker host's native breaker into a tripped state. No-op default.
    async fn force_trip_circuit_breaker(&self, _host: &str) -> anyhow::Result<()> {
        Ok(())
    }

    /// Clears a tracker host's native breaker state entirely. No-op default.
    async fn force_reset_circuit_breaker(&self, _host: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn get_torrents(&self, ids: Option<Vec<i64>>) -> anyhow::Result<Vec<Torrent>>;
    async fn get_torrent_details(&self, id: i64) -> anyhow::Result<Torrent>;
    async fn get_torrent_details_by_hash(&self, hash: &str) -> anyhow::Result<Torrent>;
    async fn add_torrent(
        &self,
        magnet_or_url: Option<&str>,
        metainfo_b64: Option<&str>,
        download_dir: Option<&str>,
        paused: bool,
    ) -> anyhow::Result<serde_json::Value>;
    async fn start_torrents(&self, ids: &[i64], now: bool) -> anyhow::Result<()>;
    async fn stop_torrents(&self, ids: &[i64]) -> anyhow::Result<()>;
    async fn verify_torrents(&self, ids: &[i64]) -> anyhow::Result<()>;
    async fn reannounce_torrents(&self, ids: &[i64]) -> anyhow::Result<()>;
    async fn remove_torrents(&self, ids: &[i64], delete_local_data: bool) -> anyhow::Result<()>;
    async fn set_location(&self, ids: &[i64], location: &str, move_data: bool) -> anyhow::Result<()>;
    async fn queue_move(&self, ids: &[i64], direction: &str) -> anyhow::Result<()>;
    async fn set_sequential_download(&self, ids: &[i64], enabled: bool) -> anyhow::Result<()>;
    async fn rename_path(&self, id: i64, path: &str, new_name: &str) -> anyhow::Result<serde_json::Value>;
    async fn set_turtle_mode(&self, enabled: bool) -> anyhow::Result<()>;
    async fn update_blocklist(&self) -> anyhow::Result<i64>;
    async fn get_free_space(&self, path: &str) -> anyhow::Result<i64>;
    async fn get_session(&self) -> anyhow::Result<serde_json::Value>;
    async fn set_session(&self, settings: serde_json::Value) -> anyhow::Result<()>;
    async fn get_session_stats(&self) -> anyhow::Result<serde_json::Value>;
    async fn test_port(&self) -> anyhow::Result<bool>;
    async fn replace_trackers(&self, id: i64, tracker_list: &str, old_url: &str, new_url: &str) -> anyhow::Result<()>;
}
