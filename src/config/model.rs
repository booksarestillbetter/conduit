// src/config/model.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct AppConfig {
    #[serde(default)]
    pub system: SystemConfig,
    #[serde(default)]
    pub nodes: HashMap<String, FetcherNodeConfig>,
    #[serde(default)]
    pub sonarr: SonarrConfig,
    #[serde(default)]
    pub radarr: RadarrConfig,
    #[serde(default)]
    pub lidarr: LidarrConfig,
    #[serde(default)]
    pub plex: PlexConfig,
    #[serde(default)]
    pub trakt: TraktConfig,
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub influx: InfluxConfig,
    #[serde(default)]
    pub space_manager: SpaceManagerConfig,
    #[serde(default)]
    pub file_sync: FileSyncConfig,
    #[serde(default)]
    pub queue_routing: QueueRoutingConfig,
    #[serde(default)]
    pub rule_pipeline: RulePipelineConfig,
    #[serde(default)]
    pub ombi: OmbiConfig,
    #[serde(default)]
    pub bazarr: BazarrConfig,
    #[serde(default)]
    pub overseerr: OverseerrConfig,
    #[serde(default)]
    pub jellyfin: JellyfinConfig,
    #[serde(default)]
    pub tracker_circuit_breaker: TrackerCircuitBreakerConfig,
    /// Independent stacks ("zones") — e.g. a general library and a separate 4K library, each
    /// with its own Sonarr/Radarr/Lidarr/Plex and a subset of fetcher nodes. Additive and
    /// opt-in: empty by default, in which case every existing single-instance behavior (global
    /// webhook URLs, unfiltered dashboard/pipeline/fetchers views) is unchanged. Distinct from
    /// `sonarr.primary`/`.replicas`, which is a library-*mirroring* concept, not multi-tenancy —
    /// a zone's Sonarr/Radarr/Lidarr is never auto-synced with any other zone's.
    #[serde(default)]
    pub zones: Vec<ZoneConfig>,
    #[serde(default)]
    pub ip_asn: IpAsnConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ZoneConfig {
    /// Stable slug used as the routing key (e.g. "general", "4k") — this is what goes in the
    /// `?zone=` query param on each zone's webhook URLs.
    pub id: String,
    /// Display name (e.g. "General Library").
    pub name: String,
    #[serde(default)]
    pub sonarr: Option<ArrNodeConfig>,
    #[serde(default)]
    pub radarr: Option<ArrNodeConfig>,
    #[serde(default)]
    pub lidarr: Option<ArrNodeConfig>,
    /// References `PlexConfig.nodes[].name` — which of the configured Plex servers belong to
    /// this zone (for scoping the Plex notify-takeover to just this zone's server(s)).
    #[serde(default)]
    pub plex_node_names: Vec<String>,
    /// References keys of the top-level `nodes` (Transmission / qBittorrent / Deluge / Synapse
    /// fetchers) map.
    #[serde(default, rename = "fetcher_node_names", alias = "transmission_node_names")]
    pub fetcher_node_names: Vec<String>,
    /// One shared secret for this zone's Sonarr/Radarr/Lidarr inbound webhooks.
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SystemConfig {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,
    /// Reserved for the multi-user / RBAC milestone (see `TODO.md`) — not yet consulted
    /// anywhere. There is currently no registration endpoint at all for it to gate; the
    /// only way to create a user today is `POST /api/auth/setup`, which is itself only
    /// reachable once, before the first admin account exists.
    #[serde(default = "default_true")]
    pub enable_registration: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    /// Serve `/metrics` without credentials. Off by default: the endpoint lists node names and
    /// counts, so Prometheus should scrape it with a bearer token (an API token works). Turn this
    /// on only for a scraper that cannot send one.
    #[serde(default = "default_false")]
    pub metrics_public: bool,
    #[serde(default = "default_false")]
    pub ssl_enabled: bool,
    #[serde(default)]
    pub ssl_cert: Option<String>,
    #[serde(default)]
    pub ssl_key: Option<String>,
    /// Display name shown in the navbar wordmark and browser tab title. Purely cosmetic —
    /// doesn't affect the product/package identity, just what a given deployment's dashboard
    /// calls itself (e.g. a household naming their instance something other than "Conduit").
    #[serde(default = "default_dashboard_title")]
    pub dashboard_title: String,
    /// Optional sidebar quick-link to a media request portal (e.g. Ombi/Overseerr/Jellyseerr, or
    /// any other request-intake UI). Hidden from the sidebar entirely when unset. Default: unset.
    #[serde(default)]
    pub request_portal_url: Option<String>,
    /// Label shown for `request_portal_url`. Default: "Media Requests".
    #[serde(default = "default_request_portal_label")]
    pub request_portal_label: String,
    /// Optional sidebar quick-link to a media playback portal (Plex/Jellyfin/Emby, or any other
    /// front-end viewers use to actually watch/listen). Hidden from the sidebar entirely when
    /// unset. Default: unset.
    #[serde(default)]
    pub media_portal_url: Option<String>,
    /// Label shown for `media_portal_url`. Default: "Media Portal".
    #[serde(default = "default_media_portal_label")]
    pub media_portal_label: String,
}

fn default_bind_addr() -> String { "0.0.0.0".to_string() }
fn default_port() -> u16 { 3000 }
fn default_poll_interval() -> u64 { 2 }
fn default_jwt_secret() -> String { uuid::Uuid::new_v4().to_string() }
fn default_true() -> bool { true }
fn default_false() -> bool { false }
fn default_log_level() -> String { "info".to_string() }
fn default_data_dir() -> String { "data".to_string() }
fn default_dashboard_title() -> String { "Conduit".to_string() }
fn default_request_portal_label() -> String { "Media Requests".to_string() }
fn default_media_portal_label() -> String { "Media Portal".to_string() }

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            port: default_port(),
            poll_interval_secs: default_poll_interval(),
            jwt_secret: default_jwt_secret(),
            enable_registration: true,
            log_level: default_log_level(),
            data_dir: default_data_dir(),
            metrics_public: false,
            ssl_enabled: false,
            ssl_cert: None,
            ssl_key: None,
            dashboard_title: default_dashboard_title(),
            request_portal_url: None,
            request_portal_label: default_request_portal_label(),
            media_portal_url: None,
            media_portal_label: default_media_portal_label(),
        }
    }
}

pub use fetcher_core::config::{FetcherNodeConfig, RetrieverClientType};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SonarrConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_arr_interval")]
    pub sync_interval_mins: u64,
    #[serde(default, alias = "master")]
    pub primary: Option<ArrNodeConfig>,
    #[serde(default, alias = "slaves")]
    pub replicas: Vec<ArrNodeConfig>,
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

impl Default for SonarrConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sync_interval_mins: 60,
            primary: None,
            replicas: Vec::new(),
            webhook_secret: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RadarrConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_arr_interval")]
    pub sync_interval_mins: u64,
    #[serde(default, alias = "master")]
    pub primary: Option<ArrNodeConfig>,
    #[serde(default, alias = "slaves")]
    pub replicas: Vec<ArrNodeConfig>,
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

fn default_arr_interval() -> u64 { 60 }

impl Default for RadarrConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sync_interval_mins: 60,
            primary: None,
            replicas: Vec::new(),
            webhook_secret: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LidarrConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_arr_interval")]
    pub sync_interval_mins: u64,
    #[serde(default, alias = "master")]
    pub primary: Option<ArrNodeConfig>,
    #[serde(default, alias = "slaves")]
    pub replicas: Vec<ArrNodeConfig>,
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

impl Default for LidarrConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sync_interval_mins: 60,
            primary: None,
            replicas: Vec::new(),
            webhook_secret: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArrNodeConfig {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    #[serde(default)]
    pub base_path: Option<String>,
    #[serde(default = "default_arr_version")]
    pub version: u32,
}

fn default_arr_version() -> u32 { 3 }

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlexConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub nodes: Vec<PlexNodeConfig>,
    #[serde(default = "default_true")]
    pub refresh_on_sync: bool,
    #[serde(default = "default_true")]
    pub digest_scrobbles: bool,
    /// Conduit's own stable plex.tv client/device identity — generated once (see
    /// `ensure_plex_client_identifier`) and reused for every OAuth and direct-server call so
    /// Plex recognizes Conduit as one consistent registered device rather than a new one each time.
    #[serde(default)]
    pub client_identifier: String,
    /// When true, Conduit itself tells Plex to scan the specific folder on every Sonarr/Radarr
    /// import — intended to replace Sonarr/Radarr's own built-in Plex notification connection
    /// (which should be disabled on their end to avoid double-triggering scans).
    #[serde(default = "default_true")]
    pub notify_on_import: bool,
}

impl Default for PlexConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            nodes: Vec::new(),
            refresh_on_sync: true,
            digest_scrobbles: true,
            client_identifier: String::new(),
            notify_on_import: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlexNodeConfig {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub token_override: Option<String>,
    /// Opt this server into the Trakt-backed multi-server watch-status sync (see
    /// `engines::watch_sync`). Off by default — purely additive.
    #[serde(default)]
    pub sync_watch_status: bool,
    /// Plex's permanent machine identifier for this server, auto-detected and persisted the
    /// first sync cycle after `sync_watch_status` is enabled (see `plex_client::fetch_server_identity`)
    /// — never user-entered. Used to match an incoming webhook's `Server.uuid` back to this node.
    #[serde(default)]
    pub server_uuid: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct OmbiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct BazarrConfig {
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct OverseerrConfig {
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct JellyfinConfig {
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TraktConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub token_expiry: Option<i64>,
    #[serde(default = "default_trakt_interval")]
    pub sync_interval_mins: u64,
    /// Separate, off-by-default opt-in for the riskier direction of the watch-status sync:
    /// Conduit writing a "watched" status onto a live Plex server on its own initiative, based on
    /// a best-effort imdb/tmdb/tvdb ID match against that server's library (see
    /// `engines::watch_sync`). Pushing local scrobbles *to* Trakt only needs
    /// `plex.nodes[].sync_watch_status` and is unaffected by this flag.
    #[serde(default)]
    pub sync_watched_back_to_plex: bool,
}

fn default_trakt_interval() -> u64 { 360 }

impl Default for TraktConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            client_id: String::new(),
            client_secret: String::new(),
            access_token: None,
            refresh_token: None,
            token_expiry: None,
            sync_interval_mins: 360,
            sync_watched_back_to_plex: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct NotificationConfig {
    /// Deprecated — kept only so `ConfigManager::load_or_init`'s one-time migration can read a
    /// pre-0.11 config and synthesize an equivalent `NotificationTarget`. Never read by dispatch
    /// or written by the Settings UI once `targets` is populated.
    #[serde(default)]
    pub mattermost: Option<MattermostConfig>,
    #[serde(default)]
    pub discord: Option<DiscordConfig>,
    #[serde(default)]
    pub generic_webhook: Option<GenericWebhookConfig>,

    /// Independently configured notification destinations — the current model. Each target
    /// picks its own channel, which event categories it receives, and an optional zone scope.
    /// Purely additive: empty by default, populated once by the legacy-field migration above on
    /// first load of a pre-0.11 config.
    #[serde(default)]
    pub targets: Vec<NotificationTarget>,
}

/// One notification destination: a channel, which event categories it receives (empty = every
/// category), and an optional zone scope (`None` = global, fires for every event regardless of
/// zone). See `notify::CATEGORY_KEYS` for the valid category strings and
/// `notify::target_applies` for the selection logic.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NotificationTarget {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub channel: NotificationChannel,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub zone_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationChannel {
    Mattermost(MattermostConfig),
    Discord(DiscordConfig),
    Pushover(PushoverConfig),
    Webhook(GenericWebhookConfig),
}

/// Pushover (https://pushover.net) — a simple mobile-push channel, useful for a narrow
/// high-signal category (e.g. only `health`) rather than a full team chat integration.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PushoverConfig {
    pub user_key: String,
    pub api_token: String,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub priority: Option<i8>,
    #[serde(default)]
    pub sound: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MattermostConfig {
    pub enabled: bool,
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default = "default_bot_name")]
    pub bot_name: String,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub server_url: Option<String>,
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default = "default_true")]
    pub use_living_cards: bool,
    #[serde(default = "default_true")]
    pub use_threading: bool,
}

fn default_bot_name() -> String { "Conduit".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscordConfig {
    pub enabled: bool,
    pub webhook_url: String,
    #[serde(default = "default_bot_name")]
    pub username: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GenericWebhookConfig {
    pub enabled: bool,
    pub url: String,
    #[serde(default)]
    pub secret: Option<String>,
    /// `Summary` (default) sends today's minimal `{event, title, message, timestamp}` body.
    /// `Full` additionally includes whatever richer detail (overview, poster, fields) the
    /// dispatching call site has on hand, for an external system that wants to ingest Conduit's
    /// event stream rather than just get a human-readable blurb.
    #[serde(default)]
    pub payload_mode: PayloadMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayloadMode {
    #[default]
    Summary,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InfluxConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_influx_host")]
    pub host: String,
    #[serde(default = "default_influx_port")]
    pub port: u16,
    #[serde(default)]
    pub org: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub use_ssl: bool,
}

/// Opt-in peer IP enrichment (country + AS description) via iptoasn.com's free, license-free
/// IP-to-ASN database — deliberately not MaxMind GeoLite2, which requires an account/license.
/// Off by default since the parsed database is ~50-60MB resident in memory.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct IpAsnConfig {
    #[serde(default)]
    pub enabled: bool,
}

fn default_influx_host() -> String { "localhost".to_string() }
fn default_influx_port() -> u16 { 8086 }

impl Default for InfluxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: default_influx_host(),
            port: default_influx_port(),
            org: String::new(),
            bucket: String::new(),
            token: String::new(),
            use_ssl: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SpaceManagerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,
    #[serde(default = "default_min_space_gb")]
    pub global_min_space_gb: u64,
    #[serde(default = "default_max_purges")]
    pub max_purges_per_cycle: usize,
    #[serde(default = "default_target_ratio")]
    pub default_target_ratio: f64,
    #[serde(default = "default_target_age_days")]
    pub default_target_age_days: u64,
    #[serde(default = "default_target_seeds")]
    pub default_target_seeds: u64,
    #[serde(default = "default_required_match_count")]
    pub default_required_match_count: u32,
}

fn default_check_interval() -> u64 { 300 }
fn default_min_space_gb() -> u64 { 50 }
fn default_max_purges() -> usize { 5 }
fn default_target_ratio() -> f64 { 2.0 }
fn default_target_age_days() -> u64 { 14 }
fn default_target_seeds() -> u64 { 20 }
fn default_required_match_count() -> u32 { 2 }

impl Default for SpaceManagerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_secs: default_check_interval(),
            global_min_space_gb: default_min_space_gb(),
            max_purges_per_cycle: default_max_purges(),
            default_target_ratio: default_target_ratio(),
            default_target_age_days: default_target_age_days(),
            default_target_seeds: default_target_seeds(),
            default_required_match_count: default_required_match_count(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemoteSyncFolderMapping {
    pub id: String,
    pub name: String,
    pub watch_dir: String,
    pub post_dir: String,
    pub media_type: String,
    #[serde(default = "default_settle_time")]
    pub settle_time_secs: u64,
    #[serde(default)]
    pub delete_source_after_move: bool,
}

fn default_settle_time() -> u64 { 10 }

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FileSyncConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_file_sync_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_sync_cmd")]
    pub sync_cmd: String,
    #[serde(default)]
    pub mappings: Vec<RemoteSyncFolderMapping>,
    #[serde(default)]
    pub tv_post: Option<String>,
    #[serde(default)]
    pub movie_post: Option<String>,
    #[serde(default)]
    pub music_post: Option<String>,
    #[serde(default)]
    pub clean_queue_days: Option<u64>,
}

fn default_file_sync_interval() -> u64 { 30 }
fn default_sync_cmd() -> String { "cp -al".to_string() }

impl Default for FileSyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: default_file_sync_interval(),
            sync_cmd: default_sync_cmd(),
            mappings: Vec::new(),
            tv_post: None,
            movie_post: None,
            music_post: None,
            clean_queue_days: Some(30),
        }
    }
}

fn default_zero() -> i32 { 0 }

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MediaTypeDefinition {
    pub id: String,                    // "tv", "movie", "music", "anime", "books", "misc"
    pub name: String,                  // "Television Series"
    pub default_queue_dir: String,     // "/media/queue/tvQueue/"
    #[serde(default)]
    pub uhd_queue_dir: Option<String>, // "/media/queue/tvUHDqueue/"
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrackerMappingRule {
    pub id: String,
    pub pattern: String,               // e.g. "*broadcasthe.net*", "*ptp*", "*redacted.ch*"
    pub media_type: String,            // "tv", "movie", "music", etc.
    #[serde(default = "default_zero")]
    pub priority: i32,
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QueueTargetConfig {
    pub directory: String,
    #[serde(default = "default_true")]
    pub notify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QueueRoutingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_post_cmd")]
    pub post_cmd: String,
    #[serde(default = "default_media_type")]
    pub default_media_type: String,
    #[serde(default = "default_uhd_markers")]
    pub uhd_markers: Vec<String>,
    #[serde(default = "default_media_types")]
    pub media_types: Vec<MediaTypeDefinition>,
    #[serde(default = "default_tracker_mappings")]
    pub tracker_mappings: Vec<TrackerMappingRule>,
    #[serde(default = "default_queues")]
    pub queues: HashMap<String, QueueTargetConfig>,
    #[serde(default = "default_filename_rules")]
    pub filename_rules: HashMap<String, Vec<String>>,
    #[serde(default = "default_tracker_rules")]
    pub tracker_rules: HashMap<String, Vec<String>>,
}

fn default_post_cmd() -> String { "cp -av".to_string() }
fn default_media_type() -> String { "misc".to_string() }

fn default_uhd_markers() -> Vec<String> {
    vec!["UHD".to_string(), "2160p".to_string(), "4K".to_string(), "REMUX-2160p".to_string()]
}

fn default_media_types() -> Vec<MediaTypeDefinition> {
    vec![
        MediaTypeDefinition {
            id: "tv".to_string(),
            name: "Television Series".to_string(),
            default_queue_dir: "/media/queue/tvQueue/".to_string(),
            uhd_queue_dir: Some("/media/queue/tvUHDqueue/".to_string()),
            description: Some("Episodes, season packs, and daily releases".to_string()),
        },
        MediaTypeDefinition {
            id: "movie".to_string(),
            name: "Feature Films".to_string(),
            default_queue_dir: "/media/queue/movieQueue/".to_string(),
            uhd_queue_dir: Some("/media/queue/movieUHDqueue/".to_string()),
            description: Some("Theatrical, WEB-DL, and Blu-ray movies".to_string()),
        },
        MediaTypeDefinition {
            id: "music".to_string(),
            name: "Music & Audio".to_string(),
            default_queue_dir: "/media/queue/musicQueue/".to_string(),
            uhd_queue_dir: None,
            description: Some("Lossless FLAC, MP3, and discographies".to_string()),
        },
        MediaTypeDefinition {
            id: "anime".to_string(),
            name: "Anime".to_string(),
            default_queue_dir: "/media/queue/animeQueue/".to_string(),
            uhd_queue_dir: None,
            description: Some("Japanese anime episodes, movies, and BDs".to_string()),
        },
        MediaTypeDefinition {
            id: "books".to_string(),
            name: "E-Books & Audiobooks".to_string(),
            default_queue_dir: "/media/queue/booksQueue/".to_string(),
            uhd_queue_dir: None,
            description: Some("EPUB, PDF, and M4B audiobooks".to_string()),
        },
        MediaTypeDefinition {
            id: "misc".to_string(),
            name: "Miscellaneous / Unclassified".to_string(),
            default_queue_dir: "/media/queue/miscQueue/".to_string(),
            uhd_queue_dir: None,
            description: Some("Default fallback queue for unmatched media".to_string()),
        },
    ]
}

fn default_tracker_mappings() -> Vec<TrackerMappingRule> {
    vec![
        TrackerMappingRule {
            id: "rule_lotv".to_string(),
            pattern: "*landof.tv*".to_string(),
            media_type: "tv".to_string(),
            priority: 15,
            comment: Some("LandOf.tv TV Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_bitmetv".to_string(),
            pattern: "*bitmetv*".to_string(),
            media_type: "tv".to_string(),
            priority: 10,
            comment: Some("BitMeTV Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_btn".to_string(),
            pattern: "*broadcasthe*".to_string(),
            media_type: "tv".to_string(),
            priority: 10,
            comment: Some("BroadcastTheNet TV Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_mttv".to_string(),
            pattern: "*morethantv*".to_string(),
            media_type: "tv".to_string(),
            priority: 10,
            comment: Some("MoreThanTV Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_ptp".to_string(),
            pattern: "*passthepopcorn*".to_string(),
            media_type: "movie".to_string(),
            priority: 10,
            comment: Some("PassThePopcorn Movie Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_hdb".to_string(),
            pattern: "*hdbits*".to_string(),
            media_type: "movie".to_string(),
            priority: 10,
            comment: Some("HDBits HD/UHD Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_ahd".to_string(),
            pattern: "*awesome-hd*".to_string(),
            media_type: "movie".to_string(),
            priority: 10,
            comment: Some("Awesome-HD Movie Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_red".to_string(),
            pattern: "*redacted*".to_string(),
            media_type: "music".to_string(),
            priority: 10,
            comment: Some("Redacted Music Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_ops".to_string(),
            pattern: "*orpheus*".to_string(),
            media_type: "music".to_string(),
            priority: 10,
            comment: Some("Orpheus Music Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_ops2".to_string(),
            pattern: "*opsfet*".to_string(),
            media_type: "music".to_string(),
            priority: 10,
            comment: Some("OPS Music Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_flacs".to_string(),
            pattern: "*flacsfor.me*".to_string(),
            media_type: "music".to_string(),
            priority: 10,
            comment: Some("Redacted Announce Host".to_string()),
        },
        TrackerMappingRule {
            id: "rule_tmb".to_string(),
            pattern: "*themixingbowl*".to_string(),
            media_type: "music".to_string(),
            priority: 10,
            comment: Some("TheMixingBowl Music Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_waffles".to_string(),
            pattern: "*waffles*".to_string(),
            media_type: "music".to_string(),
            priority: 10,
            comment: Some("Waffles Music Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_rutracker".to_string(),
            pattern: "*rutracker*".to_string(),
            media_type: "music".to_string(),
            priority: 5,
            comment: Some("RuTracker Music/Audio".to_string()),
        },
        TrackerMappingRule {
            id: "rule_ab".to_string(),
            pattern: "*animebytes*".to_string(),
            media_type: "anime".to_string(),
            priority: 10,
            comment: Some("AnimeBytes Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_nyaa".to_string(),
            pattern: "*nyaa*".to_string(),
            media_type: "anime".to_string(),
            priority: 10,
            comment: Some("Nyaa Anime Tracker".to_string()),
        },
        TrackerMappingRule {
            id: "rule_mam".to_string(),
            pattern: "*myanonamouse*".to_string(),
            media_type: "books".to_string(),
            priority: 10,
            comment: Some("MyAnonaMouse E-Book Tracker".to_string()),
        },
    ]
}

fn default_queues() -> HashMap<String, QueueTargetConfig> {
    let mut map = HashMap::new();
    map.insert("tv".to_string(), QueueTargetConfig { directory: "/media/queue/tvQueue/".to_string(), notify: true });
    map.insert("tvUHD".to_string(), QueueTargetConfig { directory: "/media/queue/tvUHDqueue/".to_string(), notify: true });
    map.insert("movie".to_string(), QueueTargetConfig { directory: "/media/queue/movieQueue/".to_string(), notify: true });
    map.insert("movieUHD".to_string(), QueueTargetConfig { directory: "/media/queue/movieUHDqueue/".to_string(), notify: true });
    map.insert("music".to_string(), QueueTargetConfig { directory: "/media/queue/musicQueue/".to_string(), notify: true });
    map.insert("anime".to_string(), QueueTargetConfig { directory: "/media/queue/animeQueue/".to_string(), notify: true });
    map.insert("books".to_string(), QueueTargetConfig { directory: "/media/queue/booksQueue/".to_string(), notify: true });
    map.insert("misc".to_string(), QueueTargetConfig { directory: "/media/queue/miscQueue/".to_string(), notify: true });
    map
}

fn default_filename_rules() -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    map.insert("tv".to_string(), vec![
        r"S\d{2}E\d{2}".to_string(),
        r"\d{1,2}x\d{1,2}".to_string(),
        r"\d{4}\.\d{2}\.\d{2}".to_string(),
    ]);
    map.insert("music".to_string(), vec![
        r"\bFLAC\b".to_string(),
        r"\bMP3\b".to_string(),
        r"320kbps".to_string(),
        r"\bV0\b".to_string(),
        r"\bLossless\b".to_string(),
        r"\b24bit\b".to_string(),
        r"\bALAC\b".to_string(),
        r"\bAAC\b".to_string(),
        r"\bWAV\b".to_string(),
    ]);
    map
}

fn default_tracker_rules() -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    map.insert("tv".to_string(), vec![
        "landof.tv".to_string(),
        "bitmetv.org".to_string(),
        "morethantv.me".to_string(),
        "broadcasthe.net".to_string(),
        "btn".to_string(),
        "lotv".to_string(),
        "mtv".to_string(),
    ]);
    map.insert("movie".to_string(), vec![
        "passthepopcorn.me".to_string(),
        "awesome-hd.me".to_string(),
        "hdbits.org".to_string(),
        "ptp".to_string(),
        "ahd".to_string(),
        "hdb".to_string(),
    ]);
    map.insert("music".to_string(), vec![
        "orpheus.network".to_string(),
        "opsfet.ch".to_string(),
        "themixingbowl.org".to_string(),
        "redacted.ch".to_string(),
        "flacsfor.me".to_string(),
        "red".to_string(),
        "ops".to_string(),
        "orpheus".to_string(),
        "rutracker.org".to_string(),
        "waffles.ch".to_string(),
    ]);
    map.insert("anime".to_string(), vec![
        "animebytes.tv".to_string(),
        "nyaa.si".to_string(),
        "tokyotosho.info".to_string(),
        "ab".to_string(),
    ]);
    map.insert("books".to_string(), vec![
        "myanonamouse.net".to_string(),
        "mam".to_string(),
    ]);
    map.insert("misc".to_string(), vec![
        "alpharatio.cc".to_string(),
        "milkie.cc".to_string(),
    ]);
    map
}

impl Default for QueueRoutingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            post_cmd: default_post_cmd(),
            default_media_type: default_media_type(),
            uhd_markers: default_uhd_markers(),
            media_types: default_media_types(),
            tracker_mappings: default_tracker_mappings(),
            queues: default_queues(),
            filename_rules: default_filename_rules(),
            tracker_rules: default_tracker_rules(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RulePipelineConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub auto_delete_unregistered: bool,
    #[serde(default = "default_unregistered_pattern")]
    pub unregistered_pattern: String,
    #[serde(default = "default_true")]
    pub auto_trigger_media_replacer: bool,
    #[serde(default = "default_true")]
    pub auto_fix_missing_data: bool,
}

fn default_unregistered_pattern() -> String {
    r"(?i)unregistered|torrent deleted|not found|torrent not registered|no data found|ensure your drives are connected|verify local data".to_string()
}

impl Default for RulePipelineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_delete_unregistered: true,
            unregistered_pattern: default_unregistered_pattern(),
            auto_trigger_media_replacer: true,
            auto_fix_missing_data: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrackerCircuitBreakerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub canary_probe_enabled: bool,
    #[serde(default = "default_true")]
    pub auto_resume_on_recovery: bool,
    /// Minimum percentage (0.0 to 1.0) of a tracker's torrents that must fail before tripping breaker (default: 0.5 = 50%)
    #[serde(default = "default_cb_failure_ratio_threshold")]
    pub failure_ratio_threshold: f64,
    /// Minimum number of torrents failing before tripping breaker (default: 3)
    #[serde(default = "default_cb_min_failures")]
    pub min_failures: usize,
    #[serde(default = "default_cb_error_patterns")]
    pub error_patterns: Vec<String>,
    #[serde(default = "default_cb_check_interval")]
    pub check_interval_secs: u64,
    /// Hard ceiling on how long a breaker can stay tripped before it is force-recovered
    /// regardless of canary announce state — guards against getting stuck open forever if
    /// the canary torrent loses its tracker-stats entry (list edited, stale stat dropped).
    #[serde(default = "default_cb_max_tripped_secs")]
    pub max_tripped_secs: i64,
    /// Initial cooldown after tripping before the canary's recovery announce is even
    /// checked, doubling on relapse (capped at `max_tripped_secs`) — mirrors Synapse's
    /// tracker breaker backoff.
    #[serde(default = "default_cb_initial_backoff_secs")]
    pub initial_backoff_secs: i64,
    /// How long, after a successful canary recovery, to stay in the `Recovering` ramp
    /// phase (staggering resumption of paused torrents) before fully clearing the breaker.
    #[serde(default = "default_cb_recovery_ramp_secs")]
    pub recovery_ramp_secs: u64,
    /// Consecutive successful canary checks required during the ramp window before full
    /// graduation to healthy (in addition to the ramp window itself elapsing).
    #[serde(default = "default_cb_recovery_success_threshold")]
    pub recovery_success_threshold: u32,
}

fn default_cb_check_interval() -> u64 { 20 }
fn default_cb_max_tripped_secs() -> i64 { 6 * 3600 }
fn default_cb_initial_backoff_secs() -> i64 { 30 }
fn default_cb_recovery_ramp_secs() -> u64 { 30 }
fn default_cb_recovery_success_threshold() -> u32 { 5 }
fn default_cb_failure_ratio_threshold() -> f64 { 0.50 }
fn default_cb_min_failures() -> usize { 3 }
fn default_cb_error_patterns() -> Vec<String> {
    vec![
        "530".to_string(),
        "502".to_string(),
        "503".to_string(),
        "504".to_string(),
        "403".to_string(),
        "429".to_string(),
        "The tracker is down".to_string(),
        "tracker is down".to_string(),
        "Connection refused".to_string(),
        "Could not connect".to_string(),
        "Timed out".to_string(),
        "Host not found".to_string(),
        "unreachable".to_string(),
        "Unknown Error".to_string(),
    ]
}

impl Default for TrackerCircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            canary_probe_enabled: true,
            auto_resume_on_recovery: true,
            failure_ratio_threshold: default_cb_failure_ratio_threshold(),
            min_failures: default_cb_min_failures(),
            error_patterns: default_cb_error_patterns(),
            check_interval_secs: default_cb_check_interval(),
            max_tripped_secs: default_cb_max_tripped_secs(),
            initial_backoff_secs: default_cb_initial_backoff_secs(),
            recovery_ramp_secs: default_cb_recovery_ramp_secs(),
            recovery_success_threshold: default_cb_recovery_success_threshold(),
        }
    }
}
