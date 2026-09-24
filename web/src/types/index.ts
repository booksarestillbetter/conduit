// web/src/types/index.ts
declare const __APP_VERSION__: string | undefined;
export const APP_VERSION = typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '0.4.0';

export type TorrentStatus = 
  | 'stopped'
  | 'checkwait'
  | 'checking'
  | 'queued'
  | 'downloading'
  | 'queuedseed'
  | 'seeding'
  | 'idle'
  | 'error'
  | 'deleted'
  | 'unknown';

export interface TrackerStat {
  announce: string;
  host: string;
  seederCount: number;
  leecherCount: number;
  downloadCount: number;
  lastAnnounceSucceeded: boolean;
  last_announce_result: string;
  isBackup: boolean;
}

export interface TorrentFile {
  name: string;
  bytesCompleted: number;
  length: number;
}

export interface UnifiedTorrent {
  compound_id: string; // "node:id"
  node: string;
  id: number;
  name: string;
  hash_string: string;
  status: TorrentStatus;
  raw_status: number;
  rate_upload: number;
  rate_download: number;
  uploaded_ever: number;
  downloaded_ever: number;
  upload_ratio: number;
  total_size: number;
  size_when_done: number;
  left_until_done: number;
  percent_done: number;
  eta: number;
  error: number;
  error_string: string;
  peers_connected: number;
  peers_sending_to_us: number;
  peers_getting_from_us: number;
  added_date: number;
  done_date: number;
  download_dir: string;
  tracker_stats: TrackerStat[];
  files?: TorrentFile[];
  queue_position?: number;
  sequential_download?: boolean;
  arr_grab?: ArrGrabRecord;
  is_circuit_broken?: boolean;
  circuit_breaker_reason?: string;
  is_canary_probe?: boolean;
}

export interface NodeStats {
  node: string;
  client_type?: 'transmission' | 'qbittorrent' | 'deluge' | 'synapse';
  connected: boolean;
  latency_ms: number;
  free_space_bytes: number;
  free_space_gb: number;
  total_torrents: number;
  active_torrents: number;
  paused_torrents: number;
  error_torrents: number;
  rate_upload: number;
  rate_download: number;
  ratio_0: number;
  ratio_lt1: number;
  ratio_gt1: number;
  ratio_gt2: number;
  alt_speed_enabled?: boolean;
  blocklist_size?: number;
  blocklist_enabled?: boolean;
}

export interface TorrentBandwidthContributor {
  id: string;
  name: string;
  node: string;
  downloadSpeed: number;
  uploadSpeed: number;
}

export interface PeerBandwidthContributor {
  address: string;
  clientName: string;
  flagStr?: string;
  downloadSpeed: number;
  uploadSpeed: number;
  progress?: number;
  isEncrypted?: boolean;
  countryCode?: string;
  asName?: string;
}

export interface BandwidthDataPoint {
  time: number;
  downloadSpeed: number;
  uploadSpeed: number;
  topTorrents?: TorrentBandwidthContributor[];
  topPeers?: PeerBandwidthContributor[];
}

export interface AggregateStats {
  total_nodes: number;
  connected_nodes: number;
  total_torrents: number;
  total_download_speed: number;
  total_upload_speed: number;
  total_size_bytes: number;
  nodes: NodeStats[];
  bandwidth_history?: BandwidthDataPoint[];
}

export interface NodeHealthStatus {
  name: string;
  connected: boolean;
  latency_ms: number;
  status: 'healthy' | 'unreachable' | 'degraded';
  last_error?: string | null;
  total_torrents: number;
  active_torrents: number;
  paused_torrents: number;
  error_torrents: number;
  free_space_gb: number;
}

export interface TrackerHealthStatus {
  host: string;
  status: 'healthy' | 'nominal' | 'warning' | 'critical' | 'circuit_broken';
  total_torrents: number;
  online_torrents?: number;
  error_torrents?: number;
  error_ratio?: number;
  health_tier?: 'healthy' | 'nominal' | 'warning' | 'critical' | 'circuit_broken';
  paused_torrents: number;
  active_probe_id?: string | null;
  active_probe_name?: string | null;
  last_announce_result: string;
  last_announce_succeeded: boolean;
  affected_nodes: string[];
  is_circuit_broken: boolean;
  /** 'active' = Conduit manages this tracker's breaker itself; 'passive' = a node's own
   * native breaker is authoritative and Conduit just mirrors its status. */
  breaker_mode: 'active' | 'passive';
  /** Raw 4-state breaker vocabulary ('healthy' | 'tripped' | 'half_open_canary' |
   * 'recovering'), present in both modes — distinct from `status`/`health_tier`, which
   * stay in the older 5-tier vocabulary for backward compatibility with existing UI. */
  cb_state?: string | null;
  /** 0-100, only set while `cb_state` is 'recovering'. */
  recovery_progress_pct?: number | null;
  consecutive_successes?: number | null;
}

export interface SystemHealthOverview {
  overall_status: 'healthy' | 'warning' | 'degraded';
  total_nodes: number;
  connected_nodes: number;
  total_torrents: number;
  total_trackers: number;
  circuit_broken_trackers: number;
  nodes: NodeHealthStatus[];
  trackers: TrackerHealthStatus[];
}

/** "Platform Health" — Conduit's own internal background engines/pollers/schedulers, distinct
 * from Arr-stack or fetcher-node health above. */
export interface EngineStatus {
  name: string;
  state: 'starting' | 'running' | 'degraded' | 'crashed';
  last_tick_at: string | null;
  last_error: string | null;
  tick_count: number;
  restart_count: number;
}

export interface UserRecord {
  id: string;
  username: string;
  is_admin: boolean;
  totp_enabled?: boolean;
  created_at: string;
}

export interface Setup2FaResponse {
  secret: string;
  otpauth_url: string;
}

export interface ApiTokenRecord {
  id: string;
  name: string;
  scopes: string[];
  created_at: string;
  expires_at?: string;
  last_used_at?: string;
}

export interface EventLogRecord {
  id: number;
  event_type: string;
  level: string;
  message: string;
  details_json?: string;
  created_at: string;
}

export interface ArrGrabRecord {
  id: string;
  scene_name: string;
  release_title: string;
  event_type: string;
  item_type: string;
  series_id?: number;
  movie_id?: number;
  artist_id?: number;
  album_id?: number;
  season_number?: number;
  episode_numbers?: string;
  episode_ids?: string;
  indexer?: string;
  download_client?: string;
  download_id?: string;
  status: string;
  re_searched: boolean;
  re_search_count: number;
  title?: string;
  year?: number;
  overview?: string;
  poster_url?: string;
  genres?: string;
  quality?: string;
  size_bytes?: number;
  imdb_id?: string;
  tmdb_id?: number;
  tvdb_id?: number;
  runtime_mins?: number;
  rating?: number;
  mattermost_post_id?: string;
  payload_json?: string;
  created_at: string;
  updated_at: string;
  /** Which ZoneConfig.id this grab was tagged with, if any. Absent for pre-0.8.0 grabs
   * and for grabs that arrived on an un-zoned webhook URL. */
  zone_id?: string;
}

/** One append-only timeline entry for a grab's full history (grabbed, imported, deleted,
 * re-searched, re-imported, etc.) — distinct from ArrGrabRecord, which only ever reflects
 * current state since multiple events for the same media item collapse onto one row there. */
export interface ArrGrabHistoryEntry {
  id: number;
  grab_id: string;
  event_type: string;
  status: string;
  release_title?: string;
  scene_name?: string;
  quality?: string;
  size_bytes?: number;
  indexer?: string;
  download_client?: string;
  download_id?: string;
  payload_json?: string;
  created_at: string;
}

export interface SonarrLiveStats {
  version?: string;
  series_count: number;
  monitored_series_count: number;
  episode_count: number;
  episode_file_count: number;
  missing_episodes_count: number;
  queue_count: number;
  size_on_disk_bytes: number;
  health_issues: string[];
  disk_free_bytes?: number;
  disk_total_bytes?: number;
}

export interface RadarrLiveStats {
  version?: string;
  movie_count: number;
  monitored_movie_count: number;
  movie_file_count: number;
  missing_movies_count: number;
  queue_count: number;
  size_on_disk_bytes: number;
  health_issues: string[];
  disk_free_bytes?: number;
  disk_total_bytes?: number;
}

export interface LidarrLiveStats {
  version?: string;
  artist_count: number;
  monitored_artist_count: number;
  album_count: number;
  track_file_count: number;
  missing_tracks_count: number;
  queue_count: number;
  size_on_disk_bytes: number;
  health_issues: string[];
  disk_free_bytes?: number;
  disk_total_bytes?: number;
}

export interface ArrStats {
  total_grabs: number;
  fetched_count: number;
  imported_count: number;
  replaced_count: number;
  sonarr_series_count: number;
  radarr_movie_count: number;
  lidarr_artist_count: number;
  sonarr_master_online: boolean;
  radarr_master_online: boolean;
  lidarr_master_online: boolean;
  sonarr_live?: SonarrLiveStats;
  radarr_live?: RadarrLiveStats;
  lidarr_live?: LidarrLiveStats;
}

export interface ArrTestConnectionResponse {
  success: boolean;
  message: string;
  app_version?: string;
}

export type FetcherNodeConfig = TransmissionNodeConfig;

export interface TransmissionNodeConfig {
  name: string;
  host: string;
  client_type?: 'transmission' | 'qbittorrent' | 'deluge' | 'synapse';
  port: number;
  rpc_path: string;
  username?: string;
  password?: string;
  use_ssl: boolean;
  enabled: boolean;
  fetcher_only: boolean;
  tv_pre?: string;
  movie_pre?: string;
  music_pre?: string;
  auto_purge_min_space_gb?: number;
  auto_purge_ratio?: number;
  auto_purge_age_days?: number;
  auto_purge_seeds?: number;
  auto_purge_match_count?: number;
  auto_purge_enabled: boolean;
  media_dir_overrides?: Record<string, string>;
}

export interface ArrNodeConfig {
  name: string;
  base_url: string;
  api_key: string;
  base_path?: string;
  version: number;
}

/** An independent stack (e.g. "General Library" / "4K Library") grouping one Sonarr/Radarr/
 * Lidarr instance with a subset of Plex servers and Fetcher nodes. Distinct from a
 * Sonarr/Radarr/Lidarr "replica," which mirrors the same library rather than being independent. */
export interface ZoneConfig {
  id: string;
  name: string;
  sonarr?: ArrNodeConfig;
  radarr?: ArrNodeConfig;
  lidarr?: ArrNodeConfig;
  plex_node_names: string[];
  fetcher_node_names: string[];
  /** @deprecated legacy key name, kept only for reading configs not yet re-saved under `fetcher_node_names` */
  transmission_node_names?: string[];
  webhook_secret?: string;
}

export interface TorrentPeer {
  address: string;
  clientName: string;
  clientIsChoked: boolean;
  clientIsInterested: boolean;
  flagstr: string;
  isDownloadingFrom: boolean;
  isEncrypted: boolean;
  isIncoming: boolean;
  isUploadingTo: boolean;
  isUtp?: boolean;
  peerIsChoked: boolean;
  peerIsInterested: boolean;
  port: number;
  progress: number;
  rateToClient: number;
  rateToPeer: number;
  countryCode?: string;
  /** AS description (ISP/network operator), from iptoasn.com enrichment when enabled. */
  asName?: string;
}

export interface TorrentTimelineEvent {
  stage: 'added' | 'grabbed' | 'downloading' | 'downloaded' | 'staging' | 'seeding' | 'error' | 'healthy' | string;
  title: string;
  description: string;
  timestamp: number;
  formatted_time: string;
  status: 'success' | 'info' | 'warning' | 'error' | 'pending';
}

export interface DetailedTorrent extends UnifiedTorrent {
  comment: string;
  creator: string;
  date_created: number;
  piece_count: number;
  piece_size: number;
  is_private: boolean;
  magnet_link: string;
  corrupt_ever: number;
  seconds_downloading: number;
  seconds_seeding: number;
  activity_date: number;
  queue_position?: number;
  sequential_download?: boolean;
  pieces?: string | null;
  availability?: number[] | null;
  peers: TorrentPeer[];
  timeline: TorrentTimelineEvent[];
  arr_grab?: ArrGrabRecord;
}

export interface QueueMovePayload {
  direction: 'top' | 'up' | 'down' | 'bottom';
}

export interface BulkQueueMovePayload {
  compound_ids: string[];
  direction: 'top' | 'up' | 'down' | 'bottom';
}

export interface SequentialDownloadPayload {
  enabled: boolean;
}

export interface RenamePathPayload {
  path: string;
  new_name: string;
}

export interface BatchReplaceTrackersPayload {
  node?: string;
  compound_ids?: string[];
  old_url: string;
  new_url: string;
}

export interface MediaTypeDefinition {
  id: string;
  name: string;
  default_queue_dir: string;
  uhd_queue_dir?: string;
  description?: string;
}

export interface TrackerMappingRule {
  id: string;
  pattern: string;
  media_type: string;
  priority: number;
  comment?: string;
}

export interface QueueTargetConfig {
  directory: string;
  notify: boolean;
}

export interface QueueRoutingConfig {
  enabled: boolean;
  post_cmd: string;
  default_media_type: string;
  uhd_markers: string[];
  media_types?: MediaTypeDefinition[];
  tracker_mappings?: TrackerMappingRule[];
  queues: Record<string, QueueTargetConfig>;
  filename_rules: Record<string, string[]>;
  tracker_rules: Record<string, string[]>;
}

export interface ClassifyFileResponse {
  media_type: string;
  queue: string;
  target_dir: string;
  post_cmd: string;
  notify: boolean;
  match_source: string;
  decision_trace?: string[];
}

export interface RemoteSyncFolderMapping {
  id: string;
  name: string;
  watch_dir: string;
  post_dir: string;
  media_type: string;
  settle_time_secs: number;
  delete_source_after_move: boolean;
}

export interface AppConfig {
  system: {
    bind_addr: string;
    port: number;
    poll_interval_secs: number;
    jwt_secret: string;
    enable_registration: boolean;
    log_level: string;
    data_dir: string;
    ssl_enabled?: boolean;
    ssl_cert?: string;
    ssl_key?: string;
    dashboard_title?: string;
    request_portal_url?: string;
    request_portal_label?: string;
    media_portal_url?: string;
    media_portal_label?: string;
    /** Keep event logs, Plex watch history, Ombi requests and grab lineage history for this
     * many days; older rows are purged. Unset (the default) keeps everything forever. Never
     * applies to `arr_grabs` (the Ghost Archive) itself. */
    history_retention_days?: number;
  };
  nodes: Record<string, TransmissionNodeConfig>;
  sonarr: {
    enabled: boolean;
    sync_interval_mins: number;
    primary?: ArrNodeConfig;
    master?: ArrNodeConfig;
    replicas: ArrNodeConfig[];
    slaves?: ArrNodeConfig[];
    webhook_secret?: string;
  };
  radarr: {
    enabled: boolean;
    sync_interval_mins: number;
    primary?: ArrNodeConfig;
    master?: ArrNodeConfig;
    replicas: ArrNodeConfig[];
    slaves?: ArrNodeConfig[];
    webhook_secret?: string;
  };
  lidarr: {
    enabled: boolean;
    sync_interval_mins: number;
    primary?: ArrNodeConfig;
    master?: ArrNodeConfig;
    replicas: ArrNodeConfig[];
    slaves?: ArrNodeConfig[];
    webhook_secret?: string;
  };
  plex: {
    enabled: boolean;
    token: string;
    nodes: Array<{
      name: string;
      url: string;
      token_override?: string;
      /** Opts this server into the Trakt-backed multi-server watch-status sync. */
      sync_watch_status?: boolean;
      /** Plex's permanent machine identifier — auto-detected, never user-entered. */
      server_uuid?: string;
    }>;
    refresh_on_sync: boolean;
    digest_scrobbles?: boolean;
    client_identifier?: string;
    notify_on_import?: boolean;
  };
  trakt: {
    enabled: boolean;
    client_id: string;
    client_secret: string;
    access_token?: string;
    refresh_token?: string;
    /** Unix seconds; set by Conduit when it refreshes the token. */
    token_expiry?: number;
    sync_interval_mins: number;
    /** Separate, off-by-default opt-in for Conduit writing a "watched" status back onto a live
     * Plex server (vs. just pushing local scrobbles to Trakt, which only needs a participating
     * node's own `sync_watch_status`). */
    sync_watched_back_to_plex?: boolean;
  };
  notifications: {
    /** Deprecated — superseded by `targets` below. The backend migrates these into an
     * equivalent target on first load; the Settings UI no longer reads or writes them. */
    mattermost?: MattermostChannelConfig;
    discord?: DiscordChannelConfig;
    generic_webhook?: GenericWebhookConfig;
    /** Independently configured notification destinations — the current model. Each target
     * picks its own channel, which event categories it receives, and an optional zone scope. */
    targets: NotificationTarget[];
  };
  space_manager: {
    enabled: boolean;
    check_interval_secs: number;
    global_min_space_gb: number;
    max_purges_per_cycle: number;
    default_target_ratio: number;
    default_target_age_days: number;
    default_target_seeds: number;
    default_required_match_count: number;
  };
  file_sync: {
    enabled: boolean;
    interval_secs: number;
    sync_cmd: string;
    mappings?: RemoteSyncFolderMapping[];
    tv_post?: string;
    movie_post?: string;
    music_post?: string;
    clean_queue_days?: number;
  };
  queue_routing: QueueRoutingConfig;
  rule_pipeline: {
    enabled: boolean;
    auto_delete_unregistered: boolean;
    unregistered_pattern: string;
    auto_trigger_media_replacer: boolean;
  };
  tracker_circuit_breaker?: TrackerCircuitBreakerConfig;
  zones?: ZoneConfig[];
  influx: InfluxConfig;
  ip_asn: IpAsnConfig;
}

export interface InfluxConfig {
  enabled: boolean;
  host: string;
  port: number;
  org: string;
  bucket: string;
  token: string;
  use_ssl: boolean;
}

/** Opt-in peer IP enrichment (country + AS description) via iptoasn.com. Off by default —
 * the parsed database is ~50-60MB resident in memory once loaded. */
export interface IpAsnConfig {
  enabled: boolean;
}

/** The fixed set of event categories a NotificationTarget may subscribe to. An empty
 * `categories` array on a target means "every category". */
export const NOTIFICATION_CATEGORIES = ['grab', 'download', 'replacement', 'health', 'error', 'autopurge', 'sync', 'scrobble'] as const;
export type NotificationCategory = typeof NOTIFICATION_CATEGORIES[number];

export interface MattermostChannelConfig {
  enabled: boolean;
  webhook_url: string;
  bot_name: string;
  channel?: string;
  icon_url?: string;
  server_url?: string;
  bot_token?: string;
  channel_id?: string;
  use_living_cards: boolean;
  use_threading: boolean;
}

export interface DiscordChannelConfig {
  enabled: boolean;
  webhook_url: string;
  username: string;
  avatar_url?: string;
}

export interface PushoverConfig {
  user_key: string;
  api_token: string;
  device?: string;
  priority?: number;
  sound?: string;
}

export interface GenericWebhookConfig {
  enabled: boolean;
  url: string;
  secret?: string;
  /** `summary` (default) sends `{event, title, message, timestamp}`. `full` additionally
   * includes whatever richer field/overview detail the dispatching event has on hand, for an
   * external system that wants to ingest Conduit's event stream rather than a text blurb. */
  payload_mode: 'summary' | 'full';
}

/** Internally tagged on `type` — the tag sits alongside the channel's own fields in the same
 * JSON object (matches the Rust `#[serde(tag = "type")]` enum), not nested under a sub-key. */
export type NotificationChannel =
  | ({ type: 'mattermost' } & MattermostChannelConfig)
  | ({ type: 'discord' } & DiscordChannelConfig)
  | ({ type: 'pushover' } & PushoverConfig)
  | ({ type: 'webhook' } & GenericWebhookConfig);

/** One notification destination: a channel, which event categories it receives (empty = every
 * category), and an optional zone scope (absent = global, fires for every event regardless of
 * zone). */
export interface NotificationTarget {
  id: string;
  name: string;
  enabled: boolean;
  channel: NotificationChannel;
  categories: string[];
  zone_id?: string;
}

export interface TrackerCircuitBreakerConfig {
  enabled: boolean;
  canary_probe_enabled: boolean;
  auto_resume_on_recovery: boolean;
  failure_ratio_threshold: number;
  min_failures: number;
  error_patterns: string[];
  check_interval_secs: number;
  max_tripped_secs: number;
  /** Initial cooldown (seconds) after tripping before the canary's recovery is checked,
   * doubling on relapse (capped at max_tripped_secs). Active mode only. */
  initial_backoff_secs: number;
  /** How long (seconds), after a successful canary recovery, to stagger resumption of
   * paused torrents before fully clearing the breaker. Active mode only. */
  recovery_ramp_secs: number;
  /** Consecutive successful canary checks required during the ramp window before full
   * graduation to healthy. Active mode only. */
  recovery_success_threshold: number;
}

export interface PlexScrobbleRecord {
  id: string;
  event: string;
  user_name: string;
  media_type: string;
  title: string;
  series_title?: string;
  season_number?: number;
  episode_number?: number;
  year?: number;
  imdb_id?: string;
  tmdb_id?: number;
  tvdb_id?: number;
  rating_key?: string;
  duration_ms?: number;
  view_offset_ms?: number;
  trakt_synced: boolean;
  raw_json: string;
  created_at: string;
}

export interface OmbiRequestRecord {
  id: string;
  event_type: string;
  requested_by: string;
  media_type: string;
  title: string;
  year?: number;
  overview?: string;
  poster_url?: string;
  imdb_id?: string;
  tmdb_id?: number;
  tvdb_id?: number;
  status: string;
  raw_json: string;
  created_at: string;
  updated_at: string;
}


// --- Universal Search ---

/** A Sonarr/Radarr catalog lookup match — the same call their own "Add Series"/"Add Movie"
 * search box makes, so `in_library` and `library_id` come straight from that instance. */
export interface ArrLookupResult {
  source: 'sonarr' | 'radarr';
  node_name: string;
  in_library: boolean;
  library_id?: number;
  title: string;
  year?: number;
  overview?: string;
  poster_url?: string;
  tvdb_id?: number;
  tmdb_id?: number;
  imdb_id?: string;
  status?: string;
  /** Sonarr only. */
  network?: string;
  /** Deep link into that Sonarr/Radarr's own web UI — the library item's page when
   * `in_library`, otherwise its "add new" search prefilled with this title. */
  open_url: string;
}

export interface UniversalSearchHistory {
  grabs: ArrGrabRecord[];
  scrobbles: PlexScrobbleRecord[];
  ombi_requests: OmbiRequestRecord[];
}

export interface UniversalSearchResponse {
  query: string;
  torrents: UnifiedTorrent[];
  history: UniversalSearchHistory;
  sonarr: ArrLookupResult[];
  radarr: ArrLookupResult[];
  /** Non-fatal per-source failures (e.g. a Sonarr instance timed out). */
  warnings: string[];
}
