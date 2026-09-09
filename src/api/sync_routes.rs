// src/api/sync_routes.rs
use crate::auth::RequireAuth;
use crate::config::{ConfigManager, MediaTypeDefinition, QueueRoutingConfig, TrackerMappingRule};
use crate::db::{Database, EventLogRecord};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::LazyLock;
use utoipa::{IntoParams, ToSchema};

// Compiled once at first use rather than per-call — these are the static built-in heuristic
// patterns; user-configured pipeline regex rules remain compiled per-rule since their source
// text can change at runtime.
static TIER1_MUSIC_CORRECTION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(FLAC|MP3|320kbps|V0|ALAC|AAC|WAV|Lossless|24bit)\b").unwrap());
static TIER1_TV_CORRECTION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(\bS\d{1,2}E\d{1,3}\b|\b\d{1,2}x\d{1,2}\b|\bSeason\s*\d+\b|\bS\d{1,2}\b)").unwrap());
static BUILTIN_TV_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(\bS\d{1,2}E\d{1,3}\b|\b\d{1,2}x\d{1,2}\b|\bSeason\s*\d+\b|\bS\d{1,2}\b|\b\d{4}\.\d{2}\.\d{2}\b|\.S\d{2}\.|\bE\d{2,3}\b)").unwrap());
static BUILTIN_MUSIC_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(FLAC|MP3|320kbps|V0|ALAC|AAC|WAV|Lossless)\b").unwrap());
static BUILTIN_ANIME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^\[[a-zA-Z0-9_\-]+\]").unwrap());
static BUILTIN_MOVIE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(\b(19\d\d|20\d\d)\b.*(1080p|2160p|720p|bluray|web-dl|webrip|remux|dvdrip|x264|x265|hevc|ddp5\.1|ac3|atmos|dts)|(1080p|2160p|720p).*(bluray|web-dl|webrip|remux))").unwrap());

#[derive(Debug, Deserialize, IntoParams)]
pub struct EventQueryParams {
    /// Maximum number of recent log entries to retrieve (default: 100)
    pub limit: Option<usize>,
    /// Log level filter ("errors_warnings", "error", "warn", "info", "debug", "all")
    pub level: Option<String>,
    /// Module / Subsystem filter (e.g. "arr", "sync", "space", "pipeline", "transmission", "auth")
    pub event_type: Option<String>,
    /// Search query string across messages and details
    pub q: Option<String>,
}

fn deserialize_string_resilient<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = serde_json::Value::deserialize(deserializer)?;
    match val {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        _ => Ok(String::new()),
    }
}

fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        Some(serde_json::Value::String(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed == "null" {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Some(serde_json::Value::Number(n)) => Ok(Some(n.to_string())),
        Some(serde_json::Value::Bool(b)) => Ok(Some(b.to_string())),
        _ => Ok(None),
    }
}

fn deserialize_optional_i32<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        Some(serde_json::Value::Number(n)) => Ok(n.as_i64().and_then(|v| i32::try_from(v).ok())),
        Some(serde_json::Value::String(s)) => Ok(s.trim().parse::<i32>().ok()),
        _ => Ok(None),
    }
}

fn deserialize_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        Some(serde_json::Value::Number(n)) => Ok(n.as_u64()),
        Some(serde_json::Value::String(s)) => Ok(s.trim().parse::<u64>().ok()),
        _ => Ok(None),
    }
}

fn deserialize_optional_trackers<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        Some(serde_json::Value::Array(arr)) => {
            let vec: Vec<String> = arr.into_iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            Ok(if vec.is_empty() { None } else { Some(vec) })
        }
        Some(serde_json::Value::String(s)) => {
            let vec: Vec<String> = s.split(&['\n', '\r', ',', ';'][..]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            Ok(if vec.is_empty() { None } else { Some(vec) })
        }
        _ => Ok(None),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[allow(dead_code)]
pub struct ClassifyFilePayload {
    /// Torrent / filename to classify (TR_TORRENT_NAME)
    #[serde(deserialize_with = "deserialize_string_resilient")]
    pub name: String,
    /// Info hash of the torrent (TR_TORRENT_HASH)
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub hash: Option<String>,
    /// Fetcher Node name
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub node: Option<String>,
    /// Tracker host / announce URL (TR_TORRENT_TRACKERS)
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub tracker: Option<String>,
    /// Array of tracker URLs (optional)
    #[serde(default, deserialize_with = "deserialize_optional_trackers")]
    pub trackers: Option<Vec<String>>,
    /// Source directory path (TR_TORRENT_DIR)
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub dir: Option<String>,
    /// Torrent ID (TR_TORRENT_ID)
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub id: Option<String>,
    /// Comma-delimited list of labels (TR_TORRENT_LABELS)
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub labels: Option<String>,
    /// Priority: -1 (Low), 0 (Normal), 1 (High) (TR_TORRENT_PRIORITY)
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub priority: Option<i32>,
    /// Bytes downloaded (TR_TORRENT_BYTES_DOWNLOADED)
    #[serde(default, deserialize_with = "deserialize_optional_u64")]
    pub bytes_downloaded: Option<u64>,
    /// Transmission short version (TR_APP_VERSION)
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub app_version: Option<String>,
    /// Local time string (TR_TIME_LOCALTIME)
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub time_localtime: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ClassifyFileResponse {
    pub media_type: String,
    pub media: String, // Convenience alias for scripts expecting .media
    pub queue: String,
    pub target_dir: String,
    pub post_cmd: String,
    pub notify: bool,
    pub match_source: String, // "arr_grab", "tracker_mapping", "tracker_rule", "filename_regex", "builtin_heuristic", "default_fallback"
    pub decision_trace: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[allow(dead_code)]
pub struct NotifyDownloadPayload {
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub hash: Option<String>,
    #[serde(deserialize_with = "deserialize_string_resilient")]
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub node: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub path: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub queue: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub target_dir: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub labels: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub priority: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_optional_u64")]
    pub bytes_downloaded: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub app_version: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub time_localtime: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/sync/events",
    tag = "System",
    summary = "List audit & event logs",
    description = "Retrieves recent system events, file sync events, and pipeline auto-purge records with level and module filters.",
    params(EventQueryParams),
    responses(
        (status = 200, description = "List of event logs", body = [EventLogRecord]),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn get_events(
    _auth: RequireAuth,
    State(db): State<Database>,
    Query(params): Query<EventQueryParams>,
) -> Result<Json<Vec<EventLogRecord>>, StatusCode> {
    let limit = params.limit.unwrap_or(200).min(1000);
    let logs = db.search_event_logs(
        params.level.as_deref(),
        params.event_type.as_deref(),
        params.q.as_deref(),
        limit,
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(logs))
}

#[utoipa::path(
    get,
    path = "/api/sync/routes",
    tag = "File Staging",
    summary = "Get Queue Routing Table",
    description = "Returns the active queue routing configuration including Media Types, Tracker Mappings, UHD markers, and regex rules.",
    responses(
        (status = 200, description = "Queue routing configuration", body = QueueRoutingConfig)
    )
)]
pub async fn get_queue_routes(
    _auth: RequireAuth,
    State(config_mgr): State<ConfigManager>,
) -> Json<QueueRoutingConfig> {
    let config = config_mgr.get().await;
    Json(config.queue_routing)
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pat = pattern.trim();
    if pat.is_empty() || text.is_empty() {
        return false;
    }
    let p_clean = pat.trim_matches('*').to_lowercase();
    text.to_lowercase().contains(&p_clean)
}

fn extract_tracker_host(raw: &str) -> String {
    let clean = raw.trim();
    if clean.starts_with("http://") || clean.starts_with("https://") || clean.starts_with("udp://") {
        if let Ok(url) = reqwest::Url::parse(clean) {
            if let Some(host) = url.host_str() {
                return host.to_lowercase();
            }
        }
    }
    // Fallback: strip port and protocol manually
    let without_proto = clean
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("udp://");
    let host_part = without_proto.split(&['/', ':', '?'][..]).next().unwrap_or(without_proto);
    host_part.to_lowercase()
}

#[utoipa::path(
    post,
    path = "/api/sync/classify",
    tag = "File Staging",
    summary = "Classify downloaded file into target queue",
    description = "Evaluates Arr grab records, centralized tracker mappings, and filename regexes using a 4-tier priority hierarchy to determine the proper hardlink queue destination.",
    request_body = ClassifyFilePayload,
    responses(
        (status = 200, description = "Classification result with target directory, command, and decision trace", body = ClassifyFileResponse)
    )
)]
pub async fn classify_file(
    State(config_mgr): State<ConfigManager>,
    State(db): State<Database>,
    Json(payload): Json<ClassifyFilePayload>,
) -> Json<ClassifyFileResponse> {
    let config = config_mgr.get().await;
    let qr = &config.queue_routing;

    let mut media_type = String::new();
    let mut match_source = "default_fallback".to_string();
    let mut decision_trace: Vec<String> = Vec::new();

    // ── Tier 1: Exact Arr Grab Lookup in SQLite Database ──
    let hash_str = payload.hash.as_deref().unwrap_or("");
    let grab_opt = db.find_arr_grab_by_hash_or_name(hash_str, &payload.name).ok().flatten();

    if let Some(ref grab) = grab_opt {
        let is_unknown_series = (grab.item_type == "series" || grab.item_type == "tv") && grab.title.as_deref() == Some("Unknown Series");

        if is_unknown_series && TIER1_MUSIC_CORRECTION_RE.is_match(&payload.name) && !TIER1_TV_CORRECTION_RE.is_match(&payload.name) {
            media_type = "music".to_string();
            match_source = "arr_grab:lidarr_corrected".to_string();
            decision_trace.push(format!("Tier 1 (Arr Grab): Corrected misclassified grab '{}' -> media_type = 'music'", grab.scene_name));
        } else if grab.item_type == "series" || grab.item_type == "tv" {
            media_type = "tv".to_string();
            match_source = "arr_grab:sonarr".to_string();
            decision_trace.push(format!("Tier 1 (Arr Grab): Matched Sonarr series grab '{}' (ID: {}) -> media_type = 'tv'", grab.scene_name, grab.id));
        } else if grab.item_type == "movie" {
            media_type = "movie".to_string();
            match_source = "arr_grab:radarr".to_string();
            decision_trace.push(format!("Tier 1 (Arr Grab): Matched Radarr movie grab '{}' (ID: {}) -> media_type = 'movie'", grab.scene_name, grab.id));
        } else if grab.item_type == "music" || grab.item_type == "album" || grab.item_type == "artist" {
            media_type = "music".to_string();
            match_source = "arr_grab:lidarr".to_string();
            decision_trace.push(format!("Tier 1 (Arr Grab): Matched Lidarr music grab '{}' (ID: {}) -> media_type = 'music'", grab.scene_name, grab.id));
        } else if !grab.item_type.is_empty() {
            media_type = grab.item_type.clone();
            match_source = format!("arr_grab:{}", grab.item_type);
            decision_trace.push(format!("Tier 1 (Arr Grab): Matched grab '{}' -> media_type = '{}'", grab.scene_name, grab.item_type));
        }
    }

    // ── Tier 2: Centralized Tracker-to-Media-Type Mappings ──
    if media_type.is_empty() {
        let mut tracker_candidates: Vec<String> = Vec::new();

        let mut add_candidate = |raw: &str| {
            for line in raw.split(&['\n', '\r', ',', ';', ' '][..]) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let candidate_str = trimmed.to_string();
                    if !tracker_candidates.contains(&candidate_str) {
                        tracker_candidates.push(candidate_str);
                    }
                    let clean_host = extract_tracker_host(trimmed);
                    if !clean_host.is_empty() && !tracker_candidates.contains(&clean_host) {
                        tracker_candidates.push(clean_host);
                    }
                }
            }
        };

        if let Some(ref t) = payload.tracker {
            add_candidate(t);
        }
        if let Some(ref tlist) = payload.trackers {
            for t in tlist {
                add_candidate(t);
            }
        }
        if let Some(ref grab) = grab_opt {
            if let Some(ref idx) = grab.indexer {
                add_candidate(idx);
            }
        }

        // 2a. Check structured tracker_mappings (sorted by priority descending)
        let mut sorted_rules = qr.tracker_mappings.clone();
        sorted_rules.sort_by(|a, b| b.priority.cmp(&a.priority));

        'mapping_loop: for rule in &sorted_rules {
            for candidate in &tracker_candidates {
                if wildcard_match(&rule.pattern, candidate) {
                    media_type = rule.media_type.clone();
                    match_source = format!("tracker_rule:{}", rule.pattern);
                    decision_trace.push(format!("Tier 2 (Tracker Mapping): Matched tracker candidate '{}' against pattern '{}' (Priority: {}) -> media_type = '{}'", candidate, rule.pattern, rule.priority, rule.media_type));
                    break 'mapping_loop;
                }
            }
        }

        // 2b. Check key-value tracker_rules map (e.g. {"tv": ["landof.tv", "bitmetv.org"], "movie": ["passthepopcorn.me"]})
        if media_type.is_empty() {
            'rule_loop: for (mtype, patterns) in &qr.tracker_rules {
                for pattern in patterns {
                    for candidate in &tracker_candidates {
                        if wildcard_match(pattern, candidate) || candidate.to_lowercase().contains(&pattern.to_lowercase()) {
                            media_type = mtype.clone();
                            match_source = format!("tracker_list:{}", pattern);
                            decision_trace.push(format!("Tier 2 (Tracker Rule List): Matched tracker candidate '{}' against '{}' -> media_type = '{}'", candidate, pattern, mtype));
                            break 'rule_loop;
                        }
                    }
                }
            }
        }
    }

    // ── Tier 3: Heuristic Filename Regex & Release Parser Rules ──
    if media_type.is_empty() {
        // 3a. User configured filename rules
        'regex_loop: for (mtype, patterns) in &qr.filename_rules {
            for pattern in patterns {
                let re_pattern = if pattern.starts_with("(?i)") {
                    pattern.clone()
                } else {
                    format!("(?i){}", pattern)
                };
                if let Ok(re) = Regex::new(&re_pattern) {
                    if re.is_match(&payload.name) {
                        media_type = mtype.to_string();
                        match_source = format!("filename_regex:{}", pattern);
                        decision_trace.push(format!("Tier 3 (Filename Regex): Matched pattern '{}' -> media_type = '{}'", pattern, mtype));
                        break 'regex_loop;
                    }
                }
            }
        }

        // 3b. Built-in High-Accuracy Media Heuristics (if user regex didn't trigger)
        if media_type.is_empty() {
            if BUILTIN_TV_RE.is_match(&payload.name) {
                media_type = "tv".to_string();
                match_source = "builtin_heuristic:tv".to_string();
                decision_trace.push("Tier 3 (Built-in Heuristic): Recognized TV episode/season pattern -> media_type = 'tv'".to_string());
            } else if BUILTIN_ANIME_RE.is_match(&payload.name) {
                media_type = "anime".to_string();
                match_source = "builtin_heuristic:anime".to_string();
                decision_trace.push("Tier 3 (Built-in Heuristic): Recognized Anime release group format -> media_type = 'anime'".to_string());
            } else if BUILTIN_MUSIC_RE.is_match(&payload.name) {
                media_type = "music".to_string();
                match_source = "builtin_heuristic:music".to_string();
                decision_trace.push("Tier 3 (Built-in Heuristic): Recognized Audio format marker -> media_type = 'music'".to_string());
            } else if BUILTIN_MOVIE_RE.is_match(&payload.name) {
                media_type = "movie".to_string();
                match_source = "builtin_heuristic:movie".to_string();
                decision_trace.push("Tier 3 (Built-in Heuristic): Recognized Feature film year/quality signature -> media_type = 'movie'".to_string());
            }
        }
    }

    // ── Tier 4: Fallback to Default Media Type ──
    if media_type.is_empty() {
        media_type = if !qr.default_media_type.is_empty() {
            qr.default_media_type.clone()
        } else {
            "misc".to_string()
        };
        decision_trace.push(format!("Tier 4 (Default Fallback): No prior tier matched -> default media_type = '{}'", media_type));
    }

    // ── UHD / 4K Marker Promotion ──
    let is_uhd = qr.uhd_markers.iter().any(|m| payload.name.to_lowercase().contains(&m.to_lowercase()));
    let resolved_queue = if is_uhd {
        let uhd_candidate = format!("{}UHD", media_type);
        decision_trace.push(format!("UHD Promotion: Name contains UHD marker -> queue promoted to '{}'", uhd_candidate));
        uhd_candidate
    } else {
        media_type.clone()
    };

    // ── Resolution: Determine Target Directory & Overrides ──
    let mut resolved_target_dir = String::new();

    // Check if node has specific folder override
    if let Some(ref node_name) = payload.node {
        if let Some(node_cfg) = config.nodes.get(node_name).or_else(|| config.nodes.values().find(|n| n.name.eq_ignore_ascii_case(node_name))) {
            let maybe_override: Option<&String> = node_cfg.media_dir_overrides.get(&resolved_queue).or_else(|| node_cfg.media_dir_overrides.get(&media_type));
            if let Some(override_dir) = maybe_override {
                if !override_dir.is_empty() {
                    resolved_target_dir = override_dir.clone();
                    decision_trace.push(format!("Node Folder Override: Node '{}' mapped '{}' to '{}'", node_name, resolved_queue, override_dir));
                }
            }
        }
    }

    // If no node override, check media_types definition
    if resolved_target_dir.is_empty() {
        if let Some(media_def) = qr.media_types.iter().find(|m| m.id.eq_ignore_ascii_case(&media_type)) {
            if is_uhd {
                if let Some(ref uhd_dir) = media_def.uhd_queue_dir {
                    if !uhd_dir.is_empty() {
                        resolved_target_dir = uhd_dir.clone();
                        decision_trace.push(format!("Media Type Directory: Used UHD queue directory '{}'", uhd_dir));
                    }
                }
            }
            if resolved_target_dir.is_empty() {
                resolved_target_dir = media_def.default_queue_dir.clone();
                decision_trace.push(format!("Media Type Directory: Used default queue directory '{}'", media_def.default_queue_dir));
            }
        }
    }

    // If still empty, check legacy queues map or fallback
    if resolved_target_dir.is_empty() {
        if let Some(target) = qr.queues.get(&resolved_queue) {
            resolved_target_dir = target.directory.clone();
            decision_trace.push(format!("Legacy Queue Map: Target directory '{}'", target.directory));
        } else {
            resolved_target_dir = format!("/media/queue/{}Queue/", resolved_queue);
            decision_trace.push(format!("Default Queue Path: Fallback directory '{}'", resolved_target_dir));
        }
    }

    if let Some(mut grab) = grab_opt.clone() {
        if let Some(ref node) = payload.node {
            if !node.is_empty() && (grab.download_client.is_none() || grab.download_client.as_deref() == Some("Transmission")) {
                grab.download_client = Some(node.clone());
                let _ = db.save_arr_grab(&grab);
            }
        }
    }

    let notify = qr.queues.get(&resolved_queue).map(|t| t.notify).unwrap_or(true);

    Json(ClassifyFileResponse {
        media_type: media_type.clone(),
        media: media_type,
        queue: resolved_queue,
        target_dir: resolved_target_dir,
        post_cmd: qr.post_cmd.clone(),
        notify,
        match_source,
        decision_trace,
    })
}

#[utoipa::path(
    post,
    path = "/api/sync/notify-download",
    tag = "File Staging",
    summary = "Notify Conduit of downloaded torrent & stage execution",
    description = "Invoked by post-download hook script to log file placement, advance Arr status, and dispatch notifications.",
    request_body = NotifyDownloadPayload,
    responses(
        (status = 200, description = "Download notification acknowledged")
    )
)]
pub async fn notify_download(
    State(config_mgr): State<ConfigManager>,
    State(db): State<Database>,
    Json(payload): Json<NotifyDownloadPayload>,
) -> Json<serde_json::Value> {
    let name_str = payload.name.trim().chars().take(1024).collect::<String>();
    if name_str.is_empty() {
        return Json(json!({"status": "error", "message": "name is required"}));
    }
    let _hash_str = payload.hash.clone().unwrap_or_else(|| "none".to_string());
    let node_str = payload.node.clone().unwrap_or_else(|| "Transmission".to_string());
    let queue_str = payload.queue.clone().unwrap_or_else(|| "Queue".to_string());
    let target_str = payload.target_dir.clone().unwrap_or_else(|| "/media/queue".to_string());

    let log_msg = format!(
        "📥 Torrent downloaded & staged: '{}' (Node: {}, Queue: {}) -> {}",
        name_str, node_str, queue_str, target_str
    );

    let _ = db.log_event("file_sync", "info", &log_msg, None);

    // Update Arr grab record status in SQLite if matching, or perform on-the-fly enrichment
    let config = config_mgr.get().await;
    let mut matched_grab = db.find_arr_grab_by_hash_or_name(payload.hash.as_deref().unwrap_or(""), &name_str).ok().flatten();

    if matched_grab.is_none() {
        matched_grab = crate::api::torrent_routes::perform_enrichment_for_torrent(
            &config,
            &db,
            &name_str,
            payload.hash.as_deref().unwrap_or(""),
            &node_str,
            payload.bytes_downloaded.unwrap_or(0) as i64,
        ).await;
    }

    let maybe_post_id = matched_grab.as_ref().and_then(|g| g.mattermost_post_id.clone());

    if let Some(mut grab) = matched_grab.clone() {
        grab.status = "staged".to_string();
        if let Some(ref node) = payload.node {
            if !node.is_empty() {
                grab.download_client = Some(node.clone());
            }
        }
        let _ = db.save_arr_grab(&grab);
    }

    let title_name = matched_grab.as_ref().and_then(|g| g.title.clone()).unwrap_or_else(|| name_str.clone());
    let item_overview = matched_grab.as_ref().and_then(|g| g.overview.clone()).unwrap_or_else(|| name_str.clone());
    let poster = matched_grab.as_ref().and_then(|g| g.poster_url.clone());
    let notif_title = format!("🐕📦 Conduit Staged Content • {}", title_name);
    let hash_clone = payload.hash.clone().unwrap_or_default();
    let name_clone = name_str.clone();
    let db_clone = db.clone();

    tokio::spawn(async move {
        let fields = vec![
            ("Status", "🟢 Staged & Hardlinked", true),
            ("Node", node_str.as_str(), true),
            ("Queue", queue_str.as_str(), true),
            ("Destination", target_str.as_str(), false),
        ];

        let post_id_opt = crate::notify::NotificationManager::dispatch_rich_or_update(
            &config.notifications,
            "sync.download",
            None,
            maybe_post_id.as_deref(),
            None,
            &notif_title,
            &item_overview,
            poster.as_deref(),
            fields,
            Some("#10B981"),
        ).await;

        if let Some(post_id) = post_id_opt {
            if let Ok(Some(mut g)) = db_clone.find_arr_grab_by_hash_or_name(&hash_clone, &name_clone) {
                g.mattermost_post_id = Some(post_id.clone());
                let _ = db_clone.save_arr_grab(&g);
            }

            // Thread a staging detail comment under the main card
            let thread_msg = format!("📥 Hardlink staging completed on node `{}` -> target: `{}`", node_str, target_str);
            let _ = crate::notify::NotificationManager::dispatch_rich_or_update(
                &config.notifications,
                "sync.download.detail",
                None,
                None,
                Some(post_id.as_str()),
                "Staging Detail",
                &thread_msg,
                None,
                Vec::new(),
                Some("#10B981"),
            ).await;
        }
    });

    Json(json!({
        "status": "ok",
        "message": "Download logged and staged successfully"
    }))
}

#[utoipa::path(
    get,
    path = "/api/sync/trackers",
    tag = "File Staging",
    summary = "Get Tracker Mapping Rules",
    description = "Returns the list of configured tracker hostname patterns and their assigned default Media Types.",
    responses(
        (status = 200, description = "List of tracker mapping rules", body = [TrackerMappingRule])
    )
)]
pub async fn get_tracker_mappings(
    _auth: RequireAuth,
    State(config_mgr): State<ConfigManager>,
) -> Json<Vec<TrackerMappingRule>> {
    let config = config_mgr.get().await;
    Json(config.queue_routing.tracker_mappings)
}

#[utoipa::path(
    post,
    path = "/api/sync/trackers",
    tag = "File Staging",
    summary = "Save Tracker Mapping Rules",
    description = "Updates the full set of tracker hostname patterns and their assigned Media Types.",
    request_body = [TrackerMappingRule],
    responses(
        (status = 200, description = "Tracker mapping rules updated successfully")
    )
)]
pub async fn save_tracker_mappings(
    _auth: RequireAuth,
    State(config_mgr): State<ConfigManager>,
    Json(rules): Json<Vec<TrackerMappingRule>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut config = config_mgr.get().await;
    config.queue_routing.tracker_mappings = rules;
    config_mgr.update(config).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({"status": "ok", "message": "Tracker mapping rules updated successfully"})))
}

#[utoipa::path(
    get,
    path = "/api/sync/media-types",
    tag = "File Staging",
    summary = "Get Media Types Definitions",
    description = "Returns the registered Media Types, display names, and default staging directories.",
    responses(
        (status = 200, description = "List of media type definitions", body = [MediaTypeDefinition])
    )
)]
pub async fn get_media_types(
    _auth: RequireAuth,
    State(config_mgr): State<ConfigManager>,
) -> Json<Vec<MediaTypeDefinition>> {
    let config = config_mgr.get().await;
    Json(config.queue_routing.media_types.clone())
}

#[utoipa::path(
    post,
    path = "/api/sync/media-types",
    tag = "File Staging",
    summary = "Save Media Types Definitions",
    description = "Updates the registered Media Types, display names, and staging directories.",
    request_body = [MediaTypeDefinition],
    responses(
        (status = 200, description = "Media types definitions updated successfully")
    )
)]
pub async fn save_media_types(
    _auth: RequireAuth,
    State(config_mgr): State<ConfigManager>,
    Json(types): Json<Vec<MediaTypeDefinition>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut config = config_mgr.get().await;
    config.queue_routing.media_types = types;
    config_mgr.update(config).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({"status": "ok", "message": "Media types updated successfully"})))
}

#[utoipa::path(
    put,
    path = "/api/nodes/{name}/media-overrides",
    tag = "Nodes",
    summary = "Update node media directory overrides",
    description = "Saves node-specific folder path overrides for media types.",
    params(
        ("name" = String, Path, description = "Fetcher node name")
    ),
    request_body = HashMap<String, String>,
    responses(
        (status = 200, description = "Node folder overrides updated"),
        (status = 404, description = "Node not found")
    )
)]
pub async fn update_node_media_overrides(
    _auth: RequireAuth,
    State(config_mgr): State<ConfigManager>,
    Path(node_name): Path<String>,
    Json(overrides): Json<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut config = config_mgr.get().await;
    let found = if let Some(node) = config.nodes.get_mut(&node_name) {
        node.media_dir_overrides = overrides;
        true
    } else if let Some(node) = config.nodes.values_mut().find(|n| n.name.eq_ignore_ascii_case(&node_name)) {
        node.media_dir_overrides = overrides;
        true
    } else {
        false
    };

    if found {
        config_mgr.update(config).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(json!({"status": "ok", "message": "Node folder overrides updated"})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct HookScriptQueryParams {
    /// Script format: "sh" | "bash" | "py" | "pl" (default: "sh")
    pub format: Option<String>,
    /// Custom Conduit URL (defaults to server url)
    pub url: Option<String>,
    /// Default node name (optional)
    pub node: Option<String>,
    /// Default API token (optional)
    pub api_key: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/sync/hook-script",
    tag = "File Staging",
    summary = "Generate Fetcher Done-Hook Script (copy2queue)",
    description = "Generates a pre-configured standalone intake hook script (Bash, Python, or Perl) ready for curl download into fetcher daemons.",
    params(HookScriptQueryParams),
    responses(
        (status = 200, description = "Generated executable hook script", body = String)
    )
)]
pub async fn get_hook_script(
    Query(params): Query<HookScriptQueryParams>,
    State(config_mgr): State<ConfigManager>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let config = config_mgr.get().await;

    let base_url = params.url.unwrap_or_else(|| {
        format!("http://{}:{}", config.system.bind_addr, config.system.port)
    });
    let default_node = params.node.unwrap_or_else(|| "Transmission".to_string());
    let api_key = params.api_key.unwrap_or_default();
    let format = params.format.unwrap_or_else(|| "sh".to_string()).to_lowercase();

    let (content_type, script) = match format.as_str() {
        "py" | "python" => {
            let py_code = format!(r#"#!/usr/bin/env python3
"""
Conduit Transmission Intake Hook Script (conduit-fetch-hook.py)
Automated multi-tier media classification, intake staging, and notification dispatch.
Supports zero-copy hardlinks (cp -al) and integrates with Sonarr, Radarr, and Lidarr pipelines.

Environment Variables consumed from Transmission daemon:
  TR_APP_VERSION, TR_TIME_LOCALTIME, TR_TORRENT_BYTES_DOWNLOADED,
  TR_TORRENT_DIR, TR_TORRENT_HASH, TR_TORRENT_ID, TR_TORRENT_LABELS,
  TR_TORRENT_NAME, TR_TORRENT_PRIORITY, TR_TORRENT_TRACKERS
"""
import os
import sys
import json
import argparse
import urllib.request
import urllib.error
import subprocess

# Defaults (Environment variables take precedence)
CONDUIT_URL = os.environ.get("CONDUIT_URL", "{base_url}").rstrip("/")
CONDUIT_API_KEY = os.environ.get("CONDUIT_API_KEY", "{api_key}")
CONDUIT_NODE_NAME = os.environ.get("CONDUIT_NODE_NAME", "{default_node}")
CONDUIT_DEBUG = os.environ.get("CONDUIT_DEBUG", os.environ.get("DEBUG", "0")).lower() in ("1", "true", "yes")

def main():
    parser = argparse.ArgumentParser(description="Conduit Transmission Intake Hook")
    parser.add_argument("--name", default=os.environ.get("TR_TORRENT_NAME", ""), help="Torrent Name")
    parser.add_argument("--hash", default=os.environ.get("TR_TORRENT_HASH", ""), help="Torrent Infohash")
    parser.add_argument("--dir", default=os.environ.get("TR_TORRENT_DIR", ""), help="Download directory")
    parser.add_argument("--tracker", default=os.environ.get("TR_TORRENT_TRACKERS", ""), help="Tracker hostname/URL")
    parser.add_argument("--id", default=os.environ.get("TR_TORRENT_ID", ""), help="Torrent ID")
    parser.add_argument("--labels", default=os.environ.get("TR_TORRENT_LABELS", ""), help="Torrent Labels")
    parser.add_argument("--priority", default=os.environ.get("TR_TORRENT_PRIORITY", ""), help="Torrent Priority")
    parser.add_argument("--bytes", default=os.environ.get("TR_TORRENT_BYTES_DOWNLOADED", ""), help="Bytes Downloaded")
    parser.add_argument("--app-version", default=os.environ.get("TR_APP_VERSION", ""), help="Transmission Version")
    parser.add_argument("--localtime", default=os.environ.get("TR_TIME_LOCALTIME", ""), help="Local Time")
    parser.add_argument("--node", default=CONDUIT_NODE_NAME, help="Transmission node identifier")
    parser.add_argument("--debug", "-d", action="store_true", default=CONDUIT_DEBUG, help="Enable verbose debug logging")
    args = parser.parse_args()

    torrent_name = args.name.strip()
    torrent_hash = args.hash.strip()
    download_dir = args.dir.strip()
    tracker_url = args.tracker.strip()
    node_name = args.node.strip()
    is_debug = args.debug

    if not torrent_name:
        print("[ERROR] Torrent name is required (pass via TR_TORRENT_NAME or --name)", file=sys.stderr)
        sys.exit(1)

    print(f"[INFO] Processing torrent: '{{torrent_name}}' (node: {{node_name}}, hash: {{torrent_hash}})")

    # 1. Query Conduit API for 4-Tier Media Classification
    classify_dict = {{
        "name": torrent_name,
        "hash": torrent_hash if torrent_hash else None,
        "node": node_name,
        "tracker": tracker_url if tracker_url else None,
        "dir": download_dir if download_dir else None,
        "id": args.id if args.id else None,
        "labels": args.labels if args.labels else None,
        "priority": int(args.priority) if args.priority else None,
        "bytes_downloaded": int(args.bytes) if args.bytes else None,
        "app_version": args.app_version if args.app_version else None,
        "time_localtime": args.localtime if args.localtime else None,
    }}
    classify_payload = json.dumps(classify_dict).encode("utf-8")

    if is_debug:
        print(f"[DEBUG] Target URL: {{CONDUIT_URL}}/api/sync/classify", file=sys.stderr)
        print(f"[DEBUG] Outbound Payload: {{json.dumps(classify_dict, indent=2)}}", file=sys.stderr)

    req = urllib.request.Request(
        f"{{CONDUIT_URL}}/api/sync/classify",
        data=classify_payload,
        headers={{
            "Content-Type": "application/json",
            "User-Agent": "Conduit-Intake-Hook/1.0",
            **({{"Authorization": f"Bearer {{CONDUIT_API_KEY}}"}} if CONDUIT_API_KEY else {{}})
        }}
    )

    data = {{}}
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            resp_body = resp.read().decode("utf-8")
            if is_debug:
                print(f"[DEBUG] Classification Response: {{resp_body}}", file=sys.stderr)
            data = json.loads(resp_body)
    except urllib.error.HTTPError as e:
        err_body = e.read().decode("utf-8", errors="replace")
        print(f"[WARN] Could not reach Conduit at '{{CONDUIT_URL}}' (HTTP {{e.code}}). Using built-in local classification heuristics.", file=sys.stderr)
        if err_body:
            print(f"[DEBUG] Conduit Server Response: {{err_body}}", file=sys.stderr)
        if is_debug:
            print(f"[DEBUG] Outbound Payload Sent: {{json.dumps(classify_dict, indent=2)}}", file=sys.stderr)
    except Exception as e:
        print(f"[WARN] Failed to contact Conduit API at {{CONDUIT_URL}}: {{e}}. Using built-in local classification heuristics.", file=sys.stderr)

    target_dir = data.get("target_dir")
    post_cmd = data.get("post_cmd")
    queue = data.get("queue")
    media_type = data.get("media_type")
    decision_trace = data.get("decision_trace", [])

    # Local fallback if Conduit offline or unclassified
    if not target_dir or not media_type:
        name_lower = torrent_name.lower()
        if any(x in name_lower for x in [".s0", ".s1", ".s2", "season", "s01", "s02", "s03", "s04", "s05"]):
            media_type = "tv"
            queue = "tv"
            target_dir = "/media/queue/tvQueue/"
        elif any(x in name_lower for x in ["1080p", "2160p", "720p", "bluray", "web-dl", "webrip", "remux"]):
            media_type = "movie"
            queue = "movie"
            target_dir = "/media/queue/movieQueue/"
        elif any(x in name_lower for x in ["flac", "mp3", "320kbps", "v0", "lossless"]):
            media_type = "music"
            queue = "music"
            target_dir = "/media/queue/musicQueue/"
        else:
            media_type = "misc"
            queue = "misc"
            target_dir = "/media/queue/miscQueue/"

    post_cmd = post_cmd or "cp -av"

    print(f"[CLASSIFIED] Media: {{media_type}} -> Queue: {{queue}} -> Target: {{target_dir}}")
    for step in decision_trace:
        print(f"  [TRACE] {{step}}")

    # 2. Execute Staging Copy / Hardlink
    os.makedirs(target_dir, exist_ok=True)
    source_path = os.path.join(download_dir, torrent_name) if download_dir else torrent_name

    import shlex
    cmd_parts = shlex.split(post_cmd) if post_cmd else ["cp", "-av"]
    exec_target = target_dir.rstrip("/") + "/"
    exec_args = cmd_parts + [source_path, exec_target]
    print(f"[EXEC] {{' '.join(shlex.quote(a) for a in exec_args)}}")
    ret = subprocess.call(exec_args, shell=False)

    if ret != 0:
        print(f"[WARN] Staging command exited with code {{ret}}", file=sys.stderr)

    # 3. Notify Conduit of completed staging
    notify_dict = {{
        "name": torrent_name,
        "hash": torrent_hash if torrent_hash else None,
        "node": node_name,
        "path": source_path,
        "queue": queue,
        "target_dir": target_dir,
        "id": args.id if args.id else None,
        "labels": args.labels if args.labels else None,
        "priority": int(args.priority) if args.priority else None,
        "bytes_downloaded": int(args.bytes) if args.bytes else None,
        "app_version": args.app_version if args.app_version else None,
        "time_localtime": args.localtime if args.localtime else None,
    }}
    notify_payload = json.dumps(notify_dict).encode("utf-8")

    if is_debug:
        print(f"[DEBUG] Staging Notification Payload: {{json.dumps(notify_dict, indent=2)}}", file=sys.stderr)

    n_req = urllib.request.Request(
        f"{{CONDUIT_URL}}/api/sync/notify-download",
        data=notify_payload,
        headers={{
            "Content-Type": "application/json",
            "User-Agent": "Conduit-Intake-Hook/1.0",
            **({{"Authorization": f"Bearer {{CONDUIT_API_KEY}}"}} if CONDUIT_API_KEY else {{}})
        }}
    )
    try:
        urllib.request.urlopen(n_req, timeout=5)
        print("[INFO] Staging and Conduit notification completed successfully.")
    except urllib.error.HTTPError as e:
        err_body = e.read().decode("utf-8", errors="replace")
        print(f"[INFO] Staging completed (Conduit notification status: HTTP {{e.code}}).")
        if err_body and is_debug:
            print(f"[DEBUG] Notification Response Body: {{err_body}}", file=sys.stderr)
    except Exception as e:
        print(f"[WARN] Failed to dispatch Conduit notification: {{e}}", file=sys.stderr)

if __name__ == "__main__":
    main()
"#, base_url = base_url, api_key = api_key, default_node = default_node);
            ("text/x-python", py_code)
        }
        "pl" | "perl" => {
            let pl_code = format!(r#"#!/usr/bin/env perl
# Conduit Transmission Intake Hook Script (conduit-fetch-hook.pl)
# Automated multi-tier media classification, intake staging, and notification dispatch.
use strict;
use warnings;
use Getopt::Long;
use HTTP::Tiny;
use JSON::PP;
use File::Path qw(make_path);

my $CONDUIT_URL = $ENV{{CONDUIT_URL}} || "{base_url}";
my $CONDUIT_API_KEY = $ENV{{CONDUIT_API_KEY}} || "{api_key}";
my $CONDUIT_NODE_NAME = $ENV{{CONDUIT_NODE_NAME}} || "{default_node}";
my $CONDUIT_DEBUG = $ENV{{CONDUIT_DEBUG}} || $ENV{{DEBUG}} || 0;

my $torrent_name     = $ENV{{TR_TORRENT_NAME}} || "";
my $torrent_hash     = $ENV{{TR_TORRENT_HASH}} || "";
my $download_dir     = $ENV{{TR_TORRENT_DIR}} || "";
my $tracker_url      = $ENV{{TR_TORRENT_TRACKERS}} || "";
my $torrent_id       = $ENV{{TR_TORRENT_ID}} || "";
my $torrent_labels   = $ENV{{TR_TORRENT_LABELS}} || "";
my $torrent_priority = $ENV{{TR_TORRENT_PRIORITY}} || "";
my $bytes_downloaded = $ENV{{TR_TORRENT_BYTES_DOWNLOADED}} || "";
my $app_version      = $ENV{{TR_APP_VERSION}} || "";
my $time_localtime   = $ENV{{TR_TIME_LOCALTIME}} || "";
my $node_name        = $CONDUIT_NODE_NAME;
my $is_debug         = $CONDUIT_DEBUG ? 1 : 0;

GetOptions(
    "name=s"        => \$torrent_name,
    "hash=s"        => \$torrent_hash,
    "dir=s"         => \$download_dir,
    "tracker=s"     => \$tracker_url,
    "id=s"          => \$torrent_id,
    "labels=s"      => \$torrent_labels,
    "priority=i"    => \$torrent_priority,
    "bytes=i"       => \$bytes_downloaded,
    "app-version=s" => \$app_version,
    "localtime=s"   => \$time_localtime,
    "node=s"        => \$node_name,
    "debug|d"       => \$is_debug,
);

die "[ERROR] Torrent name is required\n" unless $torrent_name;

print "[INFO] Processing torrent: '$torrent_name' (node: $node_name, hash: $torrent_hash)\n";

my $json = JSON::PP->new->utf8;
my $http = HTTP::Tiny->new(timeout => 10);

# 1. Query Conduit API
my $payload = $json->encode({{
    name             => $torrent_name,
    hash             => $torrent_hash || undef,
    node             => $node_name,
    tracker          => $tracker_url || undef,
    dir              => $download_dir || undef,
    id               => $torrent_id || undef,
    labels           => $torrent_labels || undef,
    priority         => $torrent_priority ne "" ? 0 + $torrent_priority : undef,
    bytes_downloaded => $bytes_downloaded ne "" ? 0 + $bytes_downloaded : undef,
    app_version      => $app_version || undef,
    time_localtime   => $time_localtime || undef,
}});

if ($is_debug) {{
    print STDERR "[DEBUG] Conduit URL: $CONDUIT_URL/api/sync/classify\n";
    print STDERR "[DEBUG] Payload: $payload\n";
}}

my $headers = {{ 'Content-Type' => 'application/json' }};
$headers->{{'Authorization'}} = "Bearer $CONDUIT_API_KEY" if $CONDUIT_API_KEY;

my $resp = $http->post("$CONDUIT_URL/api/sync/classify", {{
    headers => $headers,
    content => $payload,
}});

my $target_dir = "";
my $post_cmd   = "cp -av";
my $queue      = "misc";
my $media_type = "misc";

if ($resp->{{success}}) {{
    my $data = $json->decode($resp->{{content}});
    $target_dir = $data->{{target_dir}} || "/media/queue/miscQueue/";
    $post_cmd   = $data->{{post_cmd}} || "cp -av";
    $queue      = $data->{{queue}} || "misc";
    $media_type = $data->{{media_type}} || "misc";
}} else {{
    print STDERR "[WARN] Could not reach Conduit at '$CONDUIT_URL' (HTTP $resp->{{status}}). Using built-in local classification heuristics.\n";
    if ($resp->{{content}}) {{
        print STDERR "[DEBUG] Conduit Server Response: $resp->{{content}}\n";
    }}
    if ($is_debug) {{
        print STDERR "[DEBUG] Outbound Payload Sent: $payload\n";
    }}
    if ($torrent_name =~ /(s\d{{1,2}}e\d{{1,3}}|season\s*\d+|\.s\d{{2}}\.)/i) {{
        $media_type = "tv"; $queue = "tv"; $target_dir = "/media/queue/tvQueue/";
    }} elsif ($torrent_name =~ /(1080p|2160p|720p|bluray|web-dl|webrip|remux)/i) {{
        $media_type = "movie"; $queue = "movie"; $target_dir = "/media/queue/movieQueue/";
    }} elsif ($torrent_name =~ /(flac|mp3|320kbps|v0|lossless)/i) {{
        $media_type = "music"; $queue = "music"; $target_dir = "/media/queue/musicQueue/";
    }} else {{
        $media_type = "misc"; $queue = "misc"; $target_dir = "/media/queue/miscQueue/";
    }}
}}

print "[CLASSIFIED] Media: $media_type -> Queue: $queue -> Target: $target_dir\n";

# 2. Execute Staging Copy
make_path($target_dir) unless -d $target_dir;
my $source_path = $download_dir ? "$download_dir/$torrent_name" : $torrent_name;

my @cmd_parts = split(/\s+/, $post_cmd || "cp -av");
my $target_dest = $target_dir;
$target_dest =~ s{{/+$}}{{}};
$target_dest .= "/";
my @full_cmd = (@cmd_parts, $source_path, $target_dest);
print "[EXEC] " . join(" ", map {{ qq("$_") }} @full_cmd) . "\n";
system(@full_cmd);

# 3. Notify Conduit API
my $notify_payload = $json->encode({{
    name             => $torrent_name,
    hash             => $torrent_hash || undef,
    node             => $node_name,
    path             => $source_path,
    queue            => $queue,
    target_dir       => $target_dir,
    id               => $torrent_id || undef,
    labels           => $torrent_labels || undef,
    priority         => $torrent_priority ne "" ? 0 + $torrent_priority : undef,
    bytes_downloaded => $bytes_downloaded ne "" ? 0 + $bytes_downloaded : undef,
    app_version      => $app_version || undef,
    time_localtime   => $time_localtime || undef,
}});

if ($is_debug) {{
    print STDERR "[DEBUG] Staging Notification Payload: $notify_payload\n";
}}

my $n_resp = $http->post("$CONDUIT_URL/api/sync/notify-download", {{
    headers => $headers,
    content => $notify_payload,
}});

if ($n_resp->{{success}}) {{
    print "[INFO] Staging and Conduit notification completed successfully.\n";
}} else {{
    print "[INFO] Staging completed (Conduit notification status: HTTP $n_resp->{{status}}).\n";
    if ($n_resp->{{content}} && $is_debug) {{
        print STDERR "[DEBUG] Notification Response: $n_resp->{{content}}\n";
    }}
}}
"#, base_url = base_url, api_key = api_key, default_node = default_node);
            ("text/x-perl", pl_code)
        }
        _ => {
            // Default: Universal Bash Script
            let sh_code = format!(r#"#!/bin/sh
# Conduit Transmission Intake Hook Script (conduit-fetch-hook.sh)
# Automated multi-tier media classification, intake staging, and notification dispatch.
# Usage: Load into Transmission as script-torrent-done-filename

set -e

# Configuration (Environment variables take precedence over hardcoded defaults)
CONDUIT_URL="${{CONDUIT_URL:-{base_url}}}"
CONDUIT_API_KEY="${{CONDUIT_API_KEY:-{api_key}}}"
CONDUIT_NODE_NAME="${{CONDUIT_NODE_NAME:-{default_node}}}"
CONDUIT_DEBUG="${{CONDUIT_DEBUG:-${{DEBUG:-0}}}}"

# Parse CLI arguments and debug flag
for arg in "$@"; do
    case "$arg" in
        --debug|-d) CONDUIT_DEBUG=1 ;;
    esac
done

# Transmission Inbound Environment Variables
NAME="${{TR_TORRENT_NAME:-$1}}"
HASH="${{TR_TORRENT_HASH:-$2}}"
DIR="${{TR_TORRENT_DIR:-$3}}"
TRACKER="${{TR_TORRENT_TRACKERS:-$4}}"
ID="${{TR_TORRENT_ID:-}}"
LABELS="${{TR_TORRENT_LABELS:-}}"
PRIORITY="${{TR_TORRENT_PRIORITY:-}}"
BYTES="${{TR_TORRENT_BYTES_DOWNLOADED:-}}"
APP_VERSION="${{TR_APP_VERSION:-}}"
TIME_LOCAL="${{TR_TIME_LOCALTIME:-}}"
NODE="${{CONDUIT_NODE_NAME}}"

if [ -z "$NAME" ]; then
    echo "[ERROR] No torrent name provided. Set TR_TORRENT_NAME or pass as \$1." >&2
    exit 1
fi

echo "[INFO] Processing torrent: '$NAME' (Node: $NODE, Hash: $HASH)"

# POSIX escape helper for JSON strings (handles double quotes, backslashes, and multiline trackers)
escape_json() {{
    printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g' | tr '\r\n\t' '   '
}}

NAME_ESC=$(escape_json "$NAME")
HASH_ESC=$(escape_json "$HASH")
NODE_ESC=$(escape_json "$NODE")
TRACKER_ESC=$(escape_json "$TRACKER")
DIR_ESC=$(escape_json "$DIR")
ID_ESC=$(escape_json "$ID")
LABELS_ESC=$(escape_json "$LABELS")
APP_VERSION_ESC=$(escape_json "$APP_VERSION")
TIME_LOCAL_ESC=$(escape_json "$TIME_LOCAL")

case "$PRIORITY" in
    ''|*[!0-9-]*) PRIORITY_VAL="null" ;;
    *) PRIORITY_VAL="$PRIORITY" ;;
esac

case "$BYTES" in
    ''|*[!0-9]*) BYTES_VAL="null" ;;
    *) BYTES_VAL="$BYTES" ;;
esac

# Prepare JSON Payload
JSON_PAYLOAD=$(cat <<EOF
{{
  "name": "$NAME_ESC",
  "hash": "$HASH_ESC",
  "node": "$NODE_ESC",
  "tracker": "$TRACKER_ESC",
  "dir": "$DIR_ESC",
  "id": "$ID_ESC",
  "labels": "$LABELS_ESC",
  "priority": $PRIORITY_VAL,
  "bytes_downloaded": $BYTES_VAL,
  "app_version": "$APP_VERSION_ESC",
  "time_localtime": "$TIME_LOCAL_ESC"
}}
EOF
)

if [ "$CONDUIT_DEBUG" = "1" ] || [ "$CONDUIT_DEBUG" = "true" ]; then
    echo "[DEBUG] Sending classification request to $CONDUIT_URL/api/sync/classify" >&2
    echo "[DEBUG] Classification Payload: $JSON_PAYLOAD" >&2
fi

# 1. Query Conduit Classification Engine
AUTH_HEADER=""
API_KEY_HEADER=""
if [ -n "$CONDUIT_API_KEY" ]; then
    AUTH_HEADER="Authorization: Bearer $CONDUIT_API_KEY"
    API_KEY_HEADER="X-Api-Key: $CONDUIT_API_KEY"
fi

HTTP_CODE=0
RESPONSE=$(curl -sSL --max-time 10 -w "\n%{{http_code}}" -X POST "$CONDUIT_URL/api/sync/classify" \
    -H "Content-Type: application/json" \
    ${{AUTH_HEADER:+-H "$AUTH_HEADER"}} \
    ${{API_KEY_HEADER:+-H "$API_KEY_HEADER"}} \
    -d "$JSON_PAYLOAD" 2>/dev/null || echo "000")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
RESPONSE_BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$CONDUIT_DEBUG" = "1" ] || [ "$CONDUIT_DEBUG" = "true" ]; then
    echo "[DEBUG] Classification HTTP Status: $HTTP_CODE" >&2
    echo "[DEBUG] Classification HTTP Response: $RESPONSE_BODY" >&2
fi

TARGET_DIR=""
POST_CMD=""
QUEUE=""
MEDIA_TYPE=""

# Extract Target Dir, Post Cmd, Queue, and Media Type (using jq, python3, or grep fallback)
if [ "$HTTP_CODE" = "200" ] && [ -n "$RESPONSE_BODY" ]; then
    if command -v jq >/dev/null 2>&1; then
        TARGET_DIR=$(echo "$RESPONSE_BODY" | jq -r '.target_dir // empty')
        POST_CMD=$(echo "$RESPONSE_BODY" | jq -r '.post_cmd // empty')
        QUEUE=$(echo "$RESPONSE_BODY" | jq -r '.queue // empty')
        MEDIA_TYPE=$(echo "$RESPONSE_BODY" | jq -r '.media_type // .media // empty')
    elif command -v python3 >/dev/null 2>&1; then
        TARGET_DIR=$(echo "$RESPONSE_BODY" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('target_dir') or '')" 2>/dev/null || true)
        POST_CMD=$(echo "$RESPONSE_BODY" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('post_cmd') or '')" 2>/dev/null || true)
        QUEUE=$(echo "$RESPONSE_BODY" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('queue') or '')" 2>/dev/null || true)
        MEDIA_TYPE=$(echo "$RESPONSE_BODY" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('media_type') or d.get('media') or '')" 2>/dev/null || true)
    else
        TARGET_DIR=$(echo "$RESPONSE_BODY" | grep -o '"target_dir":"[^"]*' | cut -d'"' -f4)
        POST_CMD=$(echo "$RESPONSE_BODY" | grep -o '"post_cmd":"[^"]*' | cut -d'"' -f4)
        QUEUE=$(echo "$RESPONSE_BODY" | grep -o '"queue":"[^"]*' | cut -d'"' -f4)
        MEDIA_TYPE=$(echo "$RESPONSE_BODY" | grep -o '"media_type":"[^"]*' | cut -d'"' -f4)
        if [ -z "$MEDIA_TYPE" ]; then
            MEDIA_TYPE=$(echo "$RESPONSE_BODY" | grep -o '"media":"[^"]*' | cut -d'"' -f4)
        fi
    fi
fi

# Offline / Unreachable Heuristic Fallback (Ensures proper staging even if Conduit daemon is offline)
if [ "$HTTP_CODE" != "200" ] || [ -z "$TARGET_DIR" ] || [ -z "$MEDIA_TYPE" ]; then
    echo "[WARN] Could not reach Conduit at '$CONDUIT_URL' (HTTP $HTTP_CODE). Using built-in local classification heuristics." >&2
    if [ -n "$RESPONSE_BODY" ]; then
        echo "[DEBUG] Conduit Server Response: $RESPONSE_BODY" >&2
    fi
    if [ "$CONDUIT_DEBUG" = "1" ] || [ "$CONDUIT_DEBUG" = "true" ]; then
        echo "[DEBUG] Outbound Payload Sent: $JSON_PAYLOAD" >&2
    fi
    if echo "$NAME" | grep -Ei '(\bS[0-9]{{1,2}}E[0-9]{{1,3}}\b|\b[0-9]{{1,2}}x[0-9]{{1,2}}\b|\bSeason\s*[0-9]+\b|\bS[0-9]{{1,2}}\b|\.S[0-9]{{2}}\.|\bE[0-9]{{2,3}}\b)' >/dev/null 2>&1; then
        MEDIA_TYPE="tv"
        QUEUE="tv"
        TARGET_DIR="/media/queue/tvQueue/"
    elif echo "$NAME" | grep -Ei '(\b(19[0-9]{{2}}|20[0-9]{{2}})\b.*(1080p|2160p|720p|bluray|web-dl|webrip|remux|dvdrip|x264|x265|hevc|ddp5\.1|ac3|atmos|dts)|(1080p|2160p|720p).*(bluray|web-dl|webrip|remux))' >/dev/null 2>&1; then
        MEDIA_TYPE="movie"
        QUEUE="movie"
        TARGET_DIR="/media/queue/movieQueue/"
    elif echo "$NAME" | grep -Ei '\b(FLAC|MP3|320kbps|V0|ALAC|AAC|WAV|Lossless)\b' >/dev/null 2>&1; then
        MEDIA_TYPE="music"
        QUEUE="music"
        TARGET_DIR="/media/queue/musicQueue/"
    else
        MEDIA_TYPE="misc"
        QUEUE="misc"
        TARGET_DIR="/media/queue/miscQueue/"
    fi
fi

POST_CMD="${{POST_CMD:-cp -av}}"

echo "[CLASSIFIED] Media: $MEDIA_TYPE -> Queue: $QUEUE -> Target: $TARGET_DIR"

# 2. Ensure Target Directory Exists & Execute Staging Command
mkdir -p "$TARGET_DIR"
SOURCE_PATH="$NAME"
if [ -n "$DIR" ]; then
    SOURCE_PATH="$DIR/$NAME"
fi

echo "[EXEC] $POST_CMD \"$SOURCE_PATH\" \"$TARGET_DIR/\""
$POST_CMD "$SOURCE_PATH" "$TARGET_DIR/" || echo "[WARN] Copy command failed" >&2

# 3. Notify Conduit of Successful Staging
SOURCE_PATH_ESC=$(escape_json "$SOURCE_PATH")
TARGET_DIR_ESC=$(escape_json "$TARGET_DIR")
QUEUE_ESC=$(escape_json "$QUEUE")

NOTIFY_PAYLOAD=$(cat <<EOF
{{
  "name": "$NAME_ESC",
  "hash": "$HASH_ESC",
  "node": "$NODE_ESC",
  "path": "$SOURCE_PATH_ESC",
  "queue": "$QUEUE_ESC",
  "target_dir": "$TARGET_DIR_ESC",
  "id": "$ID_ESC",
  "labels": "$LABELS_ESC",
  "priority": $PRIORITY_VAL,
  "bytes_downloaded": $BYTES_VAL,
  "app_version": "$APP_VERSION_ESC",
  "time_localtime": "$TIME_LOCAL_ESC"
}}
EOF
)

if [ "$CONDUIT_DEBUG" = "1" ] || [ "$CONDUIT_DEBUG" = "true" ]; then
    echo "[DEBUG] Sending staging notification to $CONDUIT_URL/api/sync/notify-download" >&2
    echo "[DEBUG] Notification Payload: $NOTIFY_PAYLOAD" >&2
fi

NOTIFY_RESP=$(curl -sSL --max-time 5 -w "\n%{{http_code}}" -X POST "$CONDUIT_URL/api/sync/notify-download" \
    -H "Content-Type: application/json" \
    ${{AUTH_HEADER:+-H "$AUTH_HEADER"}} \
    ${{API_KEY_HEADER:+-H "$API_KEY_HEADER"}} \
    -d "$NOTIFY_PAYLOAD" 2>/dev/null || echo "000")

NOTIFY_CODE=$(echo "$NOTIFY_RESP" | tail -n1)
NOTIFY_BODY=$(echo "$NOTIFY_RESP" | sed '$d')

if [ "$CONDUIT_DEBUG" = "1" ] || [ "$CONDUIT_DEBUG" = "true" ]; then
    echo "[DEBUG] Notification HTTP Code: $NOTIFY_CODE" >&2
    echo "[DEBUG] Notification HTTP Body: $NOTIFY_BODY" >&2
fi

if [ "$NOTIFY_CODE" = "200" ]; then
    echo "[INFO] Staging and Conduit notification completed successfully."
else
    echo "[INFO] Staging completed (Conduit notification status: HTTP $NOTIFY_CODE)."
    if [ -n "$NOTIFY_BODY" ] && [ "$NOTIFY_CODE" != "000" ]; then
        echo "[DEBUG] Conduit Notification Response: $NOTIFY_BODY" >&2
    fi
fi
"#, base_url = base_url, api_key = api_key, default_node = default_node);
            ("text/x-shellscript", sh_code)
        }
    };

    ([(axum::http::header::CONTENT_TYPE, content_type)], script).into_response()
}

#[utoipa::path(
    get,
    path = "/api/health",
    tag = "System",
    summary = "Health check",
    description = "Service health check probe returning 'ruff ruff' greeting and operational status.",
    responses(
        (status = 200, description = "Health status payload")
    )
)]
pub async fn health_check(State(config_mgr): State<ConfigManager>) -> Json<serde_json::Value> {
    let dashboard_title = config_mgr.get().await.system.dashboard_title;
    Json(json!({
        "status": "ok",
        "service": "Conduit",
        "dashboard_title": dashboard_title,
        "version": env!("CARGO_PKG_VERSION"),
        "greeting": "ruff ruff"
    }))
}
