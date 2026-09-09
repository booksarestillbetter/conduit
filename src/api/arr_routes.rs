// src/api/arr_routes.rs
use crate::auth::{AppState, RequireAdmin, RequireAuth};
use crate::db::ArrGrabRecord;
use crate::notify::NotificationManager;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::LazyLock;
use tracing::debug;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

// Compiled once at first use rather than per-call — these patterns are static, so there's no
// reason to pay regex-compilation cost on every webhook/enrichment request.
static YEAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\(\[](\d{4})[\)\]]").unwrap());
static MUSIC_QUALITY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\[(V0|FLAC|MP3|320|24bit|Lossless|AAC|ALAC|WAV)[^\]]*\]").unwrap());
static STRIP_TAGS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{[^\}]+\}|\[[^\]]+\]|\(\d{4}\)").unwrap());
static MUSIC_MARKER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(FLAC|MP3|320kbps|V0|Lossless|24bit|ALAC|AAC|WAV)\b").unwrap());
static TV_MARKER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(\bS\d{1,2}E\d{1,3}\b|\b\d{1,2}x\d{1,2}\b|\bSeason\s*\d+\b|\bS\d{1,2}\b|\b\d{4}\.\d{2}\.\d{2}\b|\.S\d{2}\.|\bE\d{2,3}\b)").unwrap());

#[derive(Debug, Deserialize, ToSchema)]
pub struct ArrTestConnectionRequest {
    pub app_type: String, // "sonarr" | "radarr"
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ArrTestConnectionResponse {
    pub success: bool,
    pub message: String,
    pub app_version: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ArrSyncNowRequest {
    pub app_type: String, // "sonarr" | "radarr" | "all"
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SonarrLiveStats {
    pub version: Option<String>,
    pub series_count: usize,
    pub monitored_series_count: usize,
    pub episode_count: usize,
    pub episode_file_count: usize,
    pub missing_episodes_count: usize,
    pub queue_count: usize,
    pub size_on_disk_bytes: i64,
    pub health_issues: Vec<String>,
    pub disk_free_bytes: Option<i64>,
    pub disk_total_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RadarrLiveStats {
    pub version: Option<String>,
    pub movie_count: usize,
    pub monitored_movie_count: usize,
    pub movie_file_count: usize,
    pub missing_movies_count: usize,
    pub queue_count: usize,
    pub size_on_disk_bytes: i64,
    pub health_issues: Vec<String>,
    pub disk_free_bytes: Option<i64>,
    pub disk_total_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LidarrLiveStats {
    pub version: Option<String>,
    pub artist_count: usize,
    pub monitored_artist_count: usize,
    pub album_count: usize,
    pub track_file_count: usize,
    pub missing_tracks_count: usize,
    pub queue_count: usize,
    pub size_on_disk_bytes: i64,
    pub health_issues: Vec<String>,
    pub disk_free_bytes: Option<i64>,
    pub disk_total_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ArrStatsResponse {
    pub total_grabs: usize,
    pub fetched_count: usize,
    pub imported_count: usize,
    pub replaced_count: usize,
    pub sonarr_series_count: usize,
    pub radarr_movie_count: usize,
    pub lidarr_artist_count: usize,
    pub sonarr_master_online: bool,
    pub radarr_master_online: bool,
    pub lidarr_master_online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sonarr_live: Option<SonarrLiveStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radarr_live: Option<RadarrLiveStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lidarr_live: Option<LidarrLiveStats>,
    /// Populated when the live-stats fetch for an enabled+configured app failed — this is what
    /// engines::health_monitor reads to detect down/recovered state, instead of independently
    /// re-polling the same system/status endpoint on its own timer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sonarr_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radarr_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lidarr_error: Option<String>,
}

fn parse_music_release_title(title: &str) -> (String, String, Option<i64>, Option<String>) {
    let clean_title = title.trim().to_string();
    let mut year = None;
    let mut quality = None;

    if let Some(caps) = YEAR_RE.captures(&clean_title) {
        if let Some(m) = caps.get(1) {
            year = m.as_str().parse::<i64>().ok();
        }
    }

    if let Some(caps) = MUSIC_QUALITY_RE.captures(&clean_title) {
        if let Some(m) = caps.get(1) {
            quality = Some(m.as_str().to_string());
        }
    }

    let stripped = STRIP_TAGS_RE.replace_all(&clean_title, "").trim().to_string();

    if stripped.contains(" - ") {
        let parts: Vec<&str> = stripped.splitn(2, " - ").collect();
        let artist = parts[0].trim().to_string();
        let album = parts[1].trim().to_string();
        (artist, album, year, quality)
    } else {
        (stripped.clone(), stripped, year, quality)
    }
}

fn detect_inbound_media_type(
    payload: &serde_json::Value,
    qr: &crate::config::QueueRoutingConfig,
) -> String {
    if payload.get("artist").is_some() || payload.get("albums").is_some() || payload.get("trackFile").is_some() || payload.get("trackFiles").is_some() {
        return "music".to_string();
    }
    if payload.get("movie").is_some() || payload.get("movieFile").is_some() {
        return "movie".to_string();
    }
    if payload.get("series").is_some() && payload.get("series").and_then(|s| s.get("title")).and_then(|t| t.as_str()).map(|t| t != "Unknown Series").unwrap_or(false) {
        return "tv".to_string();
    }

    let release_title = payload.get("release")
        .and_then(|r| r.get("releaseTitle"))
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("episodeFile").and_then(|f| f.get("sceneName")).and_then(|v| v.as_str()))
        .or_else(|| payload.get("movieFile").and_then(|f| f.get("sceneName")).and_then(|v| v.as_str()))
        .or_else(|| payload.get("trackFile").and_then(|f| f.get("sceneName")).and_then(|v| v.as_str()))
        .unwrap_or("");

    let indexer = payload.get("release")
        .and_then(|r| r.get("indexer"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Check tracker mappings
    let mut sorted_rules = qr.tracker_mappings.clone();
    sorted_rules.sort_by(|a, b| b.priority.cmp(&a.priority));

    for rule in &sorted_rules {
        let pat_clean = rule.pattern.trim_matches('*').to_lowercase();
        if !indexer.is_empty() && (indexer.to_lowercase().contains(&pat_clean) || pat_clean.contains(&indexer.to_lowercase())) {
            return rule.media_type.clone();
        }
    }

    for (mtype, patterns) in &qr.tracker_rules {
        for pattern in patterns {
            let pat_clean = pattern.trim_matches('*').to_lowercase();
            if !indexer.is_empty() && (indexer.to_lowercase().contains(&pat_clean) || pat_clean.contains(&indexer.to_lowercase())) {
                return mtype.clone();
            }
        }
    }

    if MUSIC_MARKER_RE.is_match(release_title) && !TV_MARKER_RE.is_match(release_title) {
        return "music".to_string();
    }
    if TV_MARKER_RE.is_match(release_title) {
        return "tv".to_string();
    }

    "unknown".to_string()
}

fn infer_quality_from_release(release: &str, current: Option<&str>) -> String {
    if let Some(c) = current {
        let c_trim = c.trim();
        if !c_trim.is_empty() && !c_trim.eq_ignore_ascii_case("Standard") && !c_trim.eq_ignore_ascii_case("Unknown") {
            return c_trim.to_string();
        }
    }

    let rel_lower = release.to_lowercase();
    if rel_lower.contains("2160p") || rel_lower.contains("4k") || rel_lower.contains("uhd") {
        if rel_lower.contains("remux") { return "2160p Remux".to_string(); }
        if rel_lower.contains("bluray") { return "2160p BluRay".to_string(); }
        if rel_lower.contains("web-dl") || rel_lower.contains("webrip") { return "2160p WEB-DL".to_string(); }
        return "2160p UHD".to_string();
    }
    if rel_lower.contains("1080p") {
        if rel_lower.contains("remux") { return "1080p Remux".to_string(); }
        if rel_lower.contains("bluray") { return "1080p BluRay".to_string(); }
        if rel_lower.contains("web-dl") || rel_lower.contains("webrip") { return "1080p WEB-DL".to_string(); }
        return "1080p HD".to_string();
    }
    if rel_lower.contains("720p") {
        return "720p HD".to_string();
    }
    if rel_lower.contains("flac") {
        return "FLAC Lossless".to_string();
    }
    if rel_lower.contains("v0") {
        return "MP3 V0 (VBR)".to_string();
    }
    if rel_lower.contains("320") || rel_lower.contains("320kbps") {
        return "MP3 320k (CBR)".to_string();
    }
    if rel_lower.contains("lossless") {
        return "Lossless Audio".to_string();
    }

    "Standard".to_string()
}

fn infer_indexer_source(payload_idx: Option<&str>, grab_idx: Option<&str>, release: &str, qr: &crate::config::QueueRoutingConfig) -> String {
    if let Some(p) = payload_idx {
        let p_trim = p.trim();
        if !p_trim.is_empty() && !p_trim.eq_ignore_ascii_case("Internal") && !p_trim.eq_ignore_ascii_case("Unknown") {
            return p_trim.to_string();
        }
    }
    if let Some(g) = grab_idx {
        let g_trim = g.trim();
        if !g_trim.is_empty() && !g_trim.eq_ignore_ascii_case("Internal") && !g_trim.eq_ignore_ascii_case("Unknown") {
            return g_trim.to_string();
        }
    }

    let rel_lower = release.to_lowercase();
    for rule in &qr.tracker_mappings {
        let pat = rule.pattern.trim_matches('*').to_lowercase();
        if !pat.is_empty() && rel_lower.contains(&pat) {
            return rule.media_type.clone();
        }
    }

    "Indexer Feed".to_string()
}

fn extract_poster_url(obj: Option<&serde_json::Value>) -> Option<String> {
    let images = obj.and_then(|m| m.get("images")).and_then(|i| i.as_array())?;
    // First prioritize poster or cover
    if let Some(img) = images.iter().find(|img| {
        let ct = img.get("coverType").and_then(|c| c.as_str()).unwrap_or("");
        ct.eq_ignore_ascii_case("poster") || ct.eq_ignore_ascii_case("cover")
    }) {
        if let Some(url) = img.get("remoteUrl").and_then(|u| u.as_str()).or_else(|| img.get("url").and_then(|u| u.as_str())) {
            return Some(url.to_string());
        }
    }
    // Fallback to fanart or banner
    images.iter().find_map(|img| {
        let ct = img.get("coverType").and_then(|c| c.as_str()).unwrap_or("");
        if ct.eq_ignore_ascii_case("fanart") || ct.eq_ignore_ascii_case("banner") {
            img.get("remoteUrl").and_then(|u| u.as_str())
                .or_else(|| img.get("url").and_then(|u| u.as_str()))
                .map(|s| s.to_string())
        } else {
            None
        }
    })
}

fn format_media_info_summary(file_obj: Option<&serde_json::Value>) -> Option<String> {
    let mi = file_obj.and_then(|f| f.get("mediaInfo"))?;
    let mut parts = Vec::new();

    if let (Some(w), Some(h)) = (mi.get("width").and_then(|v| v.as_i64()), mi.get("height").and_then(|v| v.as_i64())) {
        if w >= 3800 || h >= 2100 {
            parts.push("4K UHD".to_string());
        } else if w >= 1900 || h >= 1000 {
            parts.push("1080p".to_string());
        } else if w >= 1200 || h >= 700 {
            parts.push("720p".to_string());
        }
    }

    if let Some(vc) = mi.get("videoCodec").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        parts.push(vc.to_string());
    }

    if let Some(hdr) = mi.get("videoDynamicRangeType").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        parts.push(hdr.to_string());
    } else if let Some(hdr) = mi.get("videoDynamicRange").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        parts.push(hdr.to_string());
    }

    if let Some(ac) = mi.get("audioCodec").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        let ch = mi.get("audioChannels").and_then(|v| v.as_f64()).map(|c| format!("{:.1}", c)).unwrap_or_default();
        if !ch.is_empty() {
            parts.push(format!("{} {}", ac, ch));
        } else {
            parts.push(ac.to_string());
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" • "))
    }
}

fn format_indexer_flags(payload: &serde_json::Value) -> Option<String> {
    let rel = payload.get("release");
    let mut flags = Vec::new();
    if let Some(arr) = rel.and_then(|r| r.get("indexerFlags")).and_then(|f| f.as_array()) {
        for flag in arr {
            if let Some(s) = flag.as_str() {
                flags.push(s.to_string());
            }
        }
    } else if let Some(flags_str) = payload.get("movieFile").and_then(|f| f.get("indexerFlags")).and_then(|f| f.as_str()) {
        if !flags_str.is_empty() {
            flags.push(flags_str.to_string());
        }
    }
    if flags.is_empty() { None } else { Some(flags.join(", ")) }
}

fn format_custom_format_summary(payload: &serde_json::Value) -> Option<String> {
    let cfi = payload.get("customFormatInfo")?;
    let score = cfi.get("customFormatScore").and_then(|s| s.as_i64());
    let mut names = Vec::new();
    if let Some(cfs) = cfi.get("customFormats").and_then(|cf| cf.as_array()) {
        for cf in cfs {
            if let Some(name) = cf.get("name").and_then(|n| n.as_str()) {
                names.push(name.to_string());
            }
        }
    }
    if names.is_empty() && score.is_none() {
        return None;
    }
    let names_str = if names.is_empty() { "Matched".to_string() } else { names.join(", ") };
    if let Some(sc) = score {
        Some(format!("{} ({:+})", names_str, sc))
    } else {
        Some(names_str)
    }
}

async fn handle_arr_health_event(
    state: &AppState,
    app_name: &str,
    payload: &serde_json::Value,
    zone_id: Option<&str>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let event_type = payload.get("eventType").and_then(|v| v.as_str()).unwrap_or("Health");
    let is_restored = event_type.eq_ignore_ascii_case("HealthRestored");
    let instance_name = payload.get("instanceName").and_then(|v| v.as_str()).unwrap_or(app_name);
    let level = payload.get("level").and_then(|v| v.as_str()).unwrap_or("warning");
    let check_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("SystemHealth");
    let message = payload.get("message").and_then(|v| v.as_str()).unwrap_or("Health check triggered");
    let wiki_url = payload.get("wikiUrl").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty());

    let db_level = match level.to_lowercase().as_str() {
        "error" => "error",
        "notice" => "info",
        _ => "warn",
    };

    let _ = state.db.log_event(
        app_name,
        db_level,
        &format!("{}: {}", check_type, message),
        Some(&payload.to_string()),
    );

    let color_hex = if is_restored {
        "#10B981" // Emerald
    } else {
        match level.to_lowercase().as_str() {
            "error" => "#EF4444", // Red
            "notice" => "#3B82F6", // Blue
            _ => "#F59E0B",       // Amber
        }
    };

    let title = if is_restored {
        format!("✅ {} Health Restored • {}", instance_name, check_type)
    } else {
        match level.to_lowercase().as_str() {
            "error" => format!("🚨 {} Health Alert (Error) • {}", instance_name, check_type),
            "notice" => format!("ℹ️ {} Health Notice • {}", instance_name, check_type),
            _ => format!("⚠️ {} Health Warning • {}", instance_name, check_type),
        }
    };

    let overview = if let Some(wiki) = wiki_url {
        format!("{}\n\n📖 [Troubleshooting Wiki]({})", message, wiki)
    } else {
        message.to_string()
    };

    let level_upper = level.to_uppercase();
    let fields: Vec<(&str, &str, bool)> = vec![
        ("Check", check_type, true),
        ("Severity", &level_upper, true),
        ("Instance", instance_name, true),
    ];

    let _ = NotificationManager::dispatch_rich(
        &state.config.get().await.notifications,
        &format!("{}.health", app_name),
        zone_id,
        &title,
        &overview,
        None,
        fields,
        Some(color_hex),
    ).await;

    Ok(Json(json!({"status": "ok", "event": event_type, "type": check_type})))
}

async fn handle_arr_app_update(
    state: &AppState,
    app_name: &str,
    payload: &serde_json::Value,
    zone_id: Option<&str>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_name = payload.get("instanceName").and_then(|v| v.as_str()).unwrap_or(app_name);
    let prev_ver = payload.get("previousVersion").and_then(|v| v.as_str()).unwrap_or("unknown");
    let new_ver = payload.get("newVersion").and_then(|v| v.as_str()).unwrap_or("unknown");
    let message = payload.get("message").and_then(|v| v.as_str());

    let log_msg = format!("{} updated: v{} -> v{}", instance_name, prev_ver, new_ver);
    let _ = state.db.log_event(app_name, "info", &log_msg, Some(&payload.to_string()));

    let title = format!("🚀 {} Updated to v{}", instance_name, new_ver);
    let overview = message.map(|m| m.to_string()).unwrap_or_else(|| format!("Successfully upgraded from v{} to v{}.", prev_ver, new_ver));

    let fields: Vec<(&str, &str, bool)> = vec![
        ("Previous Version", prev_ver, true),
        ("New Version", new_ver, true),
    ];

    let _ = NotificationManager::dispatch_rich(
        &state.config.get().await.notifications,
        &format!("{}.update", app_name),
        zone_id,
        &title,
        &overview,
        None,
        fields,
        Some("#0EA5E9"),
    ).await;

    Ok(Json(json!({"status": "ok", "event": "ApplicationUpdate", "version": new_ver})))
}

async fn handle_arr_manual_interaction(
    state: &AppState,
    app_name: &str,
    payload: &serde_json::Value,
    zone_id: Option<&str>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_name = payload.get("instanceName").and_then(|v| v.as_str()).unwrap_or(app_name);
    let download_status = payload.get("downloadStatus").and_then(|v| v.as_str()).unwrap_or("Warning");
    let download_client = payload.get("downloadClient").and_then(|v| v.as_str()).unwrap_or("Download Client");

    let mut reasons: Vec<String> = Vec::new();
    if let Some(arr) = payload.get("downloadStatusMessages").and_then(|v| v.as_array()) {
        for item in arr {
            let item_title = item.get("title").and_then(|t| t.as_str());
            if let Some(msgs) = item.get("messages").and_then(|m| m.as_array()) {
                let msgs_str: Vec<&str> = msgs.iter().filter_map(|m| m.as_str()).collect();
                if !msgs_str.is_empty() {
                    if let Some(t) = item_title {
                        reasons.push(format!("{}: {}", t, msgs_str.join(", ")));
                    } else {
                        reasons.push(msgs_str.join(", "));
                    }
                } else if let Some(t) = item_title {
                    reasons.push(t.to_string());
                }
            } else if let Some(t) = item_title {
                reasons.push(t.to_string());
            }
        }
    }
    let reason_str = if reasons.is_empty() {
        "Manual approval or interaction required in queue".to_string()
    } else {
        reasons.join("; ")
    };

    let title_name = payload.get("movie").and_then(|m| m.get("title")).and_then(|t| t.as_str())
        .or_else(|| payload.get("series").and_then(|s| s.get("title")).and_then(|t| t.as_str()))
        .or_else(|| payload.get("downloadInfo").and_then(|d| d.get("title")).and_then(|t| t.as_str()))
        .unwrap_or("Release");

    let year = payload.get("movie").and_then(|m| m.get("year")).and_then(|y| y.as_i64())
        .or_else(|| payload.get("series").and_then(|s| s.get("year")).and_then(|y| y.as_i64()));

    let poster_url = extract_poster_url(payload.get("movie").or_else(|| payload.get("series")));

    let log_msg = format!("Manual interaction required for '{}' in {}: {}", title_name, instance_name, reason_str);
    let _ = state.db.log_event(app_name, "warn", &log_msg, Some(&payload.to_string()));

    let notif_title = if let Some(y) = year {
        format!("🐕 Conduit Needs Help • {} ({})", title_name, y)
    } else {
        format!("🐕 Conduit Needs Help • {}", title_name)
    };

    let overview = format!("**Manual action required in {}:**\n{}\n\n**Queue Status:** `{}`", instance_name, reason_str, download_status);

    let mut fields: Vec<(&str, &str, bool)> = Vec::new();
    fields.push(("Client", download_client, true));
    fields.push(("Status", download_status, true));
    if let Some(dl_info) = payload.get("downloadInfo") {
        if let Some(idx) = dl_info.get("indexer").and_then(|i| i.as_str()) {
            fields.push(("Indexer", idx, true));
        }
    }
    let sz_str = payload.get("downloadInfo").and_then(|d| d.get("size")).and_then(|s| s.as_i64()).map(format_bytes_str);
    if let Some(ref sz) = sz_str {
        fields.push(("Size", sz.as_str(), true));
    }

    let _ = NotificationManager::dispatch_rich(
        &state.config.get().await.notifications,
        &format!("{}.manual", app_name),
        zone_id,
        &notif_title,
        &overview,
        poster_url.as_deref(),
        fields,
        Some("#F43F5E"),
    ).await;

    Ok(Json(json!({"status": "ok", "event": "ManualInteractionRequired"})))
}

async fn handle_arr_media_added(
    state: &AppState,
    app_name: &str,
    payload: &serde_json::Value,
    zone_id: Option<&str>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_name = payload.get("instanceName").and_then(|v| v.as_str()).unwrap_or(app_name);
    let is_movie = payload.get("movie").is_some();
    let media_obj = payload.get("movie").or_else(|| payload.get("series"));

    let title = media_obj.and_then(|m| m.get("title")).and_then(|t| t.as_str()).unwrap_or("Unknown Title");
    let year = media_obj.and_then(|m| m.get("year")).and_then(|y| y.as_i64());
    let overview = media_obj.and_then(|m| m.get("overview")).and_then(|o| o.as_str());
    let poster_url = extract_poster_url(media_obj);

    let add_method = payload.get("addMethod").and_then(|a| a.as_str());

    let log_msg = format!("{} added to {}: '{}'", if is_movie { "Movie" } else { "Series" }, instance_name, title);
    let _ = state.db.log_event(app_name, "info", &log_msg, Some(&payload.to_string()));

    let notif_title = if let Some(y) = year {
        format!("🐕 Conduit Tracking • {} ({})", title, y)
    } else {
        format!("🐕 Conduit Tracking • {}", title)
    };

    let overview_text = overview.unwrap_or("Added to library. Conduit will monitor indexers for available releases.");

    let mut fields: Vec<(&str, &str, bool)> = Vec::new();
    let type_str = if is_movie { "Movie" } else { "TV Series" };
    fields.push(("Type", type_str, true));
    if let Some(method) = add_method {
        fields.push(("Add Method", method, true));
    }
    let genres_vec: Vec<String> = media_obj.and_then(|m| m.get("genres")).and_then(|g| g.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let genres_str = genres_vec.join(", ");
    if !genres_str.is_empty() {
        fields.push(("Genres", &genres_str, true));
    }

    let movie_id = if is_movie { media_obj.and_then(|m| m.get("id")).and_then(|v| v.as_i64()) } else { None };
    let series_id = if !is_movie { media_obj.and_then(|m| m.get("id")).and_then(|v| v.as_i64()) } else { None };
    let tmdb_id = media_obj.and_then(|m| m.get("tmdbId")).and_then(|v| v.as_i64());
    let tvdb_id = media_obj.and_then(|m| m.get("tvdbId")).and_then(|v| v.as_i64());
    let imdb_id = media_obj.and_then(|m| m.get("imdbId")).and_then(|v| v.as_str()).map(|s| s.to_string());

    let existing_grab = state.db.find_arr_grab_by_ids_or_title(movie_id, series_id, imdb_id.as_deref(), tmdb_id, tvdb_id, Some(title)).ok().flatten();
    let existing_ombi = state.db.find_ombi_request_by_title_or_ids(title, imdb_id.as_deref(), tmdb_id, tvdb_id).ok().flatten();
    let existing_post_id = existing_grab.as_ref().and_then(|g| g.mattermost_post_id.clone())
        .or_else(|| existing_ombi.as_ref().and_then(|o| o.mattermost_post_id.clone()));

    let post_id_opt = NotificationManager::dispatch_rich_or_update(
        &state.config.get().await.notifications,
        &format!("{}.added", app_name),
        zone_id,
        existing_post_id.as_deref(),
        None,
        &notif_title,
        overview_text,
        poster_url.as_deref(),
        fields,
        Some("#8B5CF6"),
    ).await;

    let final_post_id = post_id_opt.or(existing_post_id);

    let grab_id = existing_grab.as_ref().map(|g| g.id.clone())
        .or_else(|| existing_ombi.as_ref().map(|o| format!("ombi_{}", o.id)))
        .unwrap_or_else(|| format!("track_{}_{}", app_name, uuid::Uuid::new_v4()));

    let grab_record = ArrGrabRecord {
        id: grab_id,
        scene_name: title.to_string(),
        release_title: title.to_string(),
        event_type: if is_movie { "MovieAdded".to_string() } else { "SeriesAdd".to_string() },
        item_type: if is_movie { "movie".to_string() } else { "series".to_string() },
        series_id,
        movie_id,
        artist_id: None,
        album_id: None,
        zone_id: zone_id.map(|s| s.to_string()),
        season_number: None,
        episode_numbers: None,
        episode_ids: None,
        indexer: None,
        download_client: None,
        download_id: None,
        status: existing_grab.as_ref().map(|g| g.status.clone()).unwrap_or_else(|| "wanted".to_string()),
        re_searched: false,
        re_search_count: 0,
        title: Some(title.to_string()),
        year,
        overview: overview.map(|s| s.to_string()),
        poster_url: poster_url.clone(),
        genres: if genres_str.is_empty() { None } else { Some(genres_str) },
        quality: None,
        size_bytes: None,
        imdb_id,
        tmdb_id,
        tvdb_id,
        runtime_mins: None,
        rating: None,
        mattermost_post_id: final_post_id.clone(),
        payload_json: payload.to_string(),
        created_at: existing_grab.as_ref().map(|g| g.created_at).unwrap_or_else(Utc::now),
        updated_at: Utc::now(),
    };

    let _ = state.db.save_arr_grab(&grab_record);

    if let (Some(mut ombi), Some(ref pid)) = (existing_ombi, &final_post_id) {
        ombi.mattermost_post_id = Some(pid.clone());
        let _ = state.db.save_ombi_request(&ombi);
    }

    Ok(Json(json!({"status": "ok", "event": "Added", "title": title})))
}

async fn handle_arr_media_deleted(
    state: &AppState,
    app_name: &str,
    payload: &serde_json::Value,
    zone_id: Option<&str>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_name = payload.get("instanceName").and_then(|v| v.as_str()).unwrap_or(app_name);
    let is_movie = payload.get("movie").is_some();
    let media_obj = payload.get("movie").or_else(|| payload.get("series"));

    let title = media_obj.and_then(|m| m.get("title")).and_then(|t| t.as_str()).unwrap_or("Unknown Title");
    let year = media_obj.and_then(|m| m.get("year")).and_then(|y| y.as_i64());
    let deleted_files = payload.get("deletedFiles").and_then(|v| v.as_bool()).unwrap_or(false);
    let folder_size = payload.get("movieFolderSize").and_then(|s| s.as_i64());

    let log_msg = format!("{} removed from {}: '{}' (Files deleted: {})", if is_movie { "Movie" } else { "Series" }, instance_name, title, deleted_files);
    let _ = state.db.log_event(app_name, "info", &log_msg, Some(&payload.to_string()));

    let notif_title = if let Some(y) = year {
        format!("🗑️ Conduit Removed • {} ({})", title, y)
    } else {
        format!("🗑️ Conduit Removed • {}", title)
    };

    let overview = if deleted_files {
        if let Some(sz) = folder_size.filter(|&s| s > 0) {
            format!("Removed from {} library and deleted from storage.\n**Space Freed:** {}", instance_name, format_bytes_str(sz))
        } else {
            format!("Removed from {} library and cleaned from disk storage.", instance_name)
        }
    } else {
        format!("Removed from {} library (files preserved on disk).", instance_name)
    };

    let type_str = if is_movie { "Movie" } else { "TV Series" };
    let del_str = if deleted_files { "Yes (Cleaned)" } else { "No (Preserved)" };
    let fields: Vec<(&str, &str, bool)> = vec![
        ("Type", type_str, true),
        ("Files Deleted", del_str, true),
    ];

    let _ = NotificationManager::dispatch_rich(
        &state.config.get().await.notifications,
        &format!("{}.delete", app_name),
        zone_id,
        &notif_title,
        &overview,
        None,
        fields,
        Some("#64748B"),
    ).await;

    let movie_id = if is_movie { media_obj.and_then(|m| m.get("id")).and_then(|v| v.as_i64()) } else { None };
    let series_id = if !is_movie { media_obj.and_then(|m| m.get("id")).and_then(|v| v.as_i64()) } else { None };
    let tmdb_id = media_obj.and_then(|m| m.get("tmdbId")).and_then(|v| v.as_i64());
    let imdb_id = media_obj.and_then(|m| m.get("imdbId")).and_then(|v| v.as_str());
    let tvdb_id = media_obj.and_then(|m| m.get("tvdbId")).and_then(|v| v.as_i64());
    let overview_text = media_obj.and_then(|m| m.get("overview")).and_then(|o| o.as_str()).map(|s| s.to_string());
    let poster_url = extract_poster_url(media_obj);

    let existing_grab = state.db.find_arr_grab_by_ids_or_title(movie_id, series_id, imdb_id, tmdb_id, tvdb_id, Some(title)).ok().flatten();
    let now = Utc::now();
    let event_type_name = if is_movie { "MovieDelete" } else { "SeriesDelete" };

    if let Some(mut existing) = existing_grab {
        existing.status = "deleted".to_string();
        existing.event_type = event_type_name.to_string();
        existing.updated_at = now;
        let _ = state.db.save_arr_grab(&existing);
    } else {
        let grab_id = format!("del_{}_{}", app_name, uuid::Uuid::new_v4());
        let record = ArrGrabRecord {
            id: grab_id,
            scene_name: title.to_string(),
            release_title: title.to_string(),
            event_type: event_type_name.to_string(),
            item_type: if is_movie { "movie".to_string() } else { "series".to_string() },
            series_id,
            movie_id,
            artist_id: None,
            album_id: None,
            zone_id: zone_id.map(|s| s.to_string()),
            season_number: None,
            episode_numbers: None,
            episode_ids: None,
            indexer: None,
            download_client: None,
            download_id: None,
            status: "deleted".to_string(),
            re_searched: false,
            re_search_count: 0,
            title: Some(title.to_string()),
            year,
            overview: overview_text,
            poster_url,
            genres: None,
            quality: None,
            size_bytes: folder_size,
            imdb_id: imdb_id.map(|s| s.to_string()),
            tmdb_id,
            tvdb_id,
            runtime_mins: None,
            rating: None,
            mattermost_post_id: None,
            payload_json: payload.to_string(),
            created_at: now,
            updated_at: now,
        };
        let _ = state.db.save_arr_grab(&record);
    }

    Ok(Json(json!({"status": "ok", "event": "Deleted", "title": title})))
}

async fn handle_arr_rename(
    state: &AppState,
    app_name: &str,
    payload: &serde_json::Value,
    zone_id: Option<&str>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_name = payload.get("instanceName").and_then(|v| v.as_str()).unwrap_or(app_name);
    let media_obj = payload.get("movie").or_else(|| payload.get("series"));

    let title = media_obj.and_then(|m| m.get("title")).and_then(|t| t.as_str()).unwrap_or("Media");
    let year = media_obj.and_then(|m| m.get("year")).and_then(|y| y.as_i64());

    let count = payload.get("renamedMovieFiles").or_else(|| payload.get("renamedEpisodeFiles"))
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(1);

    let log_msg = format!("Renamed {} file(s) for '{}' in {}", count, title, instance_name);
    let _ = state.db.log_event(app_name, "info", &log_msg, Some(&payload.to_string()));

    let notif_title = if let Some(y) = year {
        format!("📝 Conduit Renamed • {} ({})", title, y)
    } else {
        format!("📝 Conduit Renamed • {}", title)
    };

    let overview = format!("Renamed and organized {} file(s) according to library naming schema.", count);

    let count_str = count.to_string();
    let fields: Vec<(&str, &str, bool)> = vec![
        ("Files Renamed", &count_str, true),
        ("Instance", instance_name, true),
    ];

    let _ = NotificationManager::dispatch_rich(
        &state.config.get().await.notifications,
        &format!("{}.rename", app_name),
        zone_id,
        &notif_title,
        &overview,
        None,
        fields,
        Some("#6366F1"),
    ).await;

    Ok(Json(json!({"status": "ok", "event": "Rename", "count": count})))
}

async fn handle_lidarr_artist_added(
    state: &AppState,
    payload: &serde_json::Value,
    zone_id: Option<&str>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_name = payload.get("instanceName").and_then(|v| v.as_str()).unwrap_or("lidarr");
    let artist = payload.get("artist");

    let title = artist.and_then(|a| a.get("name")).and_then(|t| t.as_str()).unwrap_or("Unknown Artist");
    let overview = artist.and_then(|a| a.get("overview")).and_then(|o| o.as_str());
    let poster_url = extract_poster_url(artist);

    let log_msg = format!("Artist added to {}: '{}'", instance_name, title);
    let _ = state.db.log_event("lidarr", "info", &log_msg, Some(&payload.to_string()));

    let notif_title = format!("🐕 Conduit Tracking • {}", title);
    let overview_text = overview.unwrap_or("Added to library. Conduit will monitor indexers for available releases.");

    let mut fields: Vec<(&str, &str, bool)> = Vec::new();
    fields.push(("Type", "Artist", true));
    let genres_vec: Vec<String> = artist.and_then(|a| a.get("genres")).and_then(|g| g.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let genres_str = genres_vec.join(", ");
    if !genres_str.is_empty() {
        fields.push(("Genres", &genres_str, true));
    }

    let artist_id = artist.and_then(|a| a.get("id")).and_then(|v| v.as_i64());

    let existing_grab = state.db.find_arr_grab_by_ids_or_title(None, None, None, None, None, Some(title)).ok().flatten();
    let existing_ombi = state.db.find_ombi_request_by_title_or_ids(title, None, None, None).ok().flatten();
    let existing_post_id = existing_grab.as_ref().and_then(|g| g.mattermost_post_id.clone())
        .or_else(|| existing_ombi.as_ref().and_then(|o| o.mattermost_post_id.clone()));

    let post_id_opt = NotificationManager::dispatch_rich_or_update(
        &state.config.get().await.notifications,
        "lidarr.added",
        zone_id,
        existing_post_id.as_deref(),
        None,
        &notif_title,
        overview_text,
        poster_url.as_deref(),
        fields,
        Some("#8B5CF6"),
    ).await;

    let final_post_id = post_id_opt.or(existing_post_id);

    let grab_id = existing_grab.as_ref().map(|g| g.id.clone())
        .or_else(|| existing_ombi.as_ref().map(|o| format!("ombi_{}", o.id)))
        .unwrap_or_else(|| format!("track_lidarr_{}", uuid::Uuid::new_v4()));

    let grab_record = ArrGrabRecord {
        id: grab_id,
        scene_name: title.to_string(),
        release_title: title.to_string(),
        event_type: "ArtistAdd".to_string(),
        item_type: "music".to_string(),
        series_id: None,
        movie_id: None,
        artist_id,
        album_id: None,
        zone_id: zone_id.map(|s| s.to_string()),
        season_number: None,
        episode_numbers: None,
        episode_ids: None,
        indexer: None,
        download_client: None,
        download_id: None,
        status: existing_grab.as_ref().map(|g| g.status.clone()).unwrap_or_else(|| "wanted".to_string()),
        re_searched: false,
        re_search_count: 0,
        title: Some(title.to_string()),
        year: None,
        overview: overview.map(|s| s.to_string()),
        poster_url: poster_url.clone(),
        genres: if genres_str.is_empty() { None } else { Some(genres_str) },
        quality: None,
        size_bytes: None,
        imdb_id: None,
        tmdb_id: None,
        tvdb_id: None,
        runtime_mins: None,
        rating: None,
        mattermost_post_id: final_post_id.clone(),
        payload_json: payload.to_string(),
        created_at: existing_grab.as_ref().map(|g| g.created_at).unwrap_or_else(Utc::now),
        updated_at: Utc::now(),
    };

    let _ = state.db.save_arr_grab(&grab_record);

    if let (Some(mut ombi), Some(ref pid)) = (existing_ombi, &final_post_id) {
        ombi.mattermost_post_id = Some(pid.clone());
        let _ = state.db.save_ombi_request(&ombi);
    }

    Ok(Json(json!({"status": "ok", "event": "ArtistAdd", "title": title})))
}

async fn handle_lidarr_deleted(
    state: &AppState,
    event_type: &str,
    payload: &serde_json::Value,
    zone_id: Option<&str>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_name = payload.get("instanceName").and_then(|v| v.as_str()).unwrap_or("lidarr");
    let artist = payload.get("artist");
    let artist_name = artist.and_then(|a| a.get("name")).and_then(|t| t.as_str()).unwrap_or("Unknown Artist");
    let album_title = payload.get("album").and_then(|a| a.get("title")).and_then(|t| t.as_str());
    let deleted_files = payload.get("deletedFiles").and_then(|v| v.as_bool()).unwrap_or(false);

    let title = album_title.map(|a| format!("{} - {}", artist_name, a)).unwrap_or_else(|| artist_name.to_string());

    let log_msg = format!("{} removed from {}: '{}' (Files deleted: {})", event_type, instance_name, title, deleted_files);
    let _ = state.db.log_event("lidarr", "info", &log_msg, Some(&payload.to_string()));

    let notif_title = format!("🗑️ Conduit Removed • {}", title);
    let overview = if deleted_files {
        format!("Removed from {} and deleted files from disk.", instance_name)
    } else {
        format!("Removed from {} (files kept on disk).", instance_name)
    };

    let mut fields: Vec<(&str, &str, bool)> = Vec::new();
    let type_str = if event_type.eq_ignore_ascii_case("AlbumDelete") { "Album" } else { "Artist" };
    fields.push(("Type", type_str, true));
    let deleted_str = if deleted_files { "Yes" } else { "No" };
    fields.push(("Files Deleted", deleted_str, true));

    let _ = NotificationManager::dispatch_rich(
        &state.config.get().await.notifications,
        "lidarr.deleted",
        zone_id,
        &notif_title,
        &overview,
        None,
        fields,
        Some("#EF4444"),
    ).await;

    let artist_id = artist.and_then(|a| a.get("id")).and_then(|v| v.as_i64());
    let album_id = payload.get("album").and_then(|a| a.get("id")).and_then(|v| v.as_i64());
    let existing_grab = state.db.find_arr_grab_by_ids_or_title(None, None, None, None, None, Some(&title)).ok().flatten();
    let now = Utc::now();

    if let Some(mut existing) = existing_grab {
        existing.status = "deleted".to_string();
        existing.event_type = event_type.to_string();
        existing.updated_at = now;
        let _ = state.db.save_arr_grab(&existing);
    } else {
        let grab_id = format!("del_lidarr_{}", uuid::Uuid::new_v4());
        let record = ArrGrabRecord {
            id: grab_id,
            scene_name: title.clone(),
            release_title: title.clone(),
            event_type: event_type.to_string(),
            item_type: "music".to_string(),
            series_id: None,
            movie_id: None,
            artist_id,
            album_id,
            zone_id: zone_id.map(|s| s.to_string()),
            season_number: None,
            episode_numbers: None,
            episode_ids: None,
            indexer: None,
            download_client: None,
            download_id: None,
            status: "deleted".to_string(),
            re_searched: false,
            re_search_count: 0,
            title: Some(title.clone()),
            year: None,
            overview: None,
            poster_url: extract_poster_url(artist),
            genres: None,
            quality: None,
            size_bytes: None,
            imdb_id: None,
            tmdb_id: None,
            tvdb_id: None,
            runtime_mins: None,
            rating: None,
            mattermost_post_id: None,
            payload_json: payload.to_string(),
            created_at: now,
            updated_at: now,
        };
        let _ = state.db.save_arr_grab(&record);
    }

    Ok(Json(json!({"status": "ok", "event": event_type, "title": title})))
}

async fn handle_lidarr_download_failure(
    state: &AppState,
    event_type: &str,
    payload: &serde_json::Value,
    zone_id: Option<&str>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_name = payload.get("instanceName").and_then(|v| v.as_str()).unwrap_or("lidarr");
    // DownloadFailure carries no `artist` at all (flat payload); ImportFailure does.
    let artist_name = payload.get("artist").and_then(|a| a.get("name")).and_then(|t| t.as_str());
    let release_title = payload.get("releaseTitle").and_then(|v| v.as_str())
        .or_else(|| payload.get("release").and_then(|r| r.get("releaseTitle")).and_then(|v| v.as_str()))
        .unwrap_or("Release");
    let download_client = payload.get("downloadClient").and_then(|v| v.as_str()).unwrap_or("Download Client");
    let poster_url = extract_poster_url(payload.get("artist"));

    let display_title = artist_name.unwrap_or(release_title);
    let log_msg = format!("{} for '{}' in {} ({})", event_type, display_title, instance_name, release_title);
    let _ = state.db.log_event("lidarr", "warn", &log_msg, Some(&payload.to_string()));

    let notif_title = format!("🐕 Conduit Needs Help • {}", display_title);
    let overview = format!(
        "**{} in {}:**\nRelease `{}` failed and needs manual attention in the Lidarr queue.",
        event_type, instance_name, release_title
    );

    let fields: Vec<(&str, &str, bool)> = vec![
        ("Client", download_client, true),
        ("Release", release_title, true),
    ];

    let _ = NotificationManager::dispatch_rich(
        &state.config.get().await.notifications,
        "lidarr.failure",
        zone_id,
        &notif_title,
        &overview,
        poster_url.as_deref(),
        fields,
        Some("#F43F5E"),
    ).await;

    Ok(Json(json!({"status": "ok", "event": event_type})))
}

pub fn verify_webhook_secret(headers: &HeaderMap, expected: Option<&str>) -> bool {
    let expected = match expected {
        Some(s) if !s.trim().is_empty() => s.trim(),
        _ => return true,
    };
    let provided = headers
        .get("X-Webhook-Secret")
        .or_else(|| headers.get("x-webhook-secret"))
        .or_else(|| headers.get("X-Sonarr-Secret"))
        .or_else(|| headers.get("X-Radarr-Secret"))
        .or_else(|| headers.get("X-Lidarr-Secret"))
        .or_else(|| headers.get("X-Plex-Token"))
        .or_else(|| headers.get("Access-Token"))
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    use subtle::ConstantTimeEq;
    provided.as_bytes().ct_eq(expected.as_bytes()).into()
}

/// Optional `?zone=<id>` on the Sonarr/Radarr/Lidarr inbound webhook routes — each configured
/// zone's Sonarr/Radarr/Lidarr instance is set up with a URL carrying its own zone id, so Conduit
/// can tell which zone a given webhook belongs to. Absent (or not matching any configured zone)
/// falls back to the pre-zones legacy behavior: `config.{sonarr,radarr,lidarr}.primary` +
/// that app's single global `webhook_secret`, with `zone_id` left `None` on the saved grab.
#[derive(Debug, Deserialize, IntoParams)]
pub struct InboundZoneParams {
    pub zone: Option<String>,
}

/// Resolves the `?zone=` param against `config.zones`, returning the matching zone (if any) —
/// shared by all three Sonarr/Radarr/Lidarr inbound handlers.
fn resolve_zone<'a>(config: &'a crate::config::AppConfig, zone_param: Option<&str>) -> Option<&'a crate::config::ZoneConfig> {
    let zone_param = zone_param?;
    config.zones.iter().find(|z| z.id == zone_param)
}

/// Which Plex node(s) the notify-takeover should call for a given import: when the webhook
/// carried a known `zone_id`, only that zone's `plex_node_names`; otherwise (no zones
/// configured, or this webhook predates zones) every enabled Plex node, as before.
fn zone_scoped_plex_nodes(config: &crate::config::AppConfig, zone_id: Option<&str>, plex: &crate::config::PlexConfig) -> Vec<crate::config::PlexNodeConfig> {
    let Some(zone_id) = zone_id else {
        return plex.nodes.clone();
    };
    let Some(zone) = config.zones.iter().find(|z| z.id == zone_id) else {
        return plex.nodes.clone();
    };
    if zone.plex_node_names.is_empty() {
        return plex.nodes.clone();
    }
    plex.nodes.iter().filter(|n| zone.plex_node_names.contains(&n.name)).cloned().collect()
}

#[utoipa::path(
    post,
    path = "/api/sonarr/inbound",
    tag = "Arr Tracking",
    summary = "Sonarr webhook receiver",
    description = "Ingests Sonarr event webhooks (Grab, Download, EpisodeFileDelete, Rename, Health, SeriesAdd, SeriesDelete, ManualInteractionRequired). See docs/webhook_reference.md for the full per-event field reference. Configure `sonarr.webhook_secret` and check for an `X-Sonarr-Secret`/`X-Webhook-Secret` header if set — or, for a zone-specific instance, point Sonarr at this URL with `?zone=<id>` (see `zones`) and configure that zone's own `webhook_secret` instead.",
    params(InboundZoneParams),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Webhook processed successfully"),
        (status = 401, description = "Invalid or missing webhook secret (only enforced if sonarr.webhook_secret is configured)")
    )
)]
pub async fn sonarr_inbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(zone_q): Query<InboundZoneParams>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = state.config.get().await;
    let zone = resolve_zone(&config, zone_q.zone.as_deref());
    let (expected_secret, zone_id) = match zone {
        Some(z) => (z.webhook_secret.clone(), Some(z.id.clone())),
        None => (config.sonarr.webhook_secret.clone(), None),
    };
    if !verify_webhook_secret(&headers, expected_secret.as_deref()) {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"status": "error", "message": "Invalid Sonarr webhook secret"}))));
    }

    debug!("Sonarr inbound webhook payload (zone={:?}): {:?}", zone_id, payload);

    let event_type = payload.get("eventType").and_then(|v| v.as_str()).unwrap_or("Unknown");
    if event_type.eq_ignore_ascii_case("Test") {
        let _ = state.db.log_event("sonarr", "info", "Sonarr test webhook connection received & verified", None);
        return Ok(Json(json!({"status": "ok", "message": "Sonarr test connection verified"})));
    }

    if event_type.eq_ignore_ascii_case("Health") || event_type.eq_ignore_ascii_case("HealthRestored") {
        return handle_arr_health_event(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ApplicationUpdate") {
        return handle_arr_app_update(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ManualInteractionRequired") {
        return handle_arr_manual_interaction(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("SeriesAdd") {
        return handle_arr_media_added(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("SeriesDelete") {
        return handle_arr_media_deleted(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("Rename") {
        return handle_arr_rename(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }

    // Auto-detect media type and forward once without loop
    let detected_type = detect_inbound_media_type(&payload, &config.queue_routing);
    if detected_type == "music" {
        debug!("Sonarr inbound webhook detected music payload/tracker -> routing to music handler");
        return Box::pin(process_lidarr_inbound_direct(state, payload, zone_id)).await;
    } else if detected_type == "movie" {
        debug!("Sonarr inbound webhook detected movie payload/tracker -> routing to movie handler");
        return Box::pin(process_radarr_inbound_direct(state, payload, zone_id)).await;
    }

    process_sonarr_inbound_direct(state, payload, zone_id).await
}

pub async fn process_sonarr_inbound_direct(
    state: AppState,
    payload: serde_json::Value,
    zone_id: Option<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let event_type = payload.get("eventType").and_then(|v| v.as_str()).unwrap_or("Unknown");

    if event_type.eq_ignore_ascii_case("Test") {
        let _ = state.db.log_event("sonarr", "info", "Sonarr test webhook connection received & verified", None);
        return Ok(Json(json!({"status": "ok", "message": "Sonarr test connection verified"})));
    }
    if event_type.eq_ignore_ascii_case("Health") || event_type.eq_ignore_ascii_case("HealthRestored") {
        return handle_arr_health_event(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ApplicationUpdate") {
        return handle_arr_app_update(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ManualInteractionRequired") {
        return handle_arr_manual_interaction(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("SeriesAdd") {
        return handle_arr_media_added(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("SeriesDelete") {
        return handle_arr_media_deleted(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("Rename") {
        return handle_arr_rename(&state, "sonarr", &payload, zone_id.as_deref()).await;
    }

    let series = payload.get("series");
    let series_id = series.and_then(|s| s.get("id")).and_then(|v| v.as_i64());
    let series_title = series.and_then(|s| s.get("title")).and_then(|v| v.as_str()).unwrap_or("Unknown Series");
    let year = series.and_then(|s| s.get("year")).and_then(|y| y.as_i64());
    let overview = series.and_then(|s| s.get("overview")).and_then(|o| o.as_str()).map(|s| s.to_string());
    let tvdb_id = series.and_then(|s| s.get("tvdbId")).and_then(|v| v.as_i64());
    let imdb_id = series.and_then(|s| s.get("imdbId")).and_then(|v| v.as_str()).map(|s| s.to_string());
    let runtime_mins = series.and_then(|s| s.get("runtime")).and_then(|v| v.as_i64());
    let rating = series.and_then(|s| s.get("ratings")).and_then(|r| r.get("value")).and_then(|v| v.as_f64());

    let poster_url = extract_poster_url(series);

    let genres_vec: Vec<String> = series.and_then(|s| s.get("genres")).and_then(|g| g.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let genres = if genres_vec.is_empty() { None } else { Some(genres_vec.join(", ")) };

    let is_batch = payload.get("episodeFiles").and_then(|f| f.as_array()).map(|a| a.len() > 1).unwrap_or(false);
    let batch_file_count = payload.get("episodeFiles").and_then(|f| f.as_array()).map(|a| a.len()).unwrap_or(0);
    let batch_total_size: i64 = payload.get("episodeFiles").and_then(|f| f.as_array())
        .map(|arr| arr.iter().filter_map(|f| f.get("size").and_then(|s| s.as_i64())).sum())
        .unwrap_or(0);

    let episodes = payload.get("episodes").and_then(|e| e.as_array());
    let season_number = episodes.and_then(|arr| arr.first()).and_then(|e| e.get("seasonNumber")).and_then(|v| v.as_i64())
        .or_else(|| payload.get("episodeFile").and_then(|f| f.get("seasonNumber")).and_then(|v| v.as_i64()));

    let episode_numbers: Vec<i64> = episodes
        .map(|arr| arr.iter().filter_map(|e| e.get("episodeNumber").and_then(|v| v.as_i64())).collect())
        .unwrap_or_default();

    let episode_ids: Vec<i64> = episodes
        .map(|arr| arr.iter().filter_map(|e| e.get("id").and_then(|v| v.as_i64())).collect())
        .unwrap_or_default();

    let episode_file = payload.get("episodeFile")
        .or_else(|| payload.get("episodeFiles").and_then(|f| f.as_array()).and_then(|a| a.first()));
    let episode_source_path = payload.get("sourcePath").and_then(|v| v.as_str())
        .or_else(|| episode_file.and_then(|f| f.get("sourcePath")).and_then(|v| v.as_str()));
    let episode_scene_name = episode_file.and_then(|f| f.get("sceneName")).and_then(|v| v.as_str());

    let source_stem = episode_source_path.and_then(|p| {
        let p_clean = p.trim();
        if p_clean.is_empty() { return None; }
        let filename = std::path::Path::new(p_clean).file_name().and_then(|n| n.to_str()).unwrap_or(p_clean);
        let mut stem = filename;
        for ext in &[".mkv", ".mp4", ".avi", ".ts"] {
            if let Some(s) = stem.strip_suffix(ext) {
                stem = s;
            }
        }
        if !stem.is_empty() { Some(stem.to_string()) } else { None }
    });

    let release = payload.get("release");
    let release_title = release
        .and_then(|r| r.get("releaseTitle"))
        .and_then(|v| v.as_str())
        .or(episode_scene_name)
        .or(source_stem.as_deref())
        .unwrap_or(series_title);

    let raw_indexer = release.and_then(|r| r.get("indexer")).and_then(|v| v.as_str());
    let download_client = payload.get("downloadClient").and_then(|v| v.as_str()).map(|s| s.to_string());
    let download_id = payload.get("downloadId").and_then(|v| v.as_str()).map(|s| s.to_string());

    let raw_quality = release.and_then(|r| r.get("quality"))
        .and_then(|q| if q.is_string() { q.as_str() } else { q.get("name").and_then(|n| n.as_str()) })
        .or_else(|| episode_file.and_then(|f| f.get("quality")).and_then(|q| q.get("name").and_then(|n| n.as_str())));

    let raw_size_bytes = if is_batch && batch_total_size > 0 {
        Some(batch_total_size)
    } else {
        release.and_then(|r| r.get("size")).and_then(|s| s.as_i64())
            .or_else(|| episode_file.and_then(|f| f.get("size")).and_then(|s| s.as_i64()))
    };

    let delete_reason = payload.get("deleteReason").and_then(|r| r.as_str()).unwrap_or("Upgrade");

    let status = match event_type {
        "Grab" => "fetched",
        "Download" => "imported",
        "EpisodeFileDelete" => "deleted",
        _ => "tracked",
    };

    // Notify-takeover: replaces Sonarr's own built-in "Plex Media Server" connection. On import,
    // tell Plex to rescan just the affected show folder — same targeted-refresh behavior Sonarr
    // does itself (see 3rd/Sonarr/.../Plex/Server/PlexServerService.cs), just triggered by Conduit
    // instead so Sonarr's own Plex connection can be disabled.
    if status == "imported" {
        if let Some(series_path) = series.and_then(|s| s.get("path")).and_then(|v| v.as_str()).map(|s| s.to_string()) {
            let full_config = state.config.get().await;
            let plex = full_config.plex.clone();
            // Scope to this zone's Plex node(s) when known, instead of every configured node.
            let target_nodes = zone_scoped_plex_nodes(&full_config, zone_id.as_deref(), &plex);
            if plex.enabled && plex.notify_on_import && !target_nodes.is_empty() {
                tokio::spawn(async move {
                    crate::plex_client::notify_library_update(&target_nodes, &plex.token, &plex.client_identifier, &series_path, "show").await;
                });
            }
        }
    }

    let existing_grab = state.db.find_arr_grab_by_hash_or_name(download_id.as_deref().unwrap_or(""), release_title).ok().flatten()
        .or_else(|| {
            if let Some(src) = episode_source_path {
                state.db.find_arr_grab_by_hash_or_name("", src).ok().flatten()
            } else {
                None
            }
        })
        .or_else(|| {
            let ep_id = episode_ids.first().copied();
            let ep_num = episode_numbers.first().copied();
            state.db.find_arr_grab_by_series_episode(series_id, season_number, ep_num, ep_id).ok().flatten()
        })
        .or_else(|| state.db.find_arr_grab_by_ids_or_title(None, series_id, imdb_id.as_deref(), None, tvdb_id, Some(series_title)).ok().flatten());

    let grab_id = existing_grab.as_ref().map(|g| g.id.clone())
        .or(download_id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = Utc::now();

    let existing_ombi = state.db.find_ombi_request_by_title_or_ids(series_title, imdb_id.as_deref(), None, tvdb_id).ok().flatten();
    let existing_post_id = existing_grab.as_ref().and_then(|g| g.mattermost_post_id.clone())
        .or_else(|| existing_ombi.as_ref().and_then(|o| o.mattermost_post_id.clone()));

    let final_release_title = if release_title != series_title && !release_title.is_empty() {
        release_title.to_string()
    } else if let Some(ref existing) = existing_grab {
        existing.release_title.clone()
    } else {
        release_title.to_string()
    };

    let final_scene_name = if let Some(ref existing) = existing_grab {
        if !existing.scene_name.is_empty() && existing.scene_name != series_title {
            existing.scene_name.clone()
        } else {
            final_release_title.clone()
        }
    } else {
        final_release_title.clone()
    };

    let final_download_id = download_id.or_else(|| existing_grab.as_ref().and_then(|g| g.download_id.clone()));
    let final_quality = infer_quality_from_release(&final_release_title, raw_quality);
    let final_indexer = infer_indexer_source(raw_indexer, existing_grab.as_ref().and_then(|g| g.indexer.as_deref()), &final_release_title, &state.config.get().await.queue_routing);
    let final_size_bytes = raw_size_bytes.or_else(|| existing_grab.as_ref().and_then(|g| g.size_bytes));
    let created_time = existing_grab.as_ref().map(|g| g.created_at).unwrap_or(now);
    let final_download_client = existing_grab.as_ref()
        .and_then(|g| g.download_client.clone())
        .filter(|dc| !dc.eq_ignore_ascii_case("Transmission") && !dc.is_empty())
        .or(download_client)
        .unwrap_or_else(|| "Transmission".to_string());

    let mut record = ArrGrabRecord {
        id: grab_id.clone(),
        scene_name: final_scene_name,
        release_title: final_release_title.clone(),
        event_type: event_type.to_string(),
        item_type: "series".to_string(),
        series_id,
        movie_id: None,
        artist_id: None,
        album_id: None,
        zone_id: zone_id.clone(),
        season_number,
        episode_numbers: if episode_numbers.is_empty() { None } else { Some(serde_json::to_string(&episode_numbers).unwrap_or_default()) },
        episode_ids: if episode_ids.is_empty() { None } else { Some(serde_json::to_string(&episode_ids).unwrap_or_default()) },
        indexer: Some(final_indexer.clone()),
        download_client: Some(final_download_client.clone()),
        download_id: final_download_id,
        status: status.to_string(),
        re_searched: false,
        re_search_count: 0,
        title: Some(series_title.to_string()),
        year,
        overview: overview.clone(),
        poster_url: poster_url.clone(),
        genres: genres.clone(),
        quality: Some(final_quality.clone()),
        size_bytes: final_size_bytes,
        imdb_id,
        tmdb_id: None,
        tvdb_id,
        runtime_mins,
        rating,
        mattermost_post_id: existing_post_id.clone(),
        payload_json: payload.to_string(),
        created_at: created_time,
        updated_at: now,
    };

    let _ = state.db.save_arr_grab(&record);
    let _ = state.db.log_event(
        "sonarr",
        "info",
        &format!("Sonarr [{}] for '{}'", event_type, release_title),
        Some(&payload.to_string()),
    );

    // Format Conduit-Themed Living Card & Timeline
    let (action_label, color_hex) = match event_type {
        "Grab" => ("Conduit Grabbed", "#10B981"),
        "Download" => ("Conduit Retrieved & Stored", "#3B82F6"),
        "EpisodeFileDelete" => {
            if delete_reason.eq_ignore_ascii_case("MissingFromDisk") {
                ("⚠️ File Missing From Disk", "#F59E0B")
            } else {
                ("Conduit Deleted Episode", "#EF4444")
            }
        },
        _ => ("Conduit Tracked Series", "#F59E0B"),
    };

    let notif_title = if is_batch {
        format!("{} • {} Season {:02} ({} episodes)", action_label, series_title, season_number.unwrap_or(1), batch_file_count)
    } else {
        format!("{} • {} ({})", action_label, series_title, year.unwrap_or(0))
    };
    let notif_overview = overview.as_deref().unwrap_or(release_title);

    let mut fields: Vec<(&str, &str, bool)> = Vec::new();
    fields.push(("Quality", &final_quality, true));
    fields.push(("Indexer", &final_indexer, true));

    let size_fmt = final_size_bytes.map(format_bytes_str).unwrap_or_else(|| "N/A".to_string());
    fields.push(("Size", &size_fmt, true));

    let ep_info = if is_batch {
        format!("Season {:02} ({} episodes)", season_number.unwrap_or(1), batch_file_count)
    } else {
        match (season_number, episode_numbers.first()) {
            (Some(s), Some(e)) => format!("S{:02}E{:02}", s, e),
            (Some(s), None) => format!("Season {:02}", s),
            _ => "Series".to_string(),
        }
    };
    fields.push(("Episode", &ep_info, true));

    if let Some(ref g) = genres {
        fields.push(("Genres", g, true));
    }
    fields.push(("Fetcher Node", &final_download_client, true));

    let media_info_str = format_media_info_summary(payload.get("episodeFile"));
    if let Some(ref mi) = media_info_str {
        fields.push(("Media Details", mi, true));
    }

    let flags_str = format_indexer_flags(&payload);
    if let Some(ref flg) = flags_str {
        fields.push(("Release Flags", flg, true));
    }

    let cf_str = format_custom_format_summary(&payload);
    if let Some(ref cf) = cf_str {
        fields.push(("Custom Formats", cf, true));
    }

    if event_type == "EpisodeFileDelete" && !delete_reason.eq_ignore_ascii_case("Upgrade") {
        fields.push(("Delete Reason", delete_reason, true));
    }

    // Structured Pipeline Lifecycle Timeline
    let timeline_str = if event_type == "Grab" {
        format!("📥 Grabbed: {}", now.format("%Y-%m-%d %H:%M:%S UTC"))
    } else if event_type == "Download" {
        format!(
            "📥 Grabbed: {}\n🎾 Staged: {}\n🏆 Imported: {}",
            created_time.format("%Y-%m-%d %H:%M:%S UTC"),
            now.format("%Y-%m-%d %H:%M:%S UTC"),
            now.format("%Y-%m-%d %H:%M:%S UTC")
        )
    } else {
        format!("🕒 Updated: {}", now.format("%Y-%m-%d %H:%M:%S UTC"))
    };
    fields.push(("Pipeline Timeline", &timeline_str, false));

    let is_actionable_event = matches!(event_type, "Grab" | "Download" | "EpisodeFileDelete");
    if is_actionable_event && series_title != "Unknown Series" {
        if event_type == "EpisodeFileDelete" && existing_post_id.is_some() && delete_reason.eq_ignore_ascii_case("Upgrade") {
            let root_id = existing_post_id.as_deref().unwrap();
            let thread_msg = format!("🐕 Conduit cleared previous episode file to make room for incoming upgrade/import: `{}`", release_title);
            let _ = NotificationManager::dispatch_rich_or_update(
                &state.config.get().await.notifications,
                "sonarr.delete.detail",
                zone_id.as_deref(),
                None,
                Some(root_id),
                "Episode Upgrade / Cleanup",
                &thread_msg,
                None,
                Vec::new(),
                Some(color_hex),
            ).await;
        } else {
            let post_id_opt = NotificationManager::dispatch_rich_or_update(
                &state.config.get().await.notifications,
                &format!("sonarr.{}", event_type.to_lowercase()),
                zone_id.as_deref(),
                existing_post_id.as_deref(),
                None,
                &notif_title,
                notif_overview,
                poster_url.as_deref(),
                fields,
                Some(color_hex),
            ).await;

            if let Some(pid) = post_id_opt {
                record.mattermost_post_id = Some(pid);
                let _ = state.db.save_arr_grab(&record);
            }
        }
    }

    Ok(Json(json!({"status": "ok", "event": event_type, "id": grab_id})))
}

#[utoipa::path(
    post,
    path = "/api/radarr/inbound",
    tag = "Arr Tracking",
    summary = "Radarr webhook receiver",
    description = "Ingests Radarr event webhooks (Grab, Download, MovieFileDelete, Rename, Health, MovieAdded, MovieDelete, ManualInteractionRequired). See docs/webhook_reference.md for the full per-event field reference. Configure `radarr.webhook_secret` and check for an `X-Radarr-Secret`/`X-Webhook-Secret` header if set — or, for a zone-specific instance, point Radarr at this URL with `?zone=<id>` (see `zones`) and configure that zone's own `webhook_secret` instead.",
    params(InboundZoneParams),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Webhook processed successfully"),
        (status = 401, description = "Invalid or missing webhook secret (only enforced if radarr.webhook_secret is configured)")
    )
)]
pub async fn radarr_inbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(zone_q): Query<InboundZoneParams>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = state.config.get().await;
    let zone = resolve_zone(&config, zone_q.zone.as_deref());
    let (expected_secret, zone_id) = match zone {
        Some(z) => (z.webhook_secret.clone(), Some(z.id.clone())),
        None => (config.radarr.webhook_secret.clone(), None),
    };
    if !verify_webhook_secret(&headers, expected_secret.as_deref()) {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"status": "error", "message": "Invalid Radarr webhook secret"}))));
    }

    debug!("Radarr inbound webhook payload (zone={:?}): {:?}", zone_id, payload);

    let event_type = payload.get("eventType").and_then(|v| v.as_str()).unwrap_or("Unknown");
    if event_type.eq_ignore_ascii_case("Test") {
        let _ = state.db.log_event("radarr", "info", "Radarr test webhook connection received & verified", None);
        return Ok(Json(json!({"status": "ok", "message": "Radarr test connection verified"})));
    }

    if event_type.eq_ignore_ascii_case("Health") || event_type.eq_ignore_ascii_case("HealthRestored") {
        return handle_arr_health_event(&state, "radarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ApplicationUpdate") {
        return handle_arr_app_update(&state, "radarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ManualInteractionRequired") {
        return handle_arr_manual_interaction(&state, "radarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("MovieAdded") {
        return handle_arr_media_added(&state, "radarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("MovieDelete") {
        return handle_arr_media_deleted(&state, "radarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("Rename") {
        return handle_arr_rename(&state, "radarr", &payload, zone_id.as_deref()).await;
    }

    // Auto-detect media type and forward once without loop
    let detected_type = detect_inbound_media_type(&payload, &config.queue_routing);
    if detected_type == "music" {
        debug!("Radarr inbound webhook detected music payload/tracker -> routing to music handler");
        return Box::pin(process_lidarr_inbound_direct(state, payload, zone_id)).await;
    } else if detected_type == "tv" {
        debug!("Radarr inbound webhook detected TV series payload/tracker -> routing to tv handler");
        return Box::pin(process_sonarr_inbound_direct(state, payload, zone_id)).await;
    }

    process_radarr_inbound_direct(state, payload, zone_id).await
}

pub async fn process_radarr_inbound_direct(
    state: AppState,
    payload: serde_json::Value,
    zone_id: Option<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let event_type = payload.get("eventType").and_then(|v| v.as_str()).unwrap_or("Unknown");

    if event_type.eq_ignore_ascii_case("Test") {
        let _ = state.db.log_event("radarr", "info", "Radarr test webhook connection received & verified", None);
        return Ok(Json(json!({"status": "ok", "message": "Radarr test connection verified"})));
    }
    if event_type.eq_ignore_ascii_case("Health") || event_type.eq_ignore_ascii_case("HealthRestored") {
        return handle_arr_health_event(&state, "radarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ApplicationUpdate") {
        return handle_arr_app_update(&state, "radarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ManualInteractionRequired") {
        return handle_arr_manual_interaction(&state, "radarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("MovieAdded") {
        return handle_arr_media_added(&state, "radarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("MovieDelete") {
        return handle_arr_media_deleted(&state, "radarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("Rename") {
        return handle_arr_rename(&state, "radarr", &payload, zone_id.as_deref()).await;
    }

    let movie = payload.get("movie");
    let movie_id = movie.and_then(|m| m.get("id")).and_then(|v| v.as_i64());
    let movie_title = movie.and_then(|m| m.get("title")).and_then(|v| v.as_str()).unwrap_or("Unknown Movie");
    let year = movie.and_then(|m| m.get("year")).and_then(|y| y.as_i64());
    let overview = movie.and_then(|m| m.get("overview")).and_then(|o| o.as_str()).map(|s| s.to_string());
    let tmdb_id = movie.and_then(|m| m.get("tmdbId")).and_then(|v| v.as_i64());
    let imdb_id = movie.and_then(|m| m.get("imdbId")).and_then(|v| v.as_str()).map(|s| s.to_string());
    let runtime_mins = movie.and_then(|m| m.get("runtime")).and_then(|v| v.as_i64());
    let rating = movie.and_then(|m| m.get("ratings")).and_then(|r| r.get("value")).and_then(|v| v.as_f64());

    let poster_url = extract_poster_url(movie);

    let genres_vec: Vec<String> = movie.and_then(|m| m.get("genres")).and_then(|g| g.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let genres = if genres_vec.is_empty() { None } else { Some(genres_vec.join(", ")) };

    let movie_file = payload.get("movieFile");
    let movie_source_path = payload.get("sourcePath").and_then(|v| v.as_str())
        .or_else(|| movie_file.and_then(|f| f.get("sourcePath")).and_then(|v| v.as_str()));
    let movie_scene_name = movie_file.and_then(|f| f.get("sceneName")).and_then(|v| v.as_str());

    let movie_source_stem = movie_source_path.and_then(|p| {
        let p_clean = p.trim();
        if p_clean.is_empty() { return None; }
        let filename = std::path::Path::new(p_clean).file_name().and_then(|n| n.to_str()).unwrap_or(p_clean);
        let mut stem = filename;
        for ext in &[".mkv", ".mp4", ".avi", ".ts"] {
            if let Some(s) = stem.strip_suffix(ext) {
                stem = s;
            }
        }
        if !stem.is_empty() { Some(stem.to_string()) } else { None }
    });

    let release = payload.get("release");
    let release_title = release
        .and_then(|r| r.get("releaseTitle"))
        .and_then(|v| v.as_str())
        .or(movie_scene_name)
        .or(movie_source_stem.as_deref())
        .unwrap_or(movie_title);

    let raw_indexer = release.and_then(|r| r.get("indexer")).and_then(|v| v.as_str());
    let download_client = payload.get("downloadClient").and_then(|v| v.as_str()).map(|s| s.to_string());
    let download_id = payload.get("downloadId").and_then(|v| v.as_str()).map(|s| s.to_string());

    let raw_quality = release.and_then(|r| r.get("quality"))
        .and_then(|q| if q.is_string() { q.as_str() } else { q.get("name").and_then(|n| n.as_str()) })
        .or_else(|| movie_file.and_then(|f| f.get("quality")).and_then(|q| q.get("name").and_then(|n| n.as_str())));

    let raw_size_bytes = release.and_then(|r| r.get("size")).and_then(|s| s.as_i64())
        .or_else(|| movie_file.and_then(|f| f.get("size")).and_then(|s| s.as_i64()));

    let delete_reason = payload.get("deleteReason").and_then(|r| r.as_str()).unwrap_or("Upgrade");

    let status = match event_type {
        "Grab" => "fetched",
        "Download" => "imported",
        "MovieFileDelete" => "deleted",
        _ => "tracked",
    };

    // Notify-takeover: replaces Radarr's own built-in "Plex Media Server" connection. On import,
    // tell Plex to rescan just the affected movie folder — same targeted-refresh behavior Radarr
    // does itself, just triggered by Conduit instead so Radarr's own Plex connection can be disabled.
    if status == "imported" {
        if let Some(movie_path) = movie.and_then(|m| m.get("folderPath")).and_then(|v| v.as_str()).map(|s| s.to_string()) {
            let full_config = state.config.get().await;
            let plex = full_config.plex.clone();
            let target_nodes = zone_scoped_plex_nodes(&full_config, zone_id.as_deref(), &plex);
            if plex.enabled && plex.notify_on_import && !target_nodes.is_empty() {
                tokio::spawn(async move {
                    crate::plex_client::notify_library_update(&target_nodes, &plex.token, &plex.client_identifier, &movie_path, "movie").await;
                });
            }
        }
    }

    let existing_grab = state.db.find_arr_grab_by_hash_or_name(download_id.as_deref().unwrap_or(""), release_title).ok().flatten()
        .or_else(|| {
            if let Some(src) = movie_source_path {
                state.db.find_arr_grab_by_hash_or_name("", src).ok().flatten()
            } else {
                None
            }
        })
        .or_else(|| state.db.find_arr_grab_by_ids_or_title(movie_id, None, imdb_id.as_deref(), tmdb_id, None, Some(movie_title)).ok().flatten());

    let grab_id = existing_grab.as_ref().map(|g| g.id.clone())
        .or(download_id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = Utc::now();

    let existing_ombi = state.db.find_ombi_request_by_title_or_ids(movie_title, imdb_id.as_deref(), tmdb_id, None).ok().flatten();
    let existing_post_id = existing_grab.as_ref().and_then(|g| g.mattermost_post_id.clone())
        .or_else(|| existing_ombi.as_ref().and_then(|o| o.mattermost_post_id.clone()));

    let final_release_title = if release_title != movie_title && !release_title.is_empty() {
        release_title.to_string()
    } else if let Some(ref existing) = existing_grab {
        existing.release_title.clone()
    } else {
        release_title.to_string()
    };

    let final_scene_name = if let Some(ref existing) = existing_grab {
        if !existing.scene_name.is_empty() && existing.scene_name != movie_title {
            existing.scene_name.clone()
        } else {
            final_release_title.clone()
        }
    } else {
        final_release_title.clone()
    };

    let final_download_id = download_id.or_else(|| existing_grab.as_ref().and_then(|g| g.download_id.clone()));
    let final_quality = infer_quality_from_release(&final_release_title, raw_quality);
    let final_indexer = infer_indexer_source(raw_indexer, existing_grab.as_ref().and_then(|g| g.indexer.as_deref()), &final_release_title, &state.config.get().await.queue_routing);
    let final_size_bytes = raw_size_bytes.or_else(|| existing_grab.as_ref().and_then(|g| g.size_bytes));
    let created_time = existing_grab.as_ref().map(|g| g.created_at).unwrap_or(now);
    let final_download_client = existing_grab.as_ref()
        .and_then(|g| g.download_client.clone())
        .filter(|dc| !dc.eq_ignore_ascii_case("Transmission") && !dc.is_empty())
        .or(download_client)
        .unwrap_or_else(|| "Transmission".to_string());

    let mut record = ArrGrabRecord {
        id: grab_id.clone(),
        scene_name: final_scene_name,
        release_title: final_release_title.clone(),
        event_type: event_type.to_string(),
        item_type: "movie".to_string(),
        series_id: None,
        movie_id,
        artist_id: None,
        album_id: None,
        zone_id: zone_id.clone(),
        season_number: None,
        episode_numbers: None,
        episode_ids: None,
        indexer: Some(final_indexer.clone()),
        download_client: Some(final_download_client.clone()),
        download_id: final_download_id,
        status: status.to_string(),
        re_searched: false,
        re_search_count: 0,
        title: Some(movie_title.to_string()),
        year,
        overview: overview.clone(),
        poster_url: poster_url.clone(),
        genres: genres.clone(),
        quality: Some(final_quality.clone()),
        size_bytes: final_size_bytes,
        imdb_id,
        tmdb_id,
        tvdb_id: None,
        runtime_mins,
        rating,
        mattermost_post_id: existing_post_id.clone(),
        payload_json: payload.to_string(),
        created_at: created_time,
        updated_at: now,
    };

    let _ = state.db.save_arr_grab(&record);
    let _ = state.db.log_event(
        "radarr",
        "info",
        &format!("Radarr [{}] for '{}'", event_type, release_title),
        Some(&payload.to_string()),
    );

    // Format Conduit-Themed Living Card & Timeline
    let (action_label, color_hex) = match event_type {
        "Grab" => ("Conduit Grabbed", "#10B981"),
        "Download" => ("Conduit Retrieved & Stored", "#3B82F6"),
        "MovieFileDelete" => {
            if delete_reason.eq_ignore_ascii_case("MissingFromDisk") {
                ("⚠️ File Missing From Disk", "#F59E0B")
            } else {
                ("Conduit Deleted Movie", "#EF4444")
            }
        },
        _ => ("Conduit Tracked Movie", "#F59E0B"),
    };

    let notif_title = format!("{} • {} ({})", action_label, movie_title, year.unwrap_or(0));
    let notif_overview = overview.as_deref().unwrap_or(release_title);

    let mut fields: Vec<(&str, &str, bool)> = Vec::new();
    fields.push(("Quality", &final_quality, true));
    fields.push(("Indexer", &final_indexer, true));

    let size_fmt = final_size_bytes.map(format_bytes_str).unwrap_or_else(|| "N/A".to_string());
    fields.push(("Size", &size_fmt, true));

    let rt_str = runtime_mins.map(|r| format!("{} mins", r));
    if let Some(ref r_fmt) = rt_str {
        fields.push(("Runtime", r_fmt.as_str(), true));
    }
    if let Some(ref g) = genres {
        fields.push(("Genres", g, true));
    }
    fields.push(("Fetcher Node", &final_download_client, true));

    let media_info_str = format_media_info_summary(payload.get("movieFile"));
    if let Some(ref mi) = media_info_str {
        fields.push(("Media Details", mi, true));
    }

    let flags_str = format_indexer_flags(&payload);
    if let Some(ref flg) = flags_str {
        fields.push(("Release Flags", flg, true));
    }

    let cf_str = format_custom_format_summary(&payload);
    if let Some(ref cf) = cf_str {
        fields.push(("Custom Formats", cf, true));
    }

    if event_type == "MovieFileDelete" && !delete_reason.eq_ignore_ascii_case("Upgrade") {
        fields.push(("Delete Reason", delete_reason, true));
    }

    // Structured Pipeline Lifecycle Timeline
    let timeline_str = if event_type == "Grab" {
        format!("📥 Grabbed: {}", now.format("%Y-%m-%d %H:%M:%S UTC"))
    } else if event_type == "Download" {
        format!(
            "📥 Grabbed: {}\n🎾 Staged: {}\n🏆 Imported: {}",
            created_time.format("%Y-%m-%d %H:%M:%S UTC"),
            now.format("%Y-%m-%d %H:%M:%S UTC"),
            now.format("%Y-%m-%d %H:%M:%S UTC")
        )
    } else {
        format!("🕒 Updated: {}", now.format("%Y-%m-%d %H:%M:%S UTC"))
    };
    fields.push(("Pipeline Timeline", &timeline_str, false));

    let is_actionable_event = matches!(event_type, "Grab" | "Download" | "MovieFileDelete");
    if is_actionable_event && movie_title != "Unknown Movie" {
        if event_type == "MovieFileDelete" && existing_post_id.is_some() && delete_reason.eq_ignore_ascii_case("Upgrade") {
            let root_id = existing_post_id.as_deref().unwrap();
            let thread_msg = format!("🐕 Conduit cleared previous movie file to make room for incoming upgrade/import: `{}`", release_title);
            let _ = NotificationManager::dispatch_rich_or_update(
                &state.config.get().await.notifications,
                "radarr.delete.detail",
                zone_id.as_deref(),
                None,
                Some(root_id),
                "Movie Upgrade / Cleanup",
                &thread_msg,
                None,
                Vec::new(),
                Some(color_hex),
            ).await;
        } else {
            let post_id_opt = NotificationManager::dispatch_rich_or_update(
                &state.config.get().await.notifications,
                &format!("radarr.{}", event_type.to_lowercase()),
                zone_id.as_deref(),
                existing_post_id.as_deref(),
                None,
                &notif_title,
                notif_overview,
                poster_url.as_deref(),
                fields,
                Some(color_hex),
            ).await;

            if let Some(pid) = post_id_opt {
                record.mattermost_post_id = Some(pid);
                let _ = state.db.save_arr_grab(&record);
            }
        }
    }

    Ok(Json(json!({"status": "ok", "event": event_type, "id": grab_id})))
}

#[utoipa::path(
    post,
    path = "/api/lidarr/inbound",
    tag = "Arr Tracking",
    summary = "Lidarr webhook receiver",
    description = "Ingests Lidarr event webhooks (Grab, Download, DownloadFailure, ImportFailure, Rename, ArtistAdd, ArtistDelete, AlbumDelete, Health, ApplicationUpdate). See docs/webhook_reference.md for the full per-event field reference. Configure `lidarr.webhook_secret` and check for an `X-Lidarr-Secret`/`X-Webhook-Secret` header if set — or, for a zone-specific instance, point Lidarr at this URL with `?zone=<id>` (see `zones`) and configure that zone's own `webhook_secret` instead.",
    params(InboundZoneParams),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Webhook processed successfully"),
        (status = 401, description = "Invalid or missing webhook secret (only enforced if lidarr.webhook_secret is configured)")
    )
)]
pub async fn lidarr_inbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(zone_q): Query<InboundZoneParams>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = state.config.get().await;
    let zone = resolve_zone(&config, zone_q.zone.as_deref());
    let (expected_secret, zone_id) = match zone {
        Some(z) => (z.webhook_secret.clone(), Some(z.id.clone())),
        None => (config.lidarr.webhook_secret.clone(), None),
    };
    if !verify_webhook_secret(&headers, expected_secret.as_deref()) {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"status": "error", "message": "Invalid Lidarr webhook secret"}))));
    }

    debug!("Lidarr inbound webhook payload (zone={:?}): {:?}", zone_id, payload);

    let event_type = payload.get("eventType").and_then(|v| v.as_str()).unwrap_or("Unknown");
    if event_type.eq_ignore_ascii_case("Test") {
        let _ = state.db.log_event("lidarr", "info", "Lidarr test webhook connection received & verified", None);
        return Ok(Json(json!({"status": "ok", "message": "Lidarr test connection verified"})));
    }

    if event_type.eq_ignore_ascii_case("Health") || event_type.eq_ignore_ascii_case("HealthRestored") {
        return handle_arr_health_event(&state, "lidarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ApplicationUpdate") {
        return handle_arr_app_update(&state, "lidarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ArtistAdd") {
        return handle_lidarr_artist_added(&state, &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ArtistDelete") || event_type.eq_ignore_ascii_case("AlbumDelete") {
        return handle_lidarr_deleted(&state, event_type, &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("DownloadFailure") || event_type.eq_ignore_ascii_case("ImportFailure") {
        return handle_lidarr_download_failure(&state, event_type, &payload, zone_id.as_deref()).await;
    }

    // Auto-detect media type and forward once without loop
    let detected_type = detect_inbound_media_type(&payload, &config.queue_routing);
    if detected_type == "tv" {
        debug!("Lidarr inbound webhook detected TV series payload/tracker -> routing to tv handler");
        return Box::pin(process_sonarr_inbound_direct(state, payload, zone_id)).await;
    } else if detected_type == "movie" {
        debug!("Lidarr inbound webhook detected movie payload/tracker -> routing to movie handler");
        return Box::pin(process_radarr_inbound_direct(state, payload, zone_id)).await;
    }

    process_lidarr_inbound_direct(state, payload, zone_id).await
}

pub async fn process_lidarr_inbound_direct(
    state: AppState,
    payload: serde_json::Value,
    zone_id: Option<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let event_type = payload.get("eventType").and_then(|v| v.as_str()).unwrap_or("Unknown");

    if event_type.eq_ignore_ascii_case("Test") {
        let _ = state.db.log_event("lidarr", "info", "Lidarr test webhook connection received & verified", None);
        return Ok(Json(json!({"status": "ok", "message": "Lidarr test connection verified"})));
    }
    if event_type.eq_ignore_ascii_case("Health") || event_type.eq_ignore_ascii_case("HealthRestored") {
        return handle_arr_health_event(&state, "lidarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ApplicationUpdate") {
        return handle_arr_app_update(&state, "lidarr", &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ArtistAdd") {
        return handle_lidarr_artist_added(&state, &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("ArtistDelete") || event_type.eq_ignore_ascii_case("AlbumDelete") {
        return handle_lidarr_deleted(&state, event_type, &payload, zone_id.as_deref()).await;
    }
    if event_type.eq_ignore_ascii_case("DownloadFailure") || event_type.eq_ignore_ascii_case("ImportFailure") {
        return handle_lidarr_download_failure(&state, event_type, &payload, zone_id.as_deref()).await;
    }
    let artist = payload.get("artist");
    let artist_name = artist.and_then(|a| a.get("name")).and_then(|v| v.as_str()).unwrap_or("Unknown Artist");
    let artist_overview = artist.and_then(|a| a.get("overview")).and_then(|o| o.as_str()).map(|s| s.to_string());
    let artist_id = artist.and_then(|a| a.get("id")).and_then(|v| v.as_i64());

    let albums = payload.get("albums").and_then(|a| a.as_array());
    let first_album = albums.and_then(|arr| arr.first()).or_else(|| payload.get("album"));
    let album_id = first_album.and_then(|a| a.get("id")).and_then(|v| v.as_i64());
    let album_title = first_album
        .and_then(|a| a.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown Album");

    let album_year = first_album
        .and_then(|a| a.get("releaseDate"))
        .and_then(|d| d.as_str())
        .and_then(|s| s.chars().take(4).collect::<String>().parse::<i64>().ok())
        .or_else(|| payload.get("year").and_then(|y| y.as_i64()));

    let poster_url = first_album
        .and_then(|a| a.get("images"))
        .and_then(|i| i.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|img| {
                let cover_type = img.get("coverType").and_then(|c| c.as_str()).unwrap_or("");
                if cover_type == "cover" || cover_type == "poster" || cover_type == "disc" {
                    img.get("remoteUrl").and_then(|u| u.as_str()).or_else(|| img.get("url").and_then(|u| u.as_str())).map(|s| s.to_string())
                } else {
                    None
                }
            })
        })
        .or_else(|| {
            artist.and_then(|a| a.get("images")).and_then(|i| i.as_array()).and_then(|arr| {
                arr.iter().find_map(|img| {
                    img.get("remoteUrl").and_then(|u| u.as_str()).or_else(|| img.get("url").and_then(|u| u.as_str())).map(|s| s.to_string())
                })
            })
        });

    let genres_vec: Vec<String> = artist.and_then(|a| a.get("genres")).and_then(|g| g.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let genres = if genres_vec.is_empty() { None } else { Some(genres_vec.join(", ")) };

    // Lidarr's Download/ImportFailure payloads carry a plural `trackFiles[]` array (no `release`
    // object at all on Download); only `Grab`/`Retag` carry `release`/singular `trackFile`.
    let track_files = payload.get("trackFiles").and_then(|v| v.as_array());
    let first_track_file = track_files.and_then(|arr| arr.first()).or_else(|| payload.get("trackFile"));

    let release = payload.get("release");
    let release_title = release
        .and_then(|r| r.get("releaseTitle"))
        .and_then(|v| v.as_str())
        .or_else(|| first_track_file.and_then(|f| f.get("sceneName")).and_then(|v| v.as_str()))
        .unwrap_or_else(|| {
            if album_title != "Unknown Album" {
                album_title
            } else {
                artist_name
            }
        });

    let raw_indexer = release.and_then(|r| r.get("indexer")).and_then(|v| v.as_str());
    let download_client = payload.get("downloadClient").and_then(|v| v.as_str()).map(|s| s.to_string());
    let download_id = payload.get("downloadId").and_then(|v| v.as_str()).map(|s| s.to_string());

    let raw_quality = release.and_then(|r| r.get("quality"))
        .and_then(|q| if q.is_string() { q.as_str() } else { q.get("name").and_then(|n| n.as_str()) })
        .or_else(|| first_track_file.and_then(|f| f.get("quality")).and_then(|q| if q.is_string() { q.as_str() } else { q.get("name").and_then(|n| n.as_str()) }));

    let raw_size_bytes = release.and_then(|r| r.get("size")).and_then(|s| s.as_i64())
        .or_else(|| {
            track_files.map(|arr| arr.iter().filter_map(|f| f.get("size").and_then(|s| s.as_i64())).sum::<i64>()).filter(|&sum| sum > 0)
        })
        .or_else(|| first_track_file.and_then(|f| f.get("size")).and_then(|s| s.as_i64()));

    let status = match event_type {
        "Grab" => "fetched",
        // Lidarr's real wire eventType is "Download" (no "AlbumDownload"/"TrackFileDelete" —
        // those aren't in Lidarr's WebhookEventType enum; Lidarr has no per-file delete event).
        "Download" => "imported",
        _ => "tracked",
    };

    // Intelligent title & metadata parsing from release title fallback
    let (parsed_artist, parsed_album, parsed_year, parsed_quality) = parse_music_release_title(release_title);
    let final_artist = if artist_name != "Unknown Artist" { artist_name.to_string() } else { parsed_artist };
    let final_album = if album_title != "Unknown Album" { album_title.to_string() } else { parsed_album };
    let final_year = album_year.or(parsed_year);
    let final_quality = if let Some(q) = raw_quality {
        infer_quality_from_release(release_title, Some(q))
    } else if let Some(pq) = parsed_quality {
        pq
    } else {
        infer_quality_from_release(release_title, None)
    };

    let media_title = if !final_artist.is_empty() && !final_album.is_empty() && final_artist != final_album {
        format!("{} - {}", final_artist, final_album)
    } else if !final_album.is_empty() {
        final_album.clone()
    } else {
        release_title.to_string()
    };

    let track_source_path = first_track_file.and_then(|f| f.get("path")).or_else(|| first_track_file.and_then(|f| f.get("sourcePath"))).and_then(|v| v.as_str());
    let track_source_stem = track_source_path.and_then(|p| {
        let p_clean = p.trim();
        if p_clean.is_empty() { return None; }
        let filename = std::path::Path::new(p_clean).file_name().and_then(|n| n.to_str()).unwrap_or(p_clean);
        let mut stem = filename;
        for ext in &[".flac", ".mp3", ".m4a", ".aac", ".wav", ".alac"] {
            if let Some(s) = stem.strip_suffix(ext) {
                stem = s;
            }
        }
        if !stem.is_empty() { Some(stem.to_string()) } else { None }
    });

    let existing_grab = state.db.find_arr_grab_by_hash_or_name(download_id.as_deref().unwrap_or(""), release_title).ok().flatten()
        .or_else(|| {
            if let Some(ref stem) = track_source_stem {
                state.db.find_arr_grab_by_hash_or_name("", stem).ok().flatten()
            } else if let Some(src) = track_source_path {
                state.db.find_arr_grab_by_hash_or_name("", src).ok().flatten()
            } else {
                None
            }
        })
        .or_else(|| state.db.find_arr_grab_by_ids_or_title(None, None, None, None, None, Some(&media_title)).ok().flatten());

    let grab_id = existing_grab.as_ref().map(|g| g.id.clone())
        .or(download_id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = Utc::now();

    let existing_ombi = state.db.find_ombi_request_by_title_or_ids(&media_title, None, None, None).ok().flatten();
    let existing_post_id = existing_grab.as_ref().and_then(|g| g.mattermost_post_id.clone())
        .or_else(|| existing_ombi.as_ref().and_then(|o| o.mattermost_post_id.clone()));

    let final_release_title = if release_title != album_title && release_title != artist_name && !release_title.is_empty() {
        release_title.to_string()
    } else if let Some(ref existing) = existing_grab {
        existing.release_title.clone()
    } else {
        release_title.to_string()
    };

    let final_scene_name = if let Some(ref existing) = existing_grab {
        if !existing.scene_name.is_empty() && existing.scene_name != album_title && existing.scene_name != artist_name {
            existing.scene_name.clone()
        } else {
            final_release_title.clone()
        }
    } else {
        final_release_title.clone()
    };

    let final_download_id = download_id.or_else(|| existing_grab.as_ref().and_then(|g| g.download_id.clone()));
    let final_indexer = infer_indexer_source(raw_indexer, existing_grab.as_ref().and_then(|g| g.indexer.as_deref()), &final_release_title, &state.config.get().await.queue_routing);
    let final_size_bytes = raw_size_bytes.or_else(|| existing_grab.as_ref().and_then(|g| g.size_bytes));
    let created_time = existing_grab.as_ref().map(|g| g.created_at).unwrap_or(now);
    let final_download_client = existing_grab.as_ref()
        .and_then(|g| g.download_client.clone())
        .filter(|dc| !dc.eq_ignore_ascii_case("Transmission") && !dc.is_empty())
        .or(download_client)
        .unwrap_or_else(|| "Transmission".to_string());

    let mut record = ArrGrabRecord {
        id: grab_id.clone(),
        scene_name: final_scene_name,
        release_title: final_release_title.clone(),
        event_type: event_type.to_string(),
        item_type: "music".to_string(),
        series_id: None,
        movie_id: None,
        artist_id,
        album_id,
        zone_id: zone_id.clone(),
        season_number: None,
        episode_numbers: None,
        episode_ids: None,
        indexer: Some(final_indexer.clone()),
        download_client: Some(final_download_client.clone()),
        download_id: final_download_id,
        status: status.to_string(),
        re_searched: false,
        re_search_count: 0,
        title: Some(media_title.clone()),
        year: final_year,
        overview: artist_overview.clone(),
        poster_url: poster_url.clone(),
        genres: genres.clone(),
        quality: Some(final_quality.clone()),
        size_bytes: final_size_bytes,
        imdb_id: None,
        tmdb_id: None,
        tvdb_id: None,
        runtime_mins: None,
        rating: None,
        mattermost_post_id: existing_post_id.clone(),
        payload_json: payload.to_string(),
        created_at: created_time,
        updated_at: now,
    };

    let _ = state.db.save_arr_grab(&record);

    let raw_payload_str = serde_json::to_string(&payload).unwrap_or_default();
    let _ = state.db.log_event(
        "lidarr",
        "info",
        &format!("Lidarr Webhook: {} for '{}' (ID: {})", event_type, media_title, grab_id),
        Some(&raw_payload_str),
    );

    let (action_label, color_hex) = match event_type {
        "Grab" => ("Conduit Grabbed", "#E2B340"),
        "Download" => ("Conduit Retrieved & Stored", "#10B981"),
        "Rename" => ("Conduit Renamed Track", "#3B82F6"),
        _ => ("Conduit Music Activity", "#6B7280"),
    };

    let year_suffix = final_year.map(|y| format!(" ({})", y)).unwrap_or_default();
    let notif_title = format!("{} • {}{}", action_label, media_title, year_suffix);
    let notif_overview = artist_overview.as_deref().unwrap_or(release_title);

    let mut fields: Vec<(&str, &str, bool)> = Vec::new();
    fields.push(("Quality", &final_quality, true));
    fields.push(("Indexer", &final_indexer, true));

    let size_fmt = final_size_bytes.map(format_bytes_str).unwrap_or_else(|| "N/A".to_string());
    fields.push(("Size", &size_fmt, true));

    if !final_artist.is_empty() {
        fields.push(("Artist", &final_artist, true));
    }
    if !final_album.is_empty() {
        fields.push(("Album", &final_album, true));
    }

    if let Some(ref g) = genres {
        fields.push(("Genres", g, true));
    }
    fields.push(("Fetcher Node", &final_download_client, true));

    // Structured Pipeline Lifecycle Timeline
    let timeline_str = if event_type == "Grab" {
        format!("📥 Grabbed: {}", now.format("%Y-%m-%d %H:%M:%S UTC"))
    } else if event_type == "Download" {
        format!(
            "📥 Grabbed: {}\n🎾 Staged: {}\n🏆 Imported: {}",
            created_time.format("%Y-%m-%d %H:%M:%S UTC"),
            now.format("%Y-%m-%d %H:%M:%S UTC"),
            now.format("%Y-%m-%d %H:%M:%S UTC")
        )
    } else {
        format!("🕒 Updated: {}", now.format("%Y-%m-%d %H:%M:%S UTC"))
    };
    fields.push(("Pipeline Timeline", &timeline_str, false));

    let is_actionable_event = matches!(event_type, "Grab" | "Download" | "Rename");
    if is_actionable_event && !final_artist.is_empty() && final_artist != "Unknown Artist" {
        let post_id_opt = NotificationManager::dispatch_rich_or_update(
            &state.config.get().await.notifications,
            &format!("lidarr.{}", event_type.to_lowercase()),
            zone_id.as_deref(),
            existing_post_id.as_deref(),
            None,
            &notif_title,
            notif_overview,
            poster_url.as_deref(),
            fields,
            Some(color_hex),
        ).await;

        if let Some(pid) = post_id_opt {
            record.mattermost_post_id = Some(pid);
            let _ = state.db.save_arr_grab(&record);
        }
    }

    Ok(Json(json!({"status": "ok", "event": event_type, "id": grab_id})))
}

fn format_bytes_str(bytes: i64) -> String {
    if bytes <= 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let b = bytes as f64;
    let i = (b.log(1024.0).floor() as usize).min(units.len() - 1);
    format!("{:.2} {}", b / 1024f64.powi(i as i32), units[i])
}

#[utoipa::path(
    post,
    path = "/api/arr/test-connection",
    tag = "Arr Tracking",
    summary = "Test Sonarr / Radarr / Lidarr connection",
    description = "Pings a remote Sonarr, Radarr, or Lidarr instance to verify base URL and API key. Connectivity failures (wrong URL, wrong API key, unreachable host) are reported as `success: false` in the 200 response body, not as an HTTP error — a non-200 status means Conduit itself failed, not the remote Arr instance.",
    request_body = ArrTestConnectionRequest,
    responses(
        (status = 200, description = "Connection test response (check `success`)", body = ArrTestConnectionResponse),
        (status = 401, description = "Unauthorized — admin session required"),
        (status = 500, description = "Failed to build the outbound HTTP client")
    )
)]
pub async fn test_arr_connection(
    _admin: RequireAdmin,
    Json(req): Json<ArrTestConnectionRequest>,
) -> Result<Json<ArrTestConnectionResponse>, StatusCode> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let is_lidarr = req.app_type.eq_ignore_ascii_case("lidarr");
    let primary_url = if is_lidarr {
        format!("{}/api/v1/system/status", req.base_url.trim_end_matches('/'))
    } else {
        format!("{}/api/v3/system/status", req.base_url.trim_end_matches('/'))
    };

    let fallback_url = if is_lidarr {
        format!("{}/api/v3/system/status", req.base_url.trim_end_matches('/'))
    } else {
        format!("{}/api/v1/system/status", req.base_url.trim_end_matches('/'))
    };

    let mut res = client.get(&primary_url)
        .header("X-Api-Key", &req.api_key)
        .send()
        .await;

    if let Ok(ref r) = res {
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            // Try fallback API version
            if let Ok(fallback_res) = client.get(&fallback_url)
                .header("X-Api-Key", &req.api_key)
                .send()
                .await
            {
                res = Ok(fallback_res);
            }
        }
    }

    match res {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let version = data.get("version").and_then(|v| v.as_str()).map(|s| s.to_string());
            Ok(Json(ArrTestConnectionResponse {
                success: true,
                message: "Successfully connected to instance!".to_string(),
                app_version: version,
            }))
        }
        Ok(r) => {
            Ok(Json(ArrTestConnectionResponse {
                success: false,
                message: format!("Instance returned status HTTP {}", r.status()),
                app_version: None,
            }))
        }
        Err(e) => {
            Ok(Json(ArrTestConnectionResponse {
                success: false,
                message: format!("Failed to reach host: {}", e),
                app_version: None,
            }))
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/arr/sync-now",
    tag = "Arr Tracking",
    summary = "Trigger Master-to-Slave Sync Now",
    description = "Immediately executes a manual synchronization of libraries from master to slave Arr nodes.",
    request_body = ArrSyncNowRequest,
    responses(
        (status = 200, description = "Sync completed")
    )
)]
pub async fn trigger_arr_sync_now(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Json(req): Json<ArrSyncNowRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = state.config.get().await;

    let mut sync_logs = Vec::new();

    if req.app_type == "sonarr" || req.app_type == "all" {
        if let Some(ref primary) = config.sonarr.primary {
            sync_logs.push(format!("Initiating Sonarr sync from primary '{}' to {} replicas...", primary.name, config.sonarr.replicas.len()));
            let _ = state.db.log_event("arr_sync", "info", "Manual Sonarr primary-to-replica sync triggered", None);
            crate::engines::arr_sync::sync_sonarr_instances(&config).await;
        }
    }

    if req.app_type == "radarr" || req.app_type == "all" {
        if let Some(ref primary) = config.radarr.primary {
            sync_logs.push(format!("Initiating Radarr sync from primary '{}' to {} replicas...", primary.name, config.radarr.replicas.len()));
            let _ = state.db.log_event("arr_sync", "info", "Manual Radarr primary-to-replica sync triggered", None);
            crate::engines::arr_sync::sync_radarr_instances(&config).await;
        }
    }

    if req.app_type == "lidarr" || req.app_type == "all" {
        if let Some(ref primary) = config.lidarr.primary {
            sync_logs.push(format!("Initiating Lidarr sync from primary '{}' to {} replicas...", primary.name, config.lidarr.replicas.len()));
            let _ = state.db.log_event("arr_sync", "info", "Manual Lidarr primary-to-replica sync triggered", None);
            crate::engines::arr_sync::sync_lidarr_instances(&config).await;
        }
    }

    Ok(Json(json!({
        "status": "ok",
        "message": "Library synchronization executed",
        "logs": sync_logs
    })))
}

async fn fetch_sonarr_live_stats(primary: &crate::config::ArrNodeConfig) -> Result<SonarrLiveStats, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let base = primary.base_url.trim_end_matches('/');

    // 1. System status — this call's success/failure is also what health_monitor.rs relies on
    // to detect Sonarr going down/recovering, via the sonarr_error field on ArrStatsResponse
    // (no separate status ping needed there — see engines/arr_stats_poller.rs).
    let status_res = client.get(format!("{}/api/v3/system/status", base))
        .header("X-Api-Key", &primary.api_key)
        .send().await.map_err(|e| e.to_string())?;
    if !status_res.status().is_success() {
        return Err(format!("HTTP {}", status_res.status()));
    }
    let status_json = status_res.json::<serde_json::Value>().await.ok();
    let version = status_json.and_then(|v| v.get("version").and_then(|s| s.as_str()).map(|s| s.to_string()));

    // 2. Series list (gracefully resilient to timeout on large libraries)
    let series_res: Vec<serde_json::Value> = match client.get(format!("{}/api/v3/series", base))
        .header("X-Api-Key", &primary.api_key)
        .send().await
    {
        Ok(resp) => resp.json::<Vec<serde_json::Value>>().await.unwrap_or_default(),
        Err(e) => {
            debug!("Sonarr series fetch timed out or failed (using empty fallback): {}", e);
            Vec::new()
        }
    };

    let series_count = series_res.len();
    let mut monitored_count = 0;
    let mut total_episodes = 0;
    let mut total_episode_files = 0;
    let mut size_on_disk = 0i64;

    for s in &series_res {
        if s.get("monitored").and_then(|m| m.as_bool()).unwrap_or(false) {
            monitored_count += 1;
        }
        if let Some(stats) = s.get("statistics") {
            total_episodes += stats.get("totalEpisodeCount").and_then(|c| c.as_i64()).unwrap_or(0) as usize;
            total_episode_files += stats.get("episodeFileCount").and_then(|c| c.as_i64()).unwrap_or(0) as usize;
            size_on_disk += stats.get("sizeOnDisk").and_then(|c| c.as_i64()).unwrap_or(0);
        }
    }

    // 3. Queue count
    let queue_res = match client.get(format!("{}/api/v3/queue", base))
        .header("X-Api-Key", &primary.api_key)
        .query(&[("pageSize", "1")])
        .send().await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };
    let queue_count = queue_res.as_ref()
        .and_then(|q| q.get("totalRecords").and_then(|r| r.as_u64()))
        .unwrap_or(0) as usize;

    // 4. Missing wanted count
    let wanted_res = match client.get(format!("{}/api/v3/wanted/missing", base))
        .header("X-Api-Key", &primary.api_key)
        .query(&[("pageSize", "1")])
        .send().await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };
    let missing_count = wanted_res.as_ref()
        .and_then(|w| w.get("totalRecords").and_then(|r| r.as_u64()))
        .unwrap_or(0) as usize;

    // 5. Health checks
    let health_res: Vec<serde_json::Value> = match client.get(format!("{}/api/v3/health", base))
        .header("X-Api-Key", &primary.api_key)
        .send().await
    {
        Ok(resp) => resp.json::<Vec<serde_json::Value>>().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let health_issues: Vec<String> = health_res.into_iter().filter_map(|h| {
        h.get("message").and_then(|m| m.as_str()).map(|s| s.to_string())
    }).collect();

    // 6. Diskspace
    let disk_res: Vec<serde_json::Value> = match client.get(format!("{}/api/v3/diskspace", base))
        .header("X-Api-Key", &primary.api_key)
        .send().await
    {
        Ok(resp) => resp.json::<Vec<serde_json::Value>>().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let mut disk_free = 0i64;
    let mut disk_total = 0i64;
    let mut has_disk = false;
    for d in disk_res {
        if let (Some(free), Some(total)) = (d.get("freeSpace").and_then(|v| v.as_i64()), d.get("totalSpace").and_then(|v| v.as_i64())) {
            disk_free += free;
            disk_total += total;
            has_disk = true;
        }
    }

    Ok(SonarrLiveStats {
        version,
        series_count,
        monitored_series_count: monitored_count,
        episode_count: total_episodes,
        episode_file_count: total_episode_files,
        missing_episodes_count: missing_count,
        queue_count,
        size_on_disk_bytes: size_on_disk,
        health_issues,
        disk_free_bytes: if has_disk { Some(disk_free) } else { None },
        disk_total_bytes: if has_disk { Some(disk_total) } else { None },
    })
}

async fn fetch_radarr_live_stats(primary: &crate::config::ArrNodeConfig) -> Result<RadarrLiveStats, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let base = primary.base_url.trim_end_matches('/');

    // 1. System status — health_monitor.rs relies on this call's success/failure via
    // radarr_error on ArrStatsResponse, no separate status ping needed there.
    let status_res = client.get(format!("{}/api/v3/system/status", base))
        .header("X-Api-Key", &primary.api_key)
        .send().await.map_err(|e| e.to_string())?;
    if !status_res.status().is_success() {
        return Err(format!("HTTP {}", status_res.status()));
    }
    let status_json = status_res.json::<serde_json::Value>().await.ok();
    let version = status_json.and_then(|v| v.get("version").and_then(|s| s.as_str()).map(|s| s.to_string()));

    // 2. Movie list (gracefully resilient to timeout on large movie libraries)
    let movie_res: Vec<serde_json::Value> = match client.get(format!("{}/api/v3/movie", base))
        .header("X-Api-Key", &primary.api_key)
        .send().await
    {
        Ok(resp) => resp.json::<Vec<serde_json::Value>>().await.unwrap_or_default(),
        Err(e) => {
            debug!("Radarr movie fetch timed out or failed (using empty fallback): {}", e);
            Vec::new()
        }
    };

    let movie_count = movie_res.len();
    let mut monitored_count = 0;
    let mut movie_file_count = 0;
    let mut size_on_disk = 0i64;

    for m in &movie_res {
        if m.get("monitored").and_then(|v| v.as_bool()).unwrap_or(false) {
            monitored_count += 1;
        }
        if m.get("hasFile").and_then(|v| v.as_bool()).unwrap_or(false) {
            movie_file_count += 1;
        }
        size_on_disk += m.get("sizeOnDisk").and_then(|v| v.as_i64()).unwrap_or(0);
    }

    // 3. Queue count
    let queue_res = match client.get(format!("{}/api/v3/queue", base))
        .header("X-Api-Key", &primary.api_key)
        .query(&[("pageSize", "1")])
        .send().await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };
    let queue_count = queue_res.as_ref()
        .and_then(|q| q.get("totalRecords").and_then(|r| r.as_u64()))
        .unwrap_or(0) as usize;

    // 4. Missing wanted count
    let wanted_res = match client.get(format!("{}/api/v3/wanted/missing", base))
        .header("X-Api-Key", &primary.api_key)
        .query(&[("pageSize", "1")])
        .send().await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };
    let missing_count = wanted_res.as_ref()
        .and_then(|w| w.get("totalRecords").and_then(|r| r.as_u64()))
        .unwrap_or(0) as usize;

    // 5. Health checks
    let health_res: Vec<serde_json::Value> = match client.get(format!("{}/api/v3/health", base))
        .header("X-Api-Key", &primary.api_key)
        .send().await
    {
        Ok(resp) => resp.json::<Vec<serde_json::Value>>().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let health_issues: Vec<String> = health_res.into_iter().filter_map(|h| {
        h.get("message").and_then(|m| m.as_str()).map(|s| s.to_string())
    }).collect();

    // 6. Diskspace
    let disk_res: Vec<serde_json::Value> = match client.get(format!("{}/api/v3/diskspace", base))
        .header("X-Api-Key", &primary.api_key)
        .send().await
    {
        Ok(resp) => resp.json::<Vec<serde_json::Value>>().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let mut disk_free = 0i64;
    let mut disk_total = 0i64;
    let mut has_disk = false;
    for d in disk_res {
        if let (Some(free), Some(total)) = (d.get("freeSpace").and_then(|v| v.as_i64()), d.get("totalSpace").and_then(|v| v.as_i64())) {
            disk_free += free;
            disk_total += total;
            has_disk = true;
        }
    }

    Ok(RadarrLiveStats {
        version,
        movie_count,
        monitored_movie_count: monitored_count,
        movie_file_count,
        missing_movies_count: missing_count,
        queue_count,
        size_on_disk_bytes: size_on_disk,
        health_issues,
        disk_free_bytes: if has_disk { Some(disk_free) } else { None },
        disk_total_bytes: if has_disk { Some(disk_total) } else { None },
    })
}

async fn fetch_lidarr_live_stats(primary: &crate::config::ArrNodeConfig) -> Result<LidarrLiveStats, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let base = primary.base_url.trim_end_matches('/');
    let api_ver = if primary.version == 3 { 1 } else { primary.version };

    // 1. System status — health_monitor.rs relies on this call's success/failure via
    // lidarr_error on ArrStatsResponse, no separate status ping needed there.
    let status_res = client.get(format!("{}/api/v{}/system/status", base, api_ver))
        .header("X-Api-Key", &primary.api_key)
        .send().await.map_err(|e| e.to_string())?;
    if !status_res.status().is_success() {
        return Err(format!("HTTP {}", status_res.status()));
    }
    let status_json = status_res.json::<serde_json::Value>().await.ok();
    let version = status_json.and_then(|v| v.get("version").and_then(|s| s.as_str()).map(|s| s.to_string()));

    // 2. Artist list (gracefully resilient to timeout on large libraries)
    let artist_res: Vec<serde_json::Value> = match client.get(format!("{}/api/v{}/artist", base, api_ver))
        .header("X-Api-Key", &primary.api_key)
        .send().await
    {
        Ok(resp) => resp.json::<Vec<serde_json::Value>>().await.unwrap_or_default(),
        Err(e) => {
            debug!("Lidarr artist fetch timed out or failed (using empty fallback): {}", e);
            Vec::new()
        }
    };

    let artist_count = artist_res.len();
    let mut monitored_count = 0;
    let mut album_count = 0;
    let mut track_file_count = 0;
    let mut size_on_disk = 0i64;

    for a in &artist_res {
        if a.get("monitored").and_then(|v| v.as_bool()).unwrap_or(false) {
            monitored_count += 1;
        }
        if let Some(stats) = a.get("statistics") {
            album_count += stats.get("albumCount").and_then(|c| c.as_i64()).unwrap_or(0) as usize;
            track_file_count += stats.get("trackFileCount").and_then(|c| c.as_i64()).unwrap_or(0) as usize;
            size_on_disk += stats.get("sizeOnDisk").and_then(|c| c.as_i64()).unwrap_or(0);
        }
    }

    // 3. Queue count
    let queue_res = match client.get(format!("{}/api/v{}/queue", base, api_ver))
        .header("X-Api-Key", &primary.api_key)
        .query(&[("pageSize", "1")])
        .send().await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };
    let queue_count = queue_res.as_ref()
        .and_then(|q| q.get("totalRecords").and_then(|r| r.as_u64()))
        .unwrap_or(0) as usize;

    // 4. Missing wanted count
    let wanted_res = match client.get(format!("{}/api/v{}/wanted/missing", base, api_ver))
        .header("X-Api-Key", &primary.api_key)
        .query(&[("pageSize", "1")])
        .send().await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };
    let missing_count = wanted_res.as_ref()
        .and_then(|w| w.get("totalRecords").and_then(|r| r.as_u64()))
        .unwrap_or(0) as usize;

    // 5. Health checks
    let health_res: Vec<serde_json::Value> = match client.get(format!("{}/api/v{}/health", base, api_ver))
        .header("X-Api-Key", &primary.api_key)
        .send().await
    {
        Ok(resp) => resp.json::<Vec<serde_json::Value>>().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let health_issues: Vec<String> = health_res.into_iter().filter_map(|h| {
        h.get("message").and_then(|m| m.as_str()).map(|s| s.to_string())
    }).collect();

    // 6. Diskspace
    let disk_res: Vec<serde_json::Value> = match client.get(format!("{}/api/v{}/diskspace", base, api_ver))
        .header("X-Api-Key", &primary.api_key)
        .send().await
    {
        Ok(resp) => resp.json::<Vec<serde_json::Value>>().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let mut disk_free = 0i64;
    let mut disk_total = 0i64;
    let mut has_disk = false;
    for d in disk_res {
        if let (Some(free), Some(total)) = (d.get("freeSpace").and_then(|v| v.as_i64()), d.get("totalSpace").and_then(|v| v.as_i64())) {
            disk_free += free;
            disk_total += total;
            has_disk = true;
        }
    }

    Ok(LidarrLiveStats {
        version,
        artist_count,
        monitored_artist_count: monitored_count,
        album_count,
        track_file_count,
        missing_tracks_count: missing_count,
        queue_count,
        size_on_disk_bytes: size_on_disk,
        health_issues,
        disk_free_bytes: if has_disk { Some(disk_free) } else { None },
        disk_total_bytes: if has_disk { Some(disk_total) } else { None },
    })
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ArrStatsQueryParams {
    /// Return this zone's own Sonarr/Radarr/Lidarr stats instead of the legacy global/primary
    /// instance's. Omit, or pass "all", for the global (pre-zones) behavior.
    pub zone: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/arr/stats",
    tag = "Arr Tracking",
    summary = "Get Arr Ecosystem Statistics & Live Telemetry",
    description = "Returns aggregate stats on Sonarr/Radarr/Lidarr grabs, imports, library counts, queue lengths, and storage telemetry. Served from a background-refreshed cache (see the `arr_stats` WebSocket topic on `GET /api/ws` for push updates to the same, global-only data) so this responds instantly rather than polling Sonarr/Radarr/Lidarr live on every request. Pass `?zone=<id>` to get one multi-tenant zone's own stats instead of the legacy global instance's.",
    params(ArrStatsQueryParams),
    responses(
        (status = 200, description = "Arr ecosystem statistics", body = ArrStatsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown zone id")
    )
)]
pub async fn get_arr_stats(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Query(params): Query<ArrStatsQueryParams>,
) -> Result<Json<ArrStatsResponse>, StatusCode> {
    // Served from the background-refreshed cache (see engines::arr_stats_poller) so this
    // endpoint responds instantly instead of making ~15 sequential live HTTP calls to
    // Sonarr/Radarr/Lidarr on every request. Only falls back to a live (slow) computation
    // on a cold cache — e.g. immediately after startup, before the poller's first tick.
    if let Some(zone_id) = params.zone.as_deref().filter(|z| !z.is_empty() && *z != "all") {
        if let Some(cached) = state.arr_stats_cache.read().zones.get(zone_id).cloned() {
            return Ok(Json(cached));
        }
        let config = state.config.get().await;
        return match config.zones.iter().find(|z| z.id == zone_id) {
            Some(zone) => Ok(Json(compute_zone_arr_stats(&state.db, zone).await)),
            None => Err(StatusCode::NOT_FOUND),
        };
    }

    if let Some(cached) = state.arr_stats_cache.read().global.clone() {
        return Ok(Json(cached));
    }

    Ok(Json(compute_arr_stats(&state.config, &state.db).await))
}

pub async fn compute_arr_stats(config_mgr: &crate::config::ConfigManager, db: &crate::db::Database) -> ArrStatsResponse {
    let grabs = db.list_arr_grabs(500).unwrap_or_default();
    let config = config_mgr.get().await;
    compute_arr_stats_for(
        grabs,
        config.sonarr.enabled,
        config.sonarr.primary.as_ref(),
        config.radarr.enabled,
        config.radarr.primary.as_ref(),
        config.lidarr.enabled,
        config.lidarr.primary.as_ref(),
    ).await
}

/// Same computation as `compute_arr_stats`, scoped to one multi-tenant zone's own Sonarr/
/// Radarr/Lidarr instance instead of the legacy global `primary` instance, and counting only
/// grabs tagged with this zone's `zone_id` (same 500-record cap as the global path).
pub async fn compute_zone_arr_stats(db: &crate::db::Database, zone: &crate::config::ZoneConfig) -> ArrStatsResponse {
    let grabs = db.list_pipeline_items(500, None, None, None, Some(&zone.id)).unwrap_or_default();
    compute_arr_stats_for(
        grabs,
        zone.sonarr.is_some(),
        zone.sonarr.as_ref(),
        zone.radarr.is_some(),
        zone.radarr.as_ref(),
        zone.lidarr.is_some(),
        zone.lidarr.as_ref(),
    ).await
}

#[allow(clippy::too_many_arguments)]
async fn compute_arr_stats_for(
    grabs: Vec<crate::db::ArrGrabRecord>,
    sonarr_enabled: bool,
    sonarr_node: Option<&crate::config::ArrNodeConfig>,
    radarr_enabled: bool,
    radarr_node: Option<&crate::config::ArrNodeConfig>,
    lidarr_enabled: bool,
    lidarr_node: Option<&crate::config::ArrNodeConfig>,
) -> ArrStatsResponse {
    let fetched_count = grabs.iter().filter(|g| g.status == "fetched").count();
    let imported_count = grabs.iter().filter(|g| g.status == "imported").count();
    let replaced_count = grabs.iter().filter(|g| g.status == "replaced" || g.re_searched).count();
    let sonarr_count = grabs.iter().filter(|g| g.item_type == "series").count();
    let radarr_count = grabs.iter().filter(|g| g.item_type == "movie").count();
    let lidarr_count = grabs.iter().filter(|g| g.item_type == "music").count();

    let sonarr_primary_online = sonarr_node.is_some() && sonarr_enabled;
    let radarr_primary_online = radarr_node.is_some() && radarr_enabled;
    let lidarr_primary_online = lidarr_node.is_some() && lidarr_enabled;

    // Concurrently fetch live telemetry if enabled and configured. Errors are kept (not
    // discarded) so engines::health_monitor can detect down/recovered state from this same
    // cached result instead of re-polling system/status itself.
    let (sonarr_result, radarr_result, lidarr_result) = tokio::join!(
        async {
            if sonarr_enabled {
                if let Some(node) = sonarr_node {
                    return Some(fetch_sonarr_live_stats(node).await);
                }
            }
            None
        },
        async {
            if radarr_enabled {
                if let Some(node) = radarr_node {
                    return Some(fetch_radarr_live_stats(node).await);
                }
            }
            None
        },
        async {
            if lidarr_enabled {
                if let Some(node) = lidarr_node {
                    return Some(fetch_lidarr_live_stats(node).await);
                }
            }
            None
        }
    );

    let (sonarr_live, sonarr_error) = match sonarr_result {
        Some(Ok(stats)) => (Some(stats), None),
        Some(Err(e)) => (None, Some(e)),
        None => (None, None),
    };
    let (radarr_live, radarr_error) = match radarr_result {
        Some(Ok(stats)) => (Some(stats), None),
        Some(Err(e)) => (None, Some(e)),
        None => (None, None),
    };
    let (lidarr_live, lidarr_error) = match lidarr_result {
        Some(Ok(stats)) => (Some(stats), None),
        Some(Err(e)) => (None, Some(e)),
        None => (None, None),
    };

    ArrStatsResponse {
        total_grabs: grabs.len(),
        fetched_count,
        imported_count,
        replaced_count,
        sonarr_series_count: sonarr_count,
        radarr_movie_count: radarr_count,
        lidarr_artist_count: lidarr_count,
        sonarr_master_online: sonarr_primary_online,
        radarr_master_online: radarr_primary_online,
        lidarr_master_online: lidarr_primary_online,
        sonarr_live,
        radarr_live,
        lidarr_live,
        sonarr_error,
        radarr_error,
        lidarr_error,
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ArrGrabsQueryParams {
    pub q: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
    /// Filter to grabs tagged with this zone id (see `ZoneConfig`). Omit for all zones.
    pub zone: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/arr/grabs",
    tag = "Arr Tracking",
    summary = "List & search historical Arr grabs (Ghost Fetcher Archive)",
    description = "Searches and lists past Arr release grabs across Sonarr/Radarr, including active, imported, replaced, and purged downloads.",
    params(ArrGrabsQueryParams),
    responses(
        (status = 200, description = "List of historical arr grabs", body = [ArrGrabRecord]),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn list_grabs(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Query(params): Query<ArrGrabsQueryParams>,
) -> Result<Json<Vec<crate::db::ArrGrabRecord>>, StatusCode> {
    let limit = params.limit.unwrap_or(200).min(1000);
    let grabs = state.db.search_arr_grabs(params.q.as_deref(), params.status.as_deref(), params.zone.as_deref(), limit)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(grabs))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct PipelineQueryParams {
    pub app: Option<String>,
    pub status: Option<String>,
    pub q: Option<String>,
    pub limit: Option<usize>,
    /// Filter to a single configured zone's `id` (see `ZoneConfig`) — omit for the combined
    /// view across all zones (and pre-zones/legacy grabs with no zone at all).
    pub zone: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/arr/pipeline",
    tag = "Arr Tracking",
    summary = "List pipeline media items (Conduit's Scent Trail)",
    description = "Returns media items moving through the intake pipeline with full rich metadata.",
    params(PipelineQueryParams),
    responses(
        (status = 200, description = "List of pipeline items", body = [ArrGrabRecord]),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn list_pipeline(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Query(params): Query<PipelineQueryParams>,
) -> Result<Json<Vec<crate::db::ArrGrabRecord>>, StatusCode> {
    let limit = params.limit.unwrap_or(100).min(1000);
    let items = state.db.list_pipeline_items(
        limit,
        params.app.as_deref(),
        params.status.as_deref(),
        params.q.as_deref(),
        params.zone.as_deref(),
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(items))
}

#[utoipa::path(
    delete,
    path = "/api/arr/pipeline/{id}",
    tag = "Arr Tracking",
    summary = "Delete pipeline item",
    description = "Removes a pipeline media record from Conduit's tracker cache.",
    params(
        ("id" = String, Path, description = "Pipeline Grab ID")
    ),
    responses(
        (status = 200, description = "Pipeline item deleted"),
        (status = 404, description = "Item not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn delete_pipeline_item(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let deleted = state.db.delete_arr_grab(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(Json(json!({"status": "ok", "deleted": true, "id": id})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[utoipa::path(
    get,
    path = "/api/arr/pipeline/{id}/history",
    tag = "Arr Tracking",
    summary = "Get the full grab/import/delete/re-search timeline for one media item",
    description = "Returns every event ever recorded against this canonical grab id, oldest first — grabbed, imported, deleted, re-searched, re-imported, etc. Unlike the grab record itself (which collapses to current state via canonical entity reconciliation so the Pipeline/Fetchers/Archive views show one card per item), this is a genuine append-only timeline.",
    params(
        ("id" = String, Path, description = "Pipeline Grab ID")
    ),
    responses(
        (status = 200, description = "Timeline entries, oldest first", body = [crate::db::ArrGrabHistoryEntry]),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn get_grab_history(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<crate::db::ArrGrabHistoryEntry>>, StatusCode> {
    let history = state.db.list_grab_history(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(history))
}

#[utoipa::path(
    post,
    path = "/api/arr/pipeline/{id}/re-search",
    tag = "Arr Tracking",
    summary = "Trigger automated re-search in Sonarr/Radarr",
    description = "Instructs Sonarr or Radarr to trigger a fresh automatic release search for the given media item.",
    params(
        ("id" = String, Path, description = "Pipeline Grab ID")
    ),
    responses(
        (status = 200, description = "Re-search initiated"),
        (status = 404, description = "Grab record not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn re_search_pipeline_item(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let grab = state.db.get_arr_grab_by_id(&id)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Database error"}))))?
        .ok_or((StatusCode::NOT_FOUND, Json(json!({"error": "Pipeline item not found"}))))?;

    let config = state.config.get().await;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let mut executed = false;
    let mut details = String::new();

    if grab.item_type == "movie" {
        if let (Some(primary), Some(movie_id)) = (config.radarr.primary.as_ref(), grab.movie_id) {
            let cmd_url = format!("{}/api/v3/command", primary.base_url.trim_end_matches('/'));
            let cmd_body = json!({
                "name": "MoviesSearch",
                "movieIds": [movie_id]
            });
            match client.post(&cmd_url)
                .header("X-Api-Key", &primary.api_key)
                .json(&cmd_body)
                .send()
                .await
            {
                Ok(res) if res.status().is_success() => {
                    executed = true;
                    details = format!("Dispatched MoviesSearch to Radarr ({})", primary.name);
                }
                Ok(res) => {
                    details = format!("Radarr returned HTTP {}", res.status());
                }
                Err(e) => {
                    details = format!("Failed to contact Radarr: {}", e);
                }
            }
        }
    } else if grab.item_type == "series" {
        if let (Some(primary), Some(series_id)) = (config.sonarr.primary.as_ref(), grab.series_id) {
            let cmd_url = format!("{}/api/v3/command", primary.base_url.trim_end_matches('/'));
            let cmd_body = json!({
                "name": "SeriesSearch",
                "seriesId": series_id
            });
            match client.post(&cmd_url)
                .header("X-Api-Key", &primary.api_key)
                .json(&cmd_body)
                .send()
                .await
            {
                Ok(res) if res.status().is_success() => {
                    executed = true;
                    details = format!("Dispatched SeriesSearch to Sonarr ({})", primary.name);
                }
                Ok(res) => {
                    details = format!("Sonarr returned HTTP {}", res.status());
                }
                Err(e) => {
                    details = format!("Failed to contact Sonarr: {}", e);
                }
            }
        }
    } else if grab.item_type == "music" {
        if let Some(ref primary) = config.lidarr.primary {
            // Prefer a targeted AlbumSearchCommand when we know the album; otherwise fall back
            // to ArtistSearchCommand (searches the whole discography). Neither is possible
            // without at least one real Lidarr-side ID — sending an empty `albumIds: []` (the
            // previous behavior here) is silently accepted by Lidarr and searches nothing.
            let cmd_body = if let Some(album_id) = grab.album_id {
                Some(json!({ "name": "AlbumSearch", "albumIds": [album_id] }))
            } else {
                grab.artist_id.map(|artist_id| json!({ "name": "ArtistSearch", "artistId": artist_id }))
            };

            match cmd_body {
                Some(cmd_body) => {
                    let cmd_url = format!("{}/api/v1/command", primary.base_url.trim_end_matches('/'));
                    match client.post(&cmd_url)
                        .header("X-Api-Key", &primary.api_key)
                        .json(&cmd_body)
                        .send()
                        .await
                    {
                        Ok(res) if res.status().is_success() => {
                            executed = true;
                            let cmd_name = cmd_body.get("name").and_then(|n| n.as_str()).unwrap_or("Search");
                            details = format!("Dispatched {} to Lidarr ({})", cmd_name, primary.name);
                        }
                        Ok(res) => {
                            details = format!("Lidarr returned HTTP {}", res.status());
                        }
                        Err(e) => {
                            details = format!("Failed to contact Lidarr: {}", e);
                        }
                    }
                }
                None => {
                    details = "No artist/album ID recorded for this grab yet — re-search isn't possible until a fresh Lidarr webhook carries one".to_string();
                }
            }
        }
    }

    if executed {
        let _ = state.db.mark_grab_status(&id, "re-searched", true);
    }
    let _ = state.db.log_event(
        "pipeline_research",
        if executed { "info" } else { "warning" },
        &format!("Triggered re-search for '{}' ({})", grab.release_title, details),
        None,
    );

    Ok(Json(json!({
        "status": if executed { "ok" } else { "error" },
        "executed": executed,
        "details": details,
        "id": id,
    })))
}

#[utoipa::path(
    delete,
    path = "/api/arr/pipeline",
    tag = "Arr Tracking",
    summary = "Purge pipeline grab archive",
    description = "Purges historical Arr grabs from the database, optionally filtered by application.",
    params(PipelineQueryParams),
    responses(
        (status = 200, description = "Pipeline archive purged"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn purge_pipeline(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Query(params): Query<PipelineQueryParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let purged = state.db.purge_arr_grabs(params.app.as_deref()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = state.db.log_event("pipeline", "info", &format!("Purged {} historical pipeline records", purged), None);
    Ok(Json(json!({"status": "ok", "purged": purged})))
}

#[derive(Debug, Deserialize)]
pub struct SearchReleasesQuery {
    pub item_type: String, // "movie", "series", "music"
    pub movie_id: Option<i64>,
    pub series_id: Option<i64>,
    pub episode_id: Option<i64>,
    pub album_id: Option<i64>,
    pub zone_id: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/arr/search/releases",
    tag = "Arr Tracking",
    summary = "Search indexer releases via Sonarr/Radarr/Lidarr",
    description = "Queries the connected Arr instance for available indexer releases for the given media item.",
    params(
        ("item_type" = String, Query, description = "Media type (movie, series, music)"),
        ("movie_id" = Option<i64>, Query, description = "Radarr Movie ID"),
        ("series_id" = Option<i64>, Query, description = "Sonarr Series ID"),
        ("episode_id" = Option<i64>, Query, description = "Sonarr Episode ID"),
        ("album_id" = Option<i64>, Query, description = "Lidarr Album ID"),
        ("zone_id" = Option<String>, Query, description = "Zone ID")
    ),
    responses(
        (status = 200, description = "List of candidate releases"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn search_releases(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Query(params): Query<SearchReleasesQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = state.config.get().await;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let (base_url, api_key) = match params.item_type.as_str() {
        "movie" => {
            let node = if let Some(ref zid) = params.zone_id {
                config.zones.iter().find(|z| &z.id == zid).and_then(|z| z.radarr.as_ref()).or(config.radarr.primary.as_ref())
            } else {
                config.radarr.primary.as_ref()
            };
            let node = node.ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Radarr is not configured"}))))?;
            (format!("{}/api/v3/release", node.base_url.trim_end_matches('/')), node.api_key.clone())
        }
        "series" => {
            let node = if let Some(ref zid) = params.zone_id {
                config.zones.iter().find(|z| &z.id == zid).and_then(|z| z.sonarr.as_ref()).or(config.sonarr.primary.as_ref())
            } else {
                config.sonarr.primary.as_ref()
            };
            let node = node.ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Sonarr is not configured"}))))?;
            (format!("{}/api/v3/release", node.base_url.trim_end_matches('/')), node.api_key.clone())
        }
        "music" => {
            let node = if let Some(ref zid) = params.zone_id {
                config.zones.iter().find(|z| &z.id == zid).and_then(|z| z.lidarr.as_ref()).or(config.lidarr.primary.as_ref())
            } else {
                config.lidarr.primary.as_ref()
            };
            let node = node.ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Lidarr is not configured"}))))?;
            let ver = if node.version == 3 { 1 } else { node.version };
            (format!("{}/api/v{}/release", node.base_url.trim_end_matches('/'), ver), node.api_key.clone())
        }
        other => return Err((StatusCode::BAD_REQUEST, Json(json!({"error": format!("Unsupported item type '{}'", other)})))),
    };

    let mut query_params: Vec<(&str, String)> = Vec::new();
    if let Some(mid) = params.movie_id {
        query_params.push(("movieId", mid.to_string()));
    }
    if let Some(sid) = params.series_id {
        query_params.push(("seriesId", sid.to_string()));
    }
    if let Some(eid) = params.episode_id {
        query_params.push(("episodeId", eid.to_string()));
    }
    if let Some(aid) = params.album_id {
        query_params.push(("albumId", aid.to_string()));
    }

    let resp = client.get(&base_url)
        .header("X-Api-Key", &api_key)
        .query(&query_params)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Release query failed: {}", e)}))))?;

    if !resp.status().is_success() {
        return Err((StatusCode::BAD_GATEWAY, Json(json!({"error": format!("Arr instance returned HTTP {}", resp.status())}))));
    }

    let releases: serde_json::Value = resp.json().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to parse releases JSON: {}", e)}))))?;

    Ok(Json(releases))
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct GrabReleasePayload {
    pub item_type: String, // "movie", "series", "music"
    pub guid: String,
    pub indexer_id: i64,
    #[serde(default)]
    pub zone_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/arr/search/grab",
    tag = "Arr Tracking",
    summary = "Grab an indexer release via Sonarr/Radarr/Lidarr",
    description = "Instructs Sonarr, Radarr, or Lidarr to immediately grab a specific release candidate.",
    request_body = GrabReleasePayload,
    responses(
        (status = 200, description = "Release grabbed successfully"),
        (status = 400, description = "Invalid grab request"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn grab_release(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Json(payload): Json<GrabReleasePayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = state.config.get().await;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let (base_url, api_key) = match payload.item_type.as_str() {
        "movie" => {
            let node = if let Some(ref zid) = payload.zone_id {
                config.zones.iter().find(|z| &z.id == zid).and_then(|z| z.radarr.as_ref()).or(config.radarr.primary.as_ref())
            } else {
                config.radarr.primary.as_ref()
            };
            let node = node.ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Radarr is not configured"}))))?;
            (format!("{}/api/v3/release", node.base_url.trim_end_matches('/')), node.api_key.clone())
        }
        "series" => {
            let node = if let Some(ref zid) = payload.zone_id {
                config.zones.iter().find(|z| &z.id == zid).and_then(|z| z.sonarr.as_ref()).or(config.sonarr.primary.as_ref())
            } else {
                config.sonarr.primary.as_ref()
            };
            let node = node.ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Sonarr is not configured"}))))?;
            (format!("{}/api/v3/release", node.base_url.trim_end_matches('/')), node.api_key.clone())
        }
        "music" => {
            let node = if let Some(ref zid) = payload.zone_id {
                config.zones.iter().find(|z| &z.id == zid).and_then(|z| z.lidarr.as_ref()).or(config.lidarr.primary.as_ref())
            } else {
                config.lidarr.primary.as_ref()
            };
            let node = node.ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Lidarr is not configured"}))))?;
            let ver = if node.version == 3 { 1 } else { node.version };
            (format!("{}/api/v{}/release", node.base_url.trim_end_matches('/'), ver), node.api_key.clone())
        }
        other => return Err((StatusCode::BAD_REQUEST, Json(json!({"error": format!("Unsupported item type '{}'", other)})))),
    };

    let post_body = json!({
        "guid": payload.guid,
        "indexerId": payload.indexer_id
    });

    let resp = client.post(&base_url)
        .header("X-Api-Key", &api_key)
        .json(&post_body)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Grab request failed: {}", e)}))))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_txt = resp.text().await.unwrap_or_default();
        return Err((StatusCode::BAD_GATEWAY, Json(json!({"error": format!("Arr grab returned HTTP {}: {}", status, body_txt)}))));
    }

    let res_json: serde_json::Value = resp.json().await.unwrap_or(json!({"status": "ok"}));

    let _ = state.db.log_event(
        "pipeline_manual_grab",
        "info",
        &format!("Triggered manual grab for release guid '{}' on indexer {}", payload.guid, payload.indexer_id),
        None,
    );

    Ok(Json(json!({
        "status": "success",
        "message": "Grab dispatched to Arr",
        "result": res_json
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_media_info_summary() {
        let file_json = json!({
            "mediaInfo": {
                "width": 3840,
                "height": 2160,
                "videoCodec": "HEVC",
                "videoDynamicRangeType": "HDR10",
                "audioCodec": "E-AC-3",
                "audioChannels": 5.1
            }
        });
        let summary = format_media_info_summary(Some(&file_json));
        assert_eq!(summary, Some("4K UHD • HEVC • HDR10 • E-AC-3 5.1".to_string()));
    }

    #[test]
    fn test_format_indexer_flags() {
        let payload = json!({
            "release": {
                "indexerFlags": ["Freeleech", "Scene"]
            }
        });
        let flags = format_indexer_flags(&payload);
        assert_eq!(flags, Some("Freeleech, Scene".to_string()));
    }

    #[test]
    fn test_format_custom_format_summary() {
        let payload = json!({
            "customFormatInfo": {
                "customFormats": [{"id": 1, "name": "Dolby Vision"}, {"id": 2, "name": "Atmos"}],
                "customFormatScore": 1500
            }
        });
        let summary = format_custom_format_summary(&payload);
        assert_eq!(summary, Some("Dolby Vision, Atmos (+1500)".to_string()));
    }

    #[test]
    fn test_extract_poster_url() {
        let obj = json!({
            "images": [
                {"coverType": "banner", "url": "/banner.jpg"},
                {"coverType": "poster", "remoteUrl": "https://image.tmdb.org/t/p/original/poster.jpg"}
            ]
        });
        let poster = extract_poster_url(Some(&obj));
        assert_eq!(poster, Some("https://image.tmdb.org/t/p/original/poster.jpg".to_string()));
    }

    #[tokio::test]
    async fn test_radarr_health_webhook() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let cfg_path = temp_dir.path().join("config.json");
        let db = crate::db::Database::init(&db_path, None).unwrap();
        let config_mgr = crate::config::ConfigManager::load_or_init(&cfg_path).await.unwrap();
        let state = AppState {
            db: db.clone(),
            config: config_mgr,
            fetcher_pool: std::sync::Arc::new(crate::fetcher::FetcherPool::new()),
            notifiers: std::sync::Arc::new(NotificationManager),
            rate_limiter: crate::auth::RateLimiter::new(),
            ws_tickets: crate::auth::WsTicketStore::new(),
            mobile_pairs: crate::auth::MobilePairingStore::new(),
            arr_stats_cache: crate::engines::arr_stats_poller::new_cache(),
            ip_asn_db: crate::ip_asn::new_cache(),
            engine_registry: crate::engines::registry::new_registry(),
            event_bus: crate::events::new_bus(),
        };

        let health_payload = json!({
            "applicationUrl": "",
            "eventType": "Health",
            "instanceName": "Radarr",
            "level": "error",
            "message": "Movies Salem's Lot were removed from TMDb",
            "type": "RemovedMovieCheck",
            "wikiUrl": "https://wiki.servarr.com/radarr/system#movie-was-removed-from-tmdb"
        });

        let res = process_radarr_inbound_direct(state.clone(), health_payload, None).await;
        assert!(res.is_ok());

        // Ensure no junk grab record was saved with "Unknown Movie"
        let grabs = state.db.get_arr_grabs_lookup_map();
        assert_eq!(grabs.len(), 0);

        // Ensure logged into system event log
        let logs = state.db.get_event_logs(10).unwrap();
        assert!(logs.iter().any(|l| l.message.contains("RemovedMovieCheck")));
    }

    #[tokio::test]
    async fn test_sonarr_manual_interaction_webhook() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_sonarr.db");
        let cfg_path = temp_dir.path().join("config_sonarr.json");
        let db = crate::db::Database::init(&db_path, None).unwrap();
        let config_mgr = crate::config::ConfigManager::load_or_init(&cfg_path).await.unwrap();
        let state = AppState {
            db: db.clone(),
            config: config_mgr,
            fetcher_pool: std::sync::Arc::new(crate::fetcher::FetcherPool::new()),
            notifiers: std::sync::Arc::new(NotificationManager),
            rate_limiter: crate::auth::RateLimiter::new(),
            ws_tickets: crate::auth::WsTicketStore::new(),
            mobile_pairs: crate::auth::MobilePairingStore::new(),
            arr_stats_cache: crate::engines::arr_stats_poller::new_cache(),
            ip_asn_db: crate::ip_asn::new_cache(),
            engine_registry: crate::engines::registry::new_registry(),
            event_bus: crate::events::new_bus(),
        };

        let manual_payload = json!({
            "eventType": "ManualInteractionRequired",
            "instanceName": "Sonarr",
            "series": {
                "id": 10,
                "title": "Severance",
                "year": 2022
            },
            "downloadStatus": "FailedPending",
            "downloadStatusMessages": [
                {
                    "title": "Sample Detected",
                    "messages": ["Release contains sample file"]
                }
            ],
            "downloadClient": "Transmission-Pacoli"
        });

        let res = process_sonarr_inbound_direct(state.clone(), manual_payload, None).await;
        assert!(res.is_ok());

        // Ensure logged as warning
        let logs = state.db.get_event_logs(10).unwrap();
        assert!(logs.iter().any(|l| l.level == "warn" && l.message.contains("Sample Detected")));
    }
}

