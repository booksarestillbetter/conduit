// src/fetcher/models.rs
pub use fetcher_core::models::*;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UnifiedTorrent {
    pub compound_id: String, // format: "{node}:{id}"
    pub node: String,
    pub id: i64,
    pub name: String,
    pub hash_string: String,
    pub status: TorrentStatus,
    pub raw_status: i64,
    pub rate_upload: i64,
    pub rate_download: i64,
    pub uploaded_ever: i64,
    pub downloaded_ever: i64,
    pub upload_ratio: f64,
    pub total_size: i64,
    pub size_when_done: i64,
    pub left_until_done: i64,
    pub percent_done: f64,
    pub eta: i64,
    pub error: i64,
    pub error_string: String,
    pub peers_connected: i64,
    pub peers_sending_to_us: i64,
    pub peers_getting_from_us: i64,
    pub added_date: i64,
    pub done_date: i64,
    pub download_dir: String,
    pub tracker_stats: Vec<TrackerStat>,
    pub files: Option<Vec<TorrentFile>>,
    #[serde(default)]
    pub queue_position: i64,
    #[serde(default)]
    pub sequential_download: bool,
    #[serde(default)]
    pub arr_grab: Option<crate::db::ArrGrabRecord>,
    #[serde(default)]
    pub is_circuit_broken: bool,
    #[serde(default)]
    pub circuit_breaker_reason: Option<String>,
    #[serde(default)]
    pub is_canary_probe: bool,
}

impl UnifiedTorrent {
    pub fn from_torrent(node: &str, t: Torrent) -> Self {
        let mut status = TorrentStatus::from(t.status);
        if t.status == 0 {
            status = TorrentStatus::Stopped;
        } else if t.error == 2 || t.error == 3 {
            status = TorrentStatus::Error;
        } else if t.peers_sending_to_us == 0 && status == TorrentStatus::Downloading {
            status = TorrentStatus::Idle;
        }

        Self {
            compound_id: format!("{}:{}", node, t.id),
            node: node.to_string(),
            id: t.id,
            name: t.name,
            hash_string: t.hash_string,
            status,
            raw_status: t.status,
            rate_upload: t.rate_upload,
            rate_download: t.rate_download,
            uploaded_ever: t.uploaded_ever,
            downloaded_ever: t.downloaded_ever,
            upload_ratio: t.upload_ratio,
            total_size: t.total_size,
            size_when_done: t.size_when_done,
            left_until_done: t.left_until_done,
            percent_done: t.percent_done,
            eta: t.eta,
            error: t.error,
            error_string: t.error_string,
            peers_connected: t.peers_connected,
            peers_sending_to_us: t.peers_sending_to_us,
            peers_getting_from_us: t.peers_getting_from_us,
            added_date: t.added_date,
            done_date: t.done_date,
            download_dir: t.download_dir,
            tracker_stats: t.tracker_stats,
            files: t.files,
            queue_position: t.queue_position,
            sequential_download: t.sequential_download,
            arr_grab: None,
            is_circuit_broken: false,
            circuit_breaker_reason: None,
            is_canary_probe: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DetailedTorrent {
    #[serde(flatten)]
    pub unified: UnifiedTorrent,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub creator: String,
    #[serde(default)]
    pub date_created: i64,
    #[serde(default)]
    pub piece_count: i64,
    #[serde(default)]
    pub piece_size: i64,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub magnet_link: String,
    #[serde(default)]
    pub corrupt_ever: i64,
    #[serde(default)]
    pub seconds_downloading: i64,
    #[serde(default)]
    pub seconds_seeding: i64,
    #[serde(default)]
    pub activity_date: i64,
    #[serde(default)]
    pub queue_position: i64,
    #[serde(default)]
    pub sequential_download: bool,
    #[serde(default)]
    pub pieces: Option<String>,
    #[serde(default)]
    pub availability: Option<Vec<i64>>,
    #[serde(default)]
    pub peers: Vec<TorrentPeer>,
    #[serde(default)]
    pub timeline: Vec<TorrentTimelineEvent>,
    #[serde(default)]
    pub arr_grab: Option<crate::db::ArrGrabRecord>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct ActiveCircuitBreaker {
    pub tracker_host: String,
    pub canary_compound_id: String,
    pub canary_name: String,
    pub paused_torrents: Vec<String>,
    pub failing_error: String,
    pub tripped_at: i64,
    /// `"tripped"`, `"half_open_canary"`, or `"recovering"` — absent/default (empty string)
    /// is treated as `"tripped"` for records written before this field existed.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub recovery_started_at: Option<i64>,
    #[serde(default)]
    pub consecutive_successes: u32,
    /// Current re-trip cooldown, doubling on relapse (capped at
    /// `TrackerCircuitBreakerConfig::max_tripped_secs`). `0` means "use
    /// `initial_backoff_secs`" (records written before this field existed).
    #[serde(default)]
    pub backoff_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeHealthStatus {
    pub name: String,
    pub connected: bool,
    pub latency_ms: u64,
    pub status: String,
    pub last_error: Option<String>,
    pub total_torrents: usize,
    pub active_torrents: usize,
    pub paused_torrents: usize,
    pub error_torrents: usize,
    pub free_space_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrackerHealthStatus {
    pub host: String,
    pub status: String,
    pub total_torrents: usize,
    pub online_torrents: usize,
    pub error_torrents: usize,
    pub error_ratio: f64,
    pub health_tier: String,
    pub paused_torrents: usize,
    pub active_probe_id: Option<String>,
    pub active_probe_name: Option<String>,
    pub last_announce_result: String,
    pub last_announce_succeeded: bool,
    pub affected_nodes: Vec<String>,
    pub is_circuit_broken: bool,
    /// `"active"` (Conduit is managing this tracker's breaker itself) or `"passive"`
    /// (a node's own native breaker is authoritative and Conduit is just mirroring its
    /// status). Defaults to `"active"` for backward compatibility with older responses.
    #[serde(default = "default_breaker_mode")]
    pub breaker_mode: String,
    /// `"tripped"`, `"half_open_canary"`, or `"recovering"` — mirrors `ActiveCircuitBreaker`
    /// in active mode, or the owning node's native breaker state in passive mode.
    #[serde(default)]
    pub cb_state: Option<String>,
    #[serde(default)]
    pub recovery_progress_pct: Option<f32>,
    #[serde(default)]
    pub consecutive_successes: Option<u32>,
}

fn default_breaker_mode() -> String {
    "active".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SystemHealthOverview {
    pub overall_status: String,
    pub total_nodes: usize,
    pub connected_nodes: usize,
    pub total_torrents: usize,
    pub total_trackers: usize,
    pub circuit_broken_trackers: usize,
    pub nodes: Vec<NodeHealthStatus>,
    pub trackers: Vec<TrackerHealthStatus>,
}
