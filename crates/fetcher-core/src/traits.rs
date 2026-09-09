use async_trait::async_trait;
use crate::config::{FetcherNodeConfig, RetrieverClientType};
use crate::models::Torrent;

#[async_trait]
pub trait TorrentClientTrait: Send + Sync {
    fn node_name(&self) -> &str;
    fn config(&self) -> &FetcherNodeConfig;
    fn client_type(&self) -> RetrieverClientType;

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
