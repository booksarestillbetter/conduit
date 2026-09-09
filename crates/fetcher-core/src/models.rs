use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use crate::config::RetrieverClientType;

/// Feature capabilities detected on a fetcher node's daemon at runtime, distinct from the
/// static `client_type` config choice — a node configured as `Synapse` still reports
/// `native_tracker_circuit_breaker: false` if it's running an older build that predates the
/// feature. Deliberately not persisted anywhere; re-detected on every poll cycle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NodeCapabilities {
    pub native_tracker_circuit_breaker: bool,
}

/// A single tracker host's live circuit breaker status, as reported by a node's own
/// native breaker (only meaningful for nodes where `NodeCapabilities::
/// native_tracker_circuit_breaker` is true — see `TorrentClientTrait::list_circuit_breakers`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NativeCircuitBreakerStatus {
    pub host: String,
    /// `"healthy"`, `"tripped"`, `"half_open_canary"`, or `"recovering"`.
    pub state: String,
    pub consecutive_successes: u32,
    pub consecutive_failures: u32,
    pub backoff_remaining_ms: u64,
    pub recovery_progress_pct: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TorrentStatus {
    Stopped,
    CheckWait,
    Checking,
    Queued,
    Downloading,
    QueuedSeed,
    Seeding,
    Idle,
    Error,
    Deleted,
    Unknown,
}

impl From<i64> for TorrentStatus {
    fn from(val: i64) -> Self {
        match val {
            0 => TorrentStatus::Stopped,
            1 => TorrentStatus::CheckWait,
            2 => TorrentStatus::Checking,
            3 => TorrentStatus::Queued,
            4 => TorrentStatus::Downloading,
            5 => TorrentStatus::QueuedSeed,
            6 => TorrentStatus::Seeding,
            _ => TorrentStatus::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrackerStat {
    #[serde(default)]
    pub announce: String,
    #[serde(default)]
    pub host: String,
    #[serde(default, rename = "seederCount")]
    pub seeder_count: i64,
    #[serde(default, rename = "leecherCount")]
    pub leecher_count: i64,
    #[serde(default, rename = "downloadCount")]
    pub download_count: i64,
    #[serde(default, rename = "lastAnnounceSucceeded")]
    pub last_announce_succeeded: bool,
    #[serde(default, rename = "lastAnnounceResult")]
    pub last_announce_result: String,
    #[serde(default, rename = "isBackup")]
    pub is_backup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TorrentFile {
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "bytesCompleted")]
    pub bytes_completed: i64,
    #[serde(default)]
    pub length: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Torrent {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "hashString")]
    pub hash_string: String,
    #[serde(default)]
    pub status: i64,
    #[serde(default, rename = "rateUpload")]
    pub rate_upload: i64,
    #[serde(default, rename = "rateDownload")]
    pub rate_download: i64,
    #[serde(default, rename = "uploadedEver")]
    pub uploaded_ever: i64,
    #[serde(default, rename = "downloadedEver")]
    pub downloaded_ever: i64,
    #[serde(default, rename = "uploadRatio")]
    pub upload_ratio: f64,
    #[serde(default, rename = "totalSize")]
    pub total_size: i64,
    #[serde(default, rename = "sizeWhenDone")]
    pub size_when_done: i64,
    #[serde(default, rename = "leftUntilDone")]
    pub left_until_done: i64,
    #[serde(default, rename = "percentDone")]
    pub percent_done: f64,
    #[serde(default)]
    pub eta: i64,
    #[serde(default, rename = "etaIdle")]
    pub eta_idle: Option<i64>,
    #[serde(default)]
    pub error: i64,
    #[serde(default, rename = "errorString")]
    pub error_string: String,
    #[serde(default, rename = "peersConnected")]
    pub peers_connected: i64,
    #[serde(default, rename = "peersSendingToUs")]
    pub peers_sending_to_us: i64,
    #[serde(default, rename = "peersGettingFromUs")]
    pub peers_getting_from_us: i64,
    #[serde(default, rename = "addedDate")]
    pub added_date: i64,
    #[serde(default, rename = "doneDate")]
    pub done_date: i64,
    #[serde(default, rename = "downloadDir")]
    pub download_dir: String,
    #[serde(default, rename = "trackerStats")]
    pub tracker_stats: Vec<TrackerStat>,
    #[serde(default)]
    pub files: Option<Vec<TorrentFile>>,
    #[serde(default)]
    pub peers: Option<Vec<TorrentPeer>>,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub creator: Option<String>,
    #[serde(default, rename = "dateCreated")]
    pub date_created: Option<i64>,
    #[serde(default, rename = "pieceCount")]
    pub piece_count: Option<i64>,
    #[serde(default, rename = "pieceSize")]
    pub piece_size: Option<i64>,
    #[serde(default, rename = "isPrivate")]
    pub is_private: Option<bool>,
    #[serde(default, rename = "magnetLink")]
    pub magnet_link: Option<String>,
    #[serde(default, rename = "corruptEver")]
    pub corrupt_ever: Option<i64>,
    #[serde(default, rename = "secondsDownloading")]
    pub seconds_downloading: Option<i64>,
    #[serde(default, rename = "secondsSeeding")]
    pub seconds_seeding: Option<i64>,
    #[serde(default, rename = "activityDate")]
    pub activity_date: Option<i64>,
    #[serde(default, rename = "queuePosition")]
    pub queue_position: i64,
    #[serde(default, rename = "sequentialDownload")]
    pub sequential_download: bool,
    #[serde(default)]
    pub pieces: Option<String>,
    #[serde(default)]
    pub availability: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TorrentPeer {
    #[serde(default)]
    pub address: String,
    #[serde(default, rename = "clientName")]
    pub client_name: String,
    #[serde(default, rename = "clientIsChoked")]
    pub client_is_choked: bool,
    #[serde(default, rename = "clientIsInterested")]
    pub client_is_interested: bool,
    #[serde(default)]
    pub flagstr: String,
    #[serde(default, rename = "isDownloadingFrom")]
    pub is_downloading_from: bool,
    #[serde(default, rename = "isEncrypted")]
    pub is_encrypted: bool,
    #[serde(default, rename = "isIncoming")]
    pub is_incoming: bool,
    #[serde(default, rename = "isUploadingTo")]
    pub is_uploading_to: bool,
    #[serde(default, rename = "isUtp")]
    pub is_utp: bool,
    #[serde(default, rename = "peerIsChoked")]
    pub peer_is_choked: bool,
    #[serde(default, rename = "peerIsInterested")]
    pub peer_is_interested: bool,
    #[serde(default)]
    pub port: i64,
    #[serde(default)]
    pub progress: f64,
    #[serde(default, rename = "rateToClient")]
    pub rate_to_client: i64,
    #[serde(default, rename = "rateToPeer")]
    pub rate_to_peer: i64,
    #[serde(default, rename = "countryCode")]
    pub country_code: Option<String>,
    #[serde(default, rename = "asName")]
    pub as_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TorrentTimelineEvent {
    pub stage: String,
    pub title: String,
    pub description: String,
    pub timestamp: i64,
    pub formatted_time: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeStats {
    pub node: String,
    #[serde(default)]
    pub client_type: RetrieverClientType,
    pub connected: bool,
    pub latency_ms: u64,
    pub free_space_bytes: i64,
    pub free_space_gb: f64,
    pub total_torrents: usize,
    pub active_torrents: usize,
    pub paused_torrents: usize,
    pub error_torrents: usize,
    pub rate_upload: i64,
    pub rate_download: i64,
    pub ratio_0: usize,
    pub ratio_lt1: usize,
    pub ratio_gt1: usize,
    pub ratio_gt2: usize,
    #[serde(default)]
    pub alt_speed_enabled: bool,
    #[serde(default)]
    pub blocklist_size: i64,
    #[serde(default)]
    pub blocklist_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TorrentBandwidthContributor {
    pub id: String,
    pub name: String,
    pub node: String,
    #[serde(rename = "downloadSpeed")]
    pub download_speed: i64,
    #[serde(rename = "uploadSpeed")]
    pub upload_speed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BandwidthPoint {
    pub time: i64,
    #[serde(rename = "downloadSpeed")]
    pub download_speed: i64,
    #[serde(rename = "uploadSpeed")]
    pub upload_speed: i64,
    #[serde(rename = "topTorrents", default, skip_serializing_if = "Vec::is_empty")]
    pub top_torrents: Vec<TorrentBandwidthContributor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AggregateStats {
    pub total_nodes: usize,
    pub connected_nodes: usize,
    pub total_torrents: usize,
    pub total_download_speed: i64,
    pub total_upload_speed: i64,
    pub total_size_bytes: i64,
    pub nodes: Vec<NodeStats>,
    #[serde(default)]
    pub bandwidth_history: Vec<BandwidthPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddTorrentPayload {
    pub node: String,
    pub magnet_or_url: Option<String>,
    pub metainfo_base64: Option<String>,
    pub download_dir: Option<String>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub sequential_download: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BulkTorrentActionPayload {
    pub compound_ids: Vec<String>,
    pub action: BulkActionType,
    #[serde(default)]
    pub delete_local_data: bool,
    pub target_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BulkItemResult {
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BulkActionType {
    Start,
    StartNow,
    Stop,
    Verify,
    Reannounce,
    Delete,
    SetLocation,
}
