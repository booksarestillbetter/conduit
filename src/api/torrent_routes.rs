// src/api/torrent_routes.rs
use crate::api::op_error::{is_unsupported, op_failure, status_only, ApiError};
use crate::auth::RequireAuth;
use crate::db::Database;
use crate::fetcher::{
    AddTorrentPayload, AggregateStats, BulkTorrentActionPayload, DetailedTorrent,
    TorrentTimelineEvent, FetcherPool, UnifiedTorrent,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use utoipa::{IntoParams, ToSchema};

// Compiled once at first use rather than per-call — these patterns are static.
static SEASON_EPISODE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bS(\d{1,2})E(\d{1,2})(?:[-~E](\d{1,2}))?\b").unwrap());
static XEP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(\d{1,2})x(\d{1,2})\b").unwrap());
static SEASON_PACK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(?:S|Season[._\s]*)(\d{1,2})\b").unwrap());
static QUALITY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(2160p|1080p|720p|480p|UHD|4K)(?:[._\s]*(WEB-DL|WEBRip|BluRay|REMUX|HDTV|DVDRip))?\b").unwrap());
static LIKELY_SERIES_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(s\d{2}(e\d{2})?|\d{1,2}x\d{1,2}|season|complete\.series)\b").unwrap());
static RELEASE_TAGS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(2160p|1080p|720p|480p|uhd|4k|bluray|blu-ray|remux|web-dl|webrip|dvdrip|hevc|h264|h265|x264|x265|truehd|atmos|dts-hd|dts|ddp5\.1|ac3|flac|aac|s\d{2}(e\d{2})?|\d{1,2}x\d{1,2}|repack|proper)\b.*").unwrap());

#[derive(Debug, Deserialize, IntoParams)]
pub struct TorrentQueryParams {
    /// Optional node name filter (e.g. "idyll", "munro", or "all")
    pub node: Option<String>,
    /// Case-insensitive search across name, tracker host, or info hash
    pub search: Option<String>,
    /// Status filter ("downloading", "seeding", "paused", "error", "checking", "active", "all")
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct DeleteQueryParams {
    /// If true, permanently deletes downloaded payload files from disk
    #[serde(default)]
    pub delete_data: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetLocationPayload {
    /// New absolute destination directory on the node
    pub location: String,
    /// If true, moves existing downloaded files to the new location
    #[serde(default = "default_true")]
    pub move_data: bool,
}

fn default_true() -> bool { true }

#[utoipa::path(
    get,
    path = "/api/torrents",
    tag = "Torrents",
    summary = "List unified torrents",
    description = "Retrieves live torrents across all connected fetcher daemons with compound IDs (node:id).",
    params(TorrentQueryParams),
    responses(
        (status = 200, description = "List of unified torrents", body = [UnifiedTorrent]),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn list_torrents(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    State(db): State<Database>,
    Query(params): Query<TorrentQueryParams>,
) -> Json<Vec<UnifiedTorrent>> {
    let mut torrents = pool.get_torrents(
        params.node.as_deref(),
        params.search.as_deref(),
        params.status.as_deref(),
    );

    let grab_map = db.get_arr_grabs_lookup_map();
    for t in &mut torrents {
        let hash_lower = t.hash_string.to_lowercase();
        let name_lower = t.name.to_lowercase();
        let stem_lower = {
            let mut s = name_lower.as_str();
            for ext in &[".mkv", ".mp4", ".avi", ".ts", ".flac", ".mp3"] {
                if let Some(stripped) = s.strip_suffix(ext) {
                    s = stripped;
                }
            }
            s.to_string()
        };
        if let Some(grab) = grab_map.get(&hash_lower)
            .or_else(|| grab_map.get(&name_lower))
            .or_else(|| grab_map.get(&stem_lower)) {
            t.arr_grab = Some(grab.clone());
        }
    }

    Json(torrents)
}

#[utoipa::path(
    get,
    path = "/api/torrents/stats",
    tag = "Torrents",
    summary = "Get aggregated telemetry & speed stats",
    description = "Returns aggregate and per-node download/upload speeds, storage totals, and ratio distributions.",
    responses(
        (status = 200, description = "Aggregate and per-node statistics", body = AggregateStats),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn get_stats(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
) -> Json<AggregateStats> {
    Json(pool.get_aggregate_stats())
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct BandwidthHistoryQueryParams {
    /// How far back to look, in hours. Clamped to [1, 72]. Defaults to 24.
    pub hours: Option<u32>,
}

#[utoipa::path(
    get,
    path = "/api/torrents/bandwidth-history",
    tag = "Torrents",
    summary = "Get long-range downsampled bandwidth history",
    description = "Returns one-minute-averaged download/upload speed samples for up to the last 72 hours, backed by a small SQLite table (see `Database::insert_bandwidth_point`). Distinct from `AggregateStats.bandwidth_history`, which is a rolling 5-minute, 1-second-resolution in-memory buffer used for the live chart.",
    params(BandwidthHistoryQueryParams),
    responses(
        (status = 200, description = "Downsampled bandwidth history, oldest first", body = [crate::fetcher::BandwidthPoint]),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn get_bandwidth_history(
    _auth: RequireAuth,
    State(db): State<Database>,
    Query(params): Query<BandwidthHistoryQueryParams>,
) -> Result<Json<Vec<crate::fetcher::BandwidthPoint>>, StatusCode> {
    let hours = params.hours.unwrap_or(24).clamp(1, 72) as i64;
    let since_ms = chrono::Utc::now().timestamp_millis() - hours * 3600 * 1000;
    let history = db.list_bandwidth_history(since_ms).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(history))
}

#[utoipa::path(
    get,
    path = "/api/torrents/{compound_id}",
    tag = "Torrents",
    summary = "Get single torrent details & lifecycle timeline",
    description = "Fetches complete details, peers, trackers, file list, Arr grab correlation, and lifecycle timeline for a torrent by compound ID (node:id).",
    params(
        ("compound_id" = String, Path, description = "Compound identifier formatted as '{node_name}:{torrent_id}' (e.g. 'idyll:104')")
    ),
    responses(
        (status = 200, description = "Detailed torrent with peers and timeline", body = DetailedTorrent),
        (status = 404, description = "Torrent not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn get_torrent(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    State(db): State<Database>,
    State(ip_asn_cache): State<crate::ip_asn::IpAsnCache>,
    Path(mut compound_id): Path<String>,
) -> Result<Json<DetailedTorrent>, StatusCode> {
    // If not formatted as node:id, resolve from hash or grab id
    if !compound_id.contains(':') {
        if let Some(t) = pool.get_torrents(None, None, None).into_iter().find(|t| t.hash_string.eq_ignore_ascii_case(&compound_id)) {
            compound_id = t.compound_id;
        } else if let Ok(Some(grab)) = db.get_arr_grab_by_id(&compound_id) {
            compound_id = format!("pipeline:{}", grab.id);
        } else {
            return Err(StatusCode::NOT_FOUND);
        }
    }

    // Check if pipeline item
    let (node, raw_torrent) = if let Some((n, id_str)) = compound_id.split_once(':') {
        if n == "pipeline" {
            let grab_id = id_str;
            if let Ok(Some(grab)) = db.get_arr_grab_by_id(grab_id) {
                // If it has a download_id that exists on a fetcher node, load that torrent instead
                let found_in_tr = grab.download_id.as_ref().and_then(|dl_id| {
                    pool.get_torrents(None, None, None).into_iter().find(|t| t.hash_string.eq_ignore_ascii_case(dl_id))
                });

                if let Some(t) = found_in_tr {
                    let client = pool.get_client(&t.node).ok_or(StatusCode::NOT_FOUND)?;
                    let raw = client.get_torrent_details(t.id).await.unwrap_or(crate::fetcher::Torrent {
                        id: t.id,
                        name: t.name,
                        hash_string: t.hash_string,
                        status: t.raw_status,
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
                        eta_idle: None,
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
                        peers: None,
                        comment: None,
                        creator: None,
                        date_created: None,
                        piece_count: None,
                        piece_size: None,
                        is_private: None,
                        magnet_link: None,
                        corrupt_ever: None,
                        seconds_downloading: None,
                        seconds_seeding: None,
                        activity_date: None,
                        queue_position: 0,
                        sequential_download: false,
                        pieces: None,
                        availability: None,
                    });
                    (t.node, raw)
                } else {
                    let is_imported = grab.status == "imported";
                    let percent = if is_imported { 1.0 } else { 0.0 };
                    let mock_raw = crate::fetcher::Torrent {
                        id: 0,
                        name: grab.title.clone().unwrap_or_else(|| grab.release_title.clone()),
                        hash_string: grab.download_id.clone().unwrap_or_default(),
                        status: if is_imported { 6 } else { 0 },
                        rate_upload: 0,
                        rate_download: 0,
                        uploaded_ever: 0,
                        downloaded_ever: grab.size_bytes.unwrap_or(0),
                        upload_ratio: if is_imported { 1.0 } else { 0.0 },
                        total_size: grab.size_bytes.unwrap_or(0),
                        size_when_done: grab.size_bytes.unwrap_or(0),
                        left_until_done: if is_imported { 0 } else { grab.size_bytes.unwrap_or(0) },
                        percent_done: percent,
                        eta: 0,
                        eta_idle: None,
                        error: 0,
                        error_string: String::new(),
                        peers_connected: 0,
                        peers_sending_to_us: 0,
                        peers_getting_from_us: 0,
                        added_date: grab.created_at.timestamp(),
                        done_date: if is_imported { grab.updated_at.timestamp() } else { 0 },
                        download_dir: "/media/queue (pending fetch)".to_string(),
                        tracker_stats: Vec::new(),
                        files: None,
                        peers: None,
                        comment: None,
                        creator: None,
                        date_created: None,
                        piece_count: None,
                        piece_size: None,
                        is_private: None,
                        magnet_link: None,
                        corrupt_ever: None,
                        seconds_downloading: None,
                        seconds_seeding: None,
                        activity_date: None,
                        queue_position: 0,
                        sequential_download: false,
                        pieces: None,
                        availability: None,
                    };
                    ("pipeline".to_string(), mock_raw)
                }
            } else {
                return Err(StatusCode::NOT_FOUND);
            }
        } else {
            let id: i64 = id_str.parse().map_err(|_| StatusCode::NOT_FOUND)?;
            let raw = if let Some(client) = pool.get_client(n) {
                match client.get_torrent_details(id).await {
                    Ok(t) => t,
                    Err(_) => {
                        if let Some(ut) = pool.get_torrent_by_compound_id(&compound_id) {
                            crate::fetcher::Torrent {
                                id: ut.id,
                                name: ut.name,
                                hash_string: ut.hash_string,
                                status: ut.raw_status,
                                rate_upload: ut.rate_upload,
                                rate_download: ut.rate_download,
                                uploaded_ever: ut.uploaded_ever,
                                downloaded_ever: ut.downloaded_ever,
                                upload_ratio: ut.upload_ratio,
                                total_size: ut.total_size,
                                size_when_done: ut.size_when_done,
                                left_until_done: ut.left_until_done,
                                percent_done: ut.percent_done,
                                eta: ut.eta,
                                eta_idle: None,
                                error: ut.error,
                                error_string: ut.error_string,
                                peers_connected: ut.peers_connected,
                                peers_sending_to_us: ut.peers_sending_to_us,
                                peers_getting_from_us: ut.peers_getting_from_us,
                                added_date: ut.added_date,
                                done_date: ut.done_date,
                                download_dir: ut.download_dir,
                                tracker_stats: ut.tracker_stats,
                                files: ut.files,
                                peers: None,
                                comment: None,
                                creator: None,
                                date_created: None,
                                piece_count: None,
                                piece_size: None,
                                is_private: None,
                                magnet_link: None,
                                corrupt_ever: None,
                                seconds_downloading: None,
                                seconds_seeding: None,
                                activity_date: None,
                                queue_position: ut.queue_position,
                                sequential_download: ut.sequential_download,
                                pieces: None,
                                availability: None,
                            }
                        } else {
                            return Err(StatusCode::NOT_FOUND);
                        }
                    }
                }
            } else if let Some(ut) = pool.get_torrent_by_compound_id(&compound_id) {
                crate::fetcher::Torrent {
                    id: ut.id,
                    name: ut.name,
                    hash_string: ut.hash_string,
                    status: ut.raw_status,
                    rate_upload: ut.rate_upload,
                    rate_download: ut.rate_download,
                    uploaded_ever: ut.uploaded_ever,
                    downloaded_ever: ut.downloaded_ever,
                    upload_ratio: ut.upload_ratio,
                    total_size: ut.total_size,
                    size_when_done: ut.size_when_done,
                    left_until_done: ut.left_until_done,
                    percent_done: ut.percent_done,
                    eta: ut.eta,
                    eta_idle: None,
                    error: ut.error,
                    error_string: ut.error_string,
                    peers_connected: ut.peers_connected,
                    peers_sending_to_us: ut.peers_sending_to_us,
                    peers_getting_from_us: ut.peers_getting_from_us,
                    added_date: ut.added_date,
                    done_date: ut.done_date,
                    download_dir: ut.download_dir,
                    tracker_stats: ut.tracker_stats,
                    files: ut.files,
                    peers: None,
                    comment: None,
                    creator: None,
                    date_created: None,
                    piece_count: None,
                    piece_size: None,
                    is_private: None,
                    magnet_link: None,
                    corrupt_ever: None,
                    seconds_downloading: None,
                    seconds_seeding: None,
                    activity_date: None,
                    queue_position: ut.queue_position,
                    sequential_download: ut.sequential_download,
                    pieces: None,
                    availability: None,
                }
            } else {
                return Err(StatusCode::NOT_FOUND);
            };
            (n.to_string(), raw)
        }
    } else {
        return Err(StatusCode::NOT_FOUND);
    };

    let mut unified = UnifiedTorrent::from_torrent(&node, raw_torrent.clone());

    // Correlate with Arr Grab Record in Database (by hash, full name, or stem)
    let arr_grab = db.find_arr_grab_by_hash_or_name(&raw_torrent.hash_string, &raw_torrent.name).ok().flatten()
        .or_else(|| {
            let name_clean = raw_torrent.name.trim();
            let mut stem = name_clean;
            for ext in &[".mkv", ".mp4", ".avi", ".ts", ".flac", ".mp3"] {
                if let Some(s) = stem.strip_suffix(ext) {
                    stem = s;
                }
            }
            if stem != name_clean {
                db.find_arr_grab_by_hash_or_name("", stem).ok().flatten()
            } else {
                None
            }
        });

    // Check if affected by active circuit breaker
    let breakers = pool.get_active_breakers();
    let mut breaker_hit = None;
    for ts in &unified.tracker_stats {
        let host = ts.host.trim();
        if let Some(b) = breakers.get(host) {
            breaker_hit = Some(b.clone());
            if unified.compound_id == b.canary_compound_id {
                unified.is_canary_probe = true;
                unified.circuit_breaker_reason = Some(format!(
                    "Active Canary Probe: Monitoring '{}' ({})",
                    b.tracker_host, b.failing_error
                ));
            } else {
                unified.is_circuit_broken = true;
                unified.circuit_breaker_reason = Some(format!(
                    "Swarm Pressure Relieved: Tracker '{}' unreachable ({})",
                    b.tracker_host, b.failing_error
                ));
            }
            break;
        }
    }

    // Construct Visual Lifecycle Timeline (Conduit's Trail)
    let mut timeline: Vec<TorrentTimelineEvent> = Vec::new();

    // 1. Conduit Sniffed / Arr Grab Correlation
    if let Some(ref grab) = arr_grab {
        let app_name = if grab.item_type == "series" || grab.item_type == "tv" {
            "Sonarr TV"
        } else if grab.item_type == "music" {
            "Lidarr Music"
        } else {
            "Radarr Movie"
        };
        let indexer_name = grab.indexer.as_deref().unwrap_or("Arr Indexer");
        timeline.push(TorrentTimelineEvent {
            stage: "sniffed".to_string(),
            title: format!("🦴 Conduit Sniffed Kibble ({})", app_name),
            description: format!("Tracker / Indexer: {} | Release: {}", indexer_name, grab.release_title),
            timestamp: grab.created_at.timestamp(),
            formatted_time: grab.created_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            status: "success".to_string(),
        });
    }

    // 2. Added to Fetcher
    if raw_torrent.added_date > 0 {
        let dt = DateTime::from_timestamp(raw_torrent.added_date, 0).unwrap_or_else(Utc::now);
        timeline.push(TorrentTimelineEvent {
            stage: "fetching".to_string(),
            title: format!("🐾 Fetching on Node '{}'", node),
            description: format!("Assigned destination folder: '{}'", raw_torrent.download_dir),
            timestamp: raw_torrent.added_date,
            formatted_time: dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            status: "success".to_string(),
        });
    }

    // 3. Download Completed / Staged
    if raw_torrent.done_date > 0 {
        let dt = DateTime::from_timestamp(raw_torrent.done_date, 0).unwrap_or_else(Utc::now);
        let size_gb = (raw_torrent.total_size as f64) / 1_073_741_824.0;
        let file_count = raw_torrent.files.as_ref().map(|f| f.len()).unwrap_or(1);
        timeline.push(TorrentTimelineEvent {
            stage: "staged".to_string(),
            title: "🎾 Conduit Intake Staged & Hardlinked".to_string(),
            description: format!("Downloaded {:.2} GB payload across {} file(s) and hardlinked to intake queue", size_gb, file_count),
            timestamp: raw_torrent.done_date,
            formatted_time: dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            status: "success".to_string(),
        });
    } else if raw_torrent.percent_done < 1.0 && raw_torrent.status == 4 {
        let act_ts = raw_torrent.activity_date.unwrap_or(raw_torrent.added_date);
        let dt = DateTime::from_timestamp(act_ts, 0).unwrap_or_else(Utc::now);
        timeline.push(TorrentTimelineEvent {
            stage: "downloading".to_string(),
            title: format!("🐾 Active Fetch ({:.1}%)", raw_torrent.percent_done * 100.0),
            description: format!("{:.2} GB left of {:.2} GB at {:.2} MB/s",
                (raw_torrent.left_until_done as f64) / 1_073_741_824.0,
                (raw_torrent.total_size as f64) / 1_073_741_824.0,
                (raw_torrent.rate_download as f64) / 1_048_576.0
            ),
            timestamp: act_ts,
            formatted_time: dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            status: "info".to_string(),
        });
    }

    // 3b. Retrieved & Imported to Media Library
    if let Some(ref grab) = arr_grab {
        let is_imported = grab.status == "imported" || grab.event_type == "Download" || {
            if let Ok(history) = db.list_grab_history(&grab.id) {
                history.iter().any(|h| h.status == "imported" || h.event_type.eq_ignore_ascii_case("Download"))
            } else {
                false
            }
        };

        if is_imported {
            let app_name = if grab.item_type == "series" || grab.item_type == "tv" {
                "Sonarr TV"
            } else if grab.item_type == "music" {
                "Lidarr Music"
            } else {
                "Radarr Movie"
            };
            timeline.push(TorrentTimelineEvent {
                stage: "imported".to_string(),
                title: format!("🏆 {} Ingest Completed & Live in Library", app_name),
                description: format!("Media manager successfully processed and stored '{}'", grab.title.as_deref().unwrap_or(&grab.release_title)),
                timestamp: grab.updated_at.timestamp(),
                formatted_time: grab.updated_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                status: "success".to_string(),
            });
        }
    }

    // 4. Seeding & Ratio Milestone
    let seed_ts = if raw_torrent.done_date > 0 { raw_torrent.done_date } else { raw_torrent.added_date };
    let dt = DateTime::from_timestamp(seed_ts, 0).unwrap_or_else(Utc::now);
    let seeding_hours = (raw_torrent.seconds_seeding.unwrap_or(0) as f64) / 3600.0;
    let up_gb = (raw_torrent.uploaded_ever as f64) / 1_073_741_824.0;
    let (ratio_status, ratio_title) = if raw_torrent.upload_ratio >= 2.0 {
        ("success", format!("🍖 Target Ratio Achieved ({:.2}x)", raw_torrent.upload_ratio))
    } else if raw_torrent.upload_ratio >= 1.0 {
        ("info", format!("🍖 Break-even Ratio ({:.2}x)", raw_torrent.upload_ratio))
    } else {
        ("pending", format!("🍖 Guarding & Seeding ({:.2}x)", raw_torrent.upload_ratio))
    };

    timeline.push(TorrentTimelineEvent {
        stage: "seeding".to_string(),
        title: ratio_title,
        description: format!("Uploaded {:.2} GB across {} peer(s). Total seeding duration: {:.1} hrs", up_gb, raw_torrent.peers_connected, seeding_hours),
        timestamp: seed_ts,
        formatted_time: dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        status: ratio_status.to_string(),
    });

    // 5. Space Management / Circuit Breaker / Tracker Health Status
    if let Some(ref b) = breaker_hit {
        if unified.is_canary_probe {
            timeline.push(TorrentTimelineEvent {
                stage: "canary_probe".to_string(),
                title: "⚡ Active Canary Probe (Swarm Pressure Relief)".to_string(),
                description: format!(
                    "Tracker '{}' is unreachable ({}). This fetcher is actively running as the single probe monitoring for recovery.",
                    b.tracker_host, b.failing_error
                ),
                timestamp: b.tripped_at,
                formatted_time: DateTime::from_timestamp(b.tripped_at, 0).unwrap_or_else(Utc::now).format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                status: "warning".to_string(),
            });
        } else {
            timeline.push(TorrentTimelineEvent {
                stage: "circuit_breaker".to_string(),
                title: "⚡ Paused by Swarm Pressure Relief".to_string(),
                description: format!(
                    "Tracker '{}' is unreachable ({}). Paused to relieve tracker pressure while canary probe '{}' monitors for recovery.",
                    b.tracker_host, b.failing_error, b.canary_name
                ),
                timestamp: b.tripped_at,
                formatted_time: DateTime::from_timestamp(b.tripped_at, 0).unwrap_or_else(Utc::now).format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                status: "warning".to_string(),
            });
        }
    } else if raw_torrent.error != 0 {
        timeline.push(TorrentTimelineEvent {
            stage: "error".to_string(),
            title: "🚨 Swarm / Tracker Error".to_string(),
            description: raw_torrent.error_string.clone(),
            timestamp: Utc::now().timestamp(),
            formatted_time: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            status: "error".to_string(),
        });
    } else {
        let failing_tracker = raw_torrent.tracker_stats.iter().find(|ts| !ts.last_announce_succeeded && !ts.last_announce_result.is_empty());
        if let Some(ft) = failing_tracker {
            timeline.push(TorrentTimelineEvent {
                stage: "tracker_warning".to_string(),
                title: "⚠️ Tracker Announce Warning".to_string(),
                description: format!("Tracker '{}': {}", ft.host, ft.last_announce_result),
                timestamp: Utc::now().timestamp(),
                formatted_time: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                status: "warning".to_string(),
            });
        } else {
            let first_tracker = raw_torrent.tracker_stats.first().map(|ts| ts.host.as_str()).unwrap_or("Active Swarm");
            timeline.push(TorrentTimelineEvent {
                stage: "healthy".to_string(),
                title: "🦮 Tracker Registered & Healthy".to_string(),
                description: format!("Tracker: {} | Auto-Purge Eligibility Target: 2.0x", first_tracker),
                timestamp: Utc::now().timestamp(),
                formatted_time: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                status: "success".to_string(),
            });
        }
    }

    Ok(Json(DetailedTorrent {
        unified,
        comment: raw_torrent.comment.unwrap_or_default(),
        creator: raw_torrent.creator.unwrap_or_default(),
        date_created: raw_torrent.date_created.unwrap_or(0),
        piece_count: raw_torrent.piece_count.unwrap_or(0),
        piece_size: raw_torrent.piece_size.unwrap_or(0),
        is_private: raw_torrent.is_private.unwrap_or(false),
        magnet_link: raw_torrent.magnet_link.unwrap_or_default(),
        corrupt_ever: raw_torrent.corrupt_ever.unwrap_or(0),
        seconds_downloading: raw_torrent.seconds_downloading.unwrap_or(0),
        seconds_seeding: raw_torrent.seconds_seeding.unwrap_or(0),
        activity_date: raw_torrent.activity_date.unwrap_or(0),
        queue_position: raw_torrent.queue_position,
        sequential_download: raw_torrent.sequential_download,
        pieces: raw_torrent.pieces,
        availability: raw_torrent.availability,
        peers: raw_torrent.peers.unwrap_or_default().into_iter().map(|mut peer| {
            // Transmission's RPC never actually populates countryCode — this is Conduit's own
            // enrichment (opt-in, see config.ip_asn.enabled) filling in both fields at once.
            if let Some(info) = crate::ip_asn::lookup(&ip_asn_cache, &peer.address) {
                peer.country_code = Some(info.country_code);
                peer.as_name = Some(info.as_name);
            }
            peer
        }).collect(),
        timeline,
        arr_grab,
    }))
}

#[utoipa::path(
    post,
    path = "/api/torrents",
    tag = "Torrents",
    summary = "Add new torrent",
    description = "Adds a torrent to a target fetcher daemon instance via magnet link, HTTP URL, or base64-encoded .torrent file.",
    request_body = AddTorrentPayload,
    responses(
        (status = 200, description = "Torrent added successfully"),
        (status = 400, description = "Invalid payload or unknown node"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn add_torrent(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Json(payload): Json<AddTorrentPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client = pool.get_client(&payload.node)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Node '{}' not found or disabled", payload.node)}))))?;

    let res = client.add_torrent(
        payload.magnet_or_url.as_deref(),
        payload.metainfo_base64.as_deref(),
        payload.download_dir.as_deref(),
        payload.paused,
    ).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    if payload.sequential_download {
        let added_id = res.get("torrent-added")
            .or_else(|| res.get("torrent_added"))
            .or_else(|| res.get("torrent-duplicate"))
            .or_else(|| res.get("torrent_duplicate"))
            .and_then(|t| t.get("id"))
            .and_then(|id| id.as_i64());
        if let Some(id) = added_id {
            let _ = client.set_sequential_download(&[id], true).await;
        }
    }

    Ok(Json(res))
}

#[utoipa::path(
    post,
    path = "/api/torrents/bulk",
    tag = "Torrents",
    summary = "Execute bulk action across multiple nodes",
    description = "Executes start, stop, verify, reannounce, set_location, or delete actions across any selection of torrents.",
    request_body = BulkTorrentActionPayload,
    responses(
        (status = 200, description = "Bulk action execution report by node"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn bulk_action(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Json(payload): Json<BulkTorrentActionPayload>,
) -> Result<Json<HashMap<String, crate::fetcher::BulkItemResult>>, (StatusCode, Json<serde_json::Value>)> {
    let results = pool.execute_bulk_action(&payload).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(results))
}

#[utoipa::path(
    post,
    path = "/api/torrents/{compound_id}/start",
    tag = "Torrents",
    summary = "Resume/Start single torrent",
    description = "Starts download/seeding of a single torrent by compound ID.",
    params(
        ("compound_id" = String, Path, description = "Compound ID ('{node}:{id}')")
    ),
    responses(
        (status = 200, description = "Torrent started successfully"),
        (status = 404, description = "Node or torrent not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn start_torrent(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Path(compound_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (node, id_str) = compound_id.split_once(':')
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid compound ID format"}))))?;
    let id: i64 = id_str.parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid torrent ID integer"}))))?;

    let client = pool.get_client(node)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Node not found"}))))?;

    client.start_torrents(&[id], false).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({"message": "Torrent started"})))
}

#[utoipa::path(
    post,
    path = "/api/torrents/{compound_id}/stop",
    tag = "Torrents",
    summary = "Pause/Stop single torrent",
    description = "Pauses a single torrent by compound ID.",
    params(
        ("compound_id" = String, Path, description = "Compound ID ('{node}:{id}')")
    ),
    responses(
        (status = 200, description = "Torrent paused successfully"),
        (status = 404, description = "Node or torrent not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn stop_torrent(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Path(compound_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (node, id_str) = compound_id.split_once(':')
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid compound ID format"}))))?;
    let id: i64 = id_str.parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid torrent ID integer"}))))?;

    let client = pool.get_client(node)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Node not found"}))))?;

    client.stop_torrents(&[id]).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({"message": "Torrent stopped"})))
}

#[utoipa::path(
    delete,
    path = "/api/torrents/{compound_id}",
    tag = "Torrents",
    summary = "Delete single torrent",
    description = "Removes a torrent from its fetcher node, with option to permanently purge downloaded payload files from disk.",
    params(
        ("compound_id" = String, Path, description = "Compound ID ('{node}:{id}')"),
        DeleteQueryParams
    ),
    responses(
        (status = 200, description = "Torrent deleted successfully"),
        (status = 404, description = "Node or torrent not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn delete_torrent(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Path(compound_id): Path<String>,
    Query(params): Query<DeleteQueryParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (node, id_str) = compound_id.split_once(':')
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid compound ID format"}))))?;
    let id: i64 = id_str.parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid torrent ID integer"}))))?;

    let client = pool.get_client(node)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Node not found"}))))?;

    client.remove_torrents(&[id], params.delete_data).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({"message": "Torrent deleted"})))
}

#[utoipa::path(
    post,
    path = "/api/torrents/{compound_id}/location",
    tag = "Torrents",
    summary = "Set torrent download directory",
    description = "Changes the download directory for a torrent and optionally moves files.",
    params(
        ("compound_id" = String, Path, description = "Compound ID ('{node}:{id}')")
    ),
    request_body = SetLocationPayload,
    responses(
        (status = 200, description = "Location updated successfully"),
        (status = 404, description = "Node or torrent not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn set_location(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Path(compound_id): Path<String>,
    Json(payload): Json<SetLocationPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (node, id_str) = compound_id.split_once(':')
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid compound ID format"}))))?;
    let id: i64 = id_str.parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid torrent ID integer"}))))?;

    let client = pool.get_client(node)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Node not found"}))))?;

    client.set_location(&[id], &payload.location, payload.move_data).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({"message": "Location updated"})))
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct MigrateTorrentPayload {
    pub source_node: String,
    pub target_node: String,
    pub hash: String,
    #[serde(default)]
    pub target_download_dir: Option<String>,
    #[serde(default)]
    pub delete_source_torrent: bool,
    #[serde(default)]
    pub delete_source_data: bool,
}

#[utoipa::path(
    post,
    path = "/api/torrents/migrate",
    tag = "Torrents",
    summary = "Migrate a torrent between retriever nodes",
    request_body = MigrateTorrentPayload,
    responses(
        (status = 200, description = "Torrent migration initiated"),
        (status = 404, description = "Source or target node not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn migrate_torrent(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    State(db): State<Database>,
    Json(payload): Json<MigrateTorrentPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let source_client = pool.get_client(&payload.source_node)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": format!("Source node '{}' not found", payload.source_node)}))))?;
    let target_client = pool.get_client(&payload.target_node)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": format!("Target node '{}' not found", payload.target_node)}))))?;

    // 1. Fetch source torrent details
    let torrent = source_client.get_torrent_details_by_hash(&payload.hash).await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": format!("Torrent not found on source node: {}", e)}))))?;

    // 2. Build magnet link or metainfo
    let magnet = if let Some(ref m) = torrent.magnet_link {
        if !m.is_empty() {
            Some(m.as_str())
        } else {
            None
        }
    } else {
        None
    };

    let fallback_magnet = format!("magnet:?xt=urn:btih:{}&dn={}", torrent.hash_string, urlencoding::encode(&torrent.name));
    let final_magnet = magnet.unwrap_or(&fallback_magnet);

    // 3. Determine target download dir
    let target_dir = payload.target_download_dir.as_deref().or(if !torrent.download_dir.is_empty() { Some(&torrent.download_dir) } else { None });

    // 4. Add to target node
    target_client.add_torrent(Some(final_magnet), None, target_dir, false).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to add torrent to target node '{}': {}", payload.target_node, e)}))))?;

    // 5. Optionally remove from source node
    if payload.delete_source_torrent {
        let _ = source_client.remove_torrents(&[torrent.id], payload.delete_source_data).await;
    }

    let log_msg = format!("Migrated torrent '{}' ({}) from node '{}' to node '{}'", torrent.name, torrent.hash_string, payload.source_node, payload.target_node);
    let _ = db.log_event("torrent_migration", "info", &log_msg, None);

    Ok(Json(json!({
        "message": "Torrent migrated successfully",
        "name": torrent.name,
        "hash": torrent.hash_string,
        "source_node": payload.source_node,
        "target_node": payload.target_node
    })))
}

pub fn infer_tracker_name_from_string(text: &str, qr: &crate::config::QueueRoutingConfig) -> String {
    let lower = text.to_lowercase();
    for rule in &qr.tracker_mappings {
        let pat = rule.pattern.trim_matches('*').to_lowercase();
        if !pat.is_empty() && lower.contains(&pat) {
            return rule.media_type.clone();
        }
    }

    if lower.contains("broadcasthe") || lower.contains("btn") {
        "BroadcasTheNet".to_string()
    } else if lower.contains("passthepopcorn") || lower.contains("ptp") {
        "PassThePopcorn".to_string()
    } else if lower.contains("redacted") || lower.contains("red.ch") {
        "Redacted".to_string()
    } else if lower.contains("orpheus") || lower.contains("ops") {
        "Orpheus".to_string()
    } else if lower.contains("animebytes") || lower.contains("ab") {
        "AnimeBytes".to_string()
    } else if lower.contains("gazellegames") || lower.contains("ggn") {
        "GazelleGames".to_string()
    } else if lower.contains("bibliotik") {
        "Bibliotik".to_string()
    } else if lower.contains("morethantv") || lower.contains("mtv") {
        "MoreThanTV".to_string()
    } else if lower.contains("hdbits") {
        "HDBits".to_string()
    } else if lower.contains("torrentleech") || lower.contains("tl") {
        "TorrentLeech".to_string()
    } else if lower.contains("alpharatio") {
        "AlphaRatio".to_string()
    } else if lower.contains("iptorrents") || lower.contains("ipt") {
        "IPTorrents".to_string()
    } else if lower.contains("digitalcore") {
        "DigitalCore".to_string()
    } else if lower.contains("filelist") {
        "FileList".to_string()
    } else if lower.contains("avistaz") {
        "Avistaz".to_string()
    } else if lower.contains("cinemaz") {
        "CinemaZ".to_string()
    } else if lower.contains("animez") {
        "AnimeZ".to_string()
    } else if lower.contains("myspleen") {
        "MySpleen".to_string()
    } else if lower.contains("privatehd") {
        "PrivateHD".to_string()
    } else if lower.contains("secret-cinema") {
        "SecretCinema".to_string()
    } else {
        "Direct Intake".to_string()
    }
}

struct SonarrGrabMatch {
    pub series_title: String,
    pub year: Option<i64>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub genres: Option<String>,
    pub series_id: Option<i64>,
    pub season_number: Option<i64>,
    pub episode_numbers: Option<String>,
    pub tvdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub indexer: String,
    pub download_client: Option<String>,
    pub release_title: String,
    pub quality: Option<String>,
    pub size_bytes: Option<i64>,
    pub raw_json: String,
}

async fn find_sonarr_queue_or_history(cfg: &crate::config::AppConfig, hash: &str, name: &str) -> Option<SonarrGrabMatch> {
    if !cfg.sonarr.enabled {
        return None;
    }
    let primary = cfg.sonarr.primary.as_ref()?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    let base = primary.base_url.trim_end_matches('/');

    // 1. Check Sonarr Active Queue: GET /api/v3/queue?includeSeries=true&includeEpisode=true
    let queue_url = format!("{}/api/v3/queue", base);
    if let Ok(resp) = client.get(&queue_url)
        .header("X-Api-Key", &primary.api_key)
        .query(&[("includeSeries", "true"), ("includeEpisode", "true")])
        .send().await
    {
        if let Ok(queue_val) = resp.json::<serde_json::Value>().await {
            let records = queue_val.get("records").and_then(|r| r.as_array()).or_else(|| queue_val.as_array());
            if let Some(recs) = records {
                for rec in recs {
                    let dl_id = rec.get("downloadId").and_then(|v| v.as_str()).unwrap_or("");
                    let title = rec.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let matches_hash = !dl_id.is_empty() && dl_id.eq_ignore_ascii_case(hash);
                    let matches_title = !title.is_empty() && (name.contains(title) || title.contains(name));

                    if matches_hash || matches_title {
                        let series = rec.get("series");
                        let series_title = series.and_then(|s| s.get("title")).and_then(|v| v.as_str()).unwrap_or(title).to_string();
                        let year = series.and_then(|s| s.get("year")).and_then(|v| v.as_i64());
                        let overview = series.and_then(|s| s.get("overview")).and_then(|v| v.as_str()).map(|s| s.to_string());
                        let tvdb_id = series.and_then(|s| s.get("tvdbId")).and_then(|v| v.as_i64());
                        let imdb_id = series.and_then(|s| s.get("imdbId")).and_then(|v| v.as_str()).map(|s| s.to_string());
                        let series_id = series.and_then(|s| s.get("id")).and_then(|v| v.as_i64());

                        let poster = series.and_then(|s| s.get("images")).and_then(|i| i.as_array()).and_then(|imgs| {
                            imgs.iter().find(|img| img.get("coverType").and_then(|c| c.as_str()) == Some("poster"))
                                .and_then(|p| p.get("remoteUrl").or_else(|| p.get("url")))
                                .and_then(|u| u.as_str())
                                .map(|s| s.to_string())
                        });

                        let genres = series.and_then(|s| s.get("genres")).and_then(|g| g.as_array()).map(|arr| {
                            arr.iter().filter_map(|g| g.as_str()).collect::<Vec<_>>().join(", ")
                        });

                        let episode = rec.get("episode");
                        let season_number = episode.and_then(|e| e.get("seasonNumber")).and_then(|v| v.as_i64());
                        let episode_number = episode.and_then(|e| e.get("episodeNumber")).and_then(|v| v.as_i64());
                        let episode_numbers = episode_number.map(|e| serde_json::to_string(&vec![e]).unwrap_or_default());

                        let raw_indexer = rec.get("indexer").and_then(|v| v.as_str()).unwrap_or("");
                        let indexer = if !raw_indexer.is_empty() && !raw_indexer.eq_ignore_ascii_case("unknown") {
                            raw_indexer.to_string()
                        } else {
                            infer_tracker_name_from_string(title, &cfg.queue_routing)
                        };

                        let download_client = rec.get("downloadClient").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let size_bytes = rec.get("size").and_then(|v| v.as_i64());
                        let quality = rec.get("quality").and_then(|q| q.get("quality")).and_then(|q| q.get("name")).and_then(|n| n.as_str()).map(|s| s.to_string());

                        return Some(SonarrGrabMatch {
                            series_title,
                            year,
                            overview,
                            poster_url: poster,
                            genres,
                            series_id,
                            season_number,
                            episode_numbers,
                            tvdb_id,
                            imdb_id,
                            indexer,
                            download_client,
                            release_title: if !title.is_empty() { title.to_string() } else { name.to_string() },
                            quality,
                            size_bytes,
                            raw_json: rec.to_string(),
                        });
                    }
                }
            }
        }
    }

    // 2. Check Sonarr History: GET /api/v3/history?eventType=1&pageSize=50
    let history_url = format!("{}/api/v3/history", base);
    if let Ok(resp) = client.get(&history_url)
        .header("X-Api-Key", &primary.api_key)
        .query(&[("eventType", "1"), ("pageSize", "50")])
        .send().await
    {
        if let Ok(hist_val) = resp.json::<serde_json::Value>().await {
            let records = hist_val.get("records").and_then(|r| r.as_array()).or_else(|| hist_val.as_array());
            if let Some(recs) = records {
                for rec in recs {
                    let dl_id = rec.get("downloadId").and_then(|v| v.as_str()).unwrap_or("");
                    let source_title = rec.get("sourceTitle").and_then(|v| v.as_str()).unwrap_or("");
                    let matches_hash = !dl_id.is_empty() && dl_id.eq_ignore_ascii_case(hash);
                    let matches_title = !source_title.is_empty() && (name.contains(source_title) || source_title.contains(name));

                    if matches_hash || matches_title {
                        let series = rec.get("series");
                        let series_title = series.and_then(|s| s.get("title")).and_then(|v| v.as_str()).unwrap_or(source_title).to_string();
                        let year = series.and_then(|s| s.get("year")).and_then(|v| v.as_i64());
                        let overview = series.and_then(|s| s.get("overview")).and_then(|v| v.as_str()).map(|s| s.to_string());
                        let tvdb_id = series.and_then(|s| s.get("tvdbId")).and_then(|v| v.as_i64());
                        let imdb_id = series.and_then(|s| s.get("imdbId")).and_then(|v| v.as_str()).map(|s| s.to_string());
                        let series_id = series.and_then(|s| s.get("id")).and_then(|v| v.as_i64());

                        let poster = series.and_then(|s| s.get("images")).and_then(|i| i.as_array()).and_then(|imgs| {
                            imgs.iter().find(|img| img.get("coverType").and_then(|c| c.as_str()) == Some("poster"))
                                .and_then(|p| p.get("remoteUrl").or_else(|| p.get("url")))
                                .and_then(|u| u.as_str())
                                .map(|s| s.to_string())
                        });

                        let genres = series.and_then(|s| s.get("genres")).and_then(|g| g.as_array()).map(|arr| {
                            arr.iter().filter_map(|g| g.as_str()).collect::<Vec<_>>().join(", ")
                        });

                        let data = rec.get("data");
                        let raw_indexer = data.and_then(|d| d.get("indexer")).and_then(|v| v.as_str()).unwrap_or("");
                        let indexer = if !raw_indexer.is_empty() && !raw_indexer.eq_ignore_ascii_case("unknown") {
                            raw_indexer.to_string()
                        } else {
                            infer_tracker_name_from_string(source_title, &cfg.queue_routing)
                        };

                        let download_client = data.and_then(|d| d.get("downloadClient")).and_then(|v| v.as_str()).map(|s| s.to_string());
                        let quality = rec.get("quality").and_then(|q| q.get("quality")).and_then(|q| q.get("name")).and_then(|n| n.as_str()).map(|s| s.to_string());

                        return Some(SonarrGrabMatch {
                            series_title,
                            year,
                            overview,
                            poster_url: poster,
                            genres,
                            series_id,
                            season_number: None,
                            episode_numbers: None,
                            tvdb_id,
                            imdb_id,
                            indexer,
                            download_client,
                            release_title: if !source_title.is_empty() { source_title.to_string() } else { name.to_string() },
                            quality,
                            size_bytes: None,
                            raw_json: rec.to_string(),
                        });
                    }
                }
            }
        }
    }

    None
}

struct RadarrGrabMatch {
    pub movie_title: String,
    pub year: Option<i64>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub genres: Option<String>,
    pub movie_id: Option<i64>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub indexer: String,
    pub download_client: Option<String>,
    pub release_title: String,
    pub quality: Option<String>,
    pub size_bytes: Option<i64>,
    pub raw_json: String,
}

async fn find_radarr_queue_or_history(cfg: &crate::config::AppConfig, hash: &str, name: &str) -> Option<RadarrGrabMatch> {
    if !cfg.radarr.enabled {
        return None;
    }
    let primary = cfg.radarr.primary.as_ref()?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    let base = primary.base_url.trim_end_matches('/');

    let queue_url = format!("{}/api/v3/queue", base);
    if let Ok(resp) = client.get(&queue_url)
        .header("X-Api-Key", &primary.api_key)
        .query(&[("includeMovie", "true")])
        .send().await
    {
        if let Ok(queue_val) = resp.json::<serde_json::Value>().await {
            let records = queue_val.get("records").and_then(|r| r.as_array()).or_else(|| queue_val.as_array());
            if let Some(recs) = records {
                for rec in recs {
                    let dl_id = rec.get("downloadId").and_then(|v| v.as_str()).unwrap_or("");
                    let title = rec.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let matches_hash = !dl_id.is_empty() && dl_id.eq_ignore_ascii_case(hash);
                    let matches_title = !title.is_empty() && (name.contains(title) || title.contains(name));

                    if matches_hash || matches_title {
                        let movie = rec.get("movie");
                        let movie_title = movie.and_then(|m| m.get("title")).and_then(|v| v.as_str()).unwrap_or(title).to_string();
                        let year = movie.and_then(|m| m.get("year")).and_then(|v| v.as_i64());
                        let overview = movie.and_then(|m| m.get("overview")).and_then(|v| v.as_str()).map(|s| s.to_string());
                        let tmdb_id = movie.and_then(|m| m.get("tmdbId")).and_then(|v| v.as_i64());
                        let imdb_id = movie.and_then(|m| m.get("imdbId")).and_then(|v| v.as_str()).map(|s| s.to_string());
                        let movie_id = movie.and_then(|m| m.get("id")).and_then(|v| v.as_i64());

                        let poster = movie.and_then(|m| m.get("images")).and_then(|i| i.as_array()).and_then(|imgs| {
                            imgs.iter().find(|img| img.get("coverType").and_then(|c| c.as_str()) == Some("poster"))
                                .and_then(|p| p.get("remoteUrl").or_else(|| p.get("url")))
                                .and_then(|u| u.as_str())
                                .map(|s| s.to_string())
                        });

                        let genres = movie.and_then(|m| m.get("genres")).and_then(|g| g.as_array()).map(|arr| {
                            arr.iter().filter_map(|g| g.as_str()).collect::<Vec<_>>().join(", ")
                        });

                        let raw_indexer = rec.get("indexer").and_then(|v| v.as_str()).unwrap_or("");
                        let indexer = if !raw_indexer.is_empty() && !raw_indexer.eq_ignore_ascii_case("unknown") {
                            raw_indexer.to_string()
                        } else {
                            infer_tracker_name_from_string(title, &cfg.queue_routing)
                        };

                        let download_client = rec.get("downloadClient").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let size_bytes = rec.get("size").and_then(|v| v.as_i64());
                        let quality = rec.get("quality").and_then(|q| q.get("quality")).and_then(|q| q.get("name")).and_then(|n| n.as_str()).map(|s| s.to_string());

                        return Some(RadarrGrabMatch {
                            movie_title,
                            year,
                            overview,
                            poster_url: poster,
                            genres,
                            movie_id,
                            tmdb_id,
                            imdb_id,
                            indexer,
                            download_client,
                            release_title: if !title.is_empty() { title.to_string() } else { name.to_string() },
                            quality,
                            size_bytes,
                            raw_json: rec.to_string(),
                        });
                    }
                }
            }
        }
    }

    None
}

async fn lookup_sonarr(cfg: &crate::config::AppConfig, query_str: &str) -> Option<serde_json::Value> {
    if cfg.sonarr.enabled {
        if let Some(primary) = cfg.sonarr.primary.as_ref() {
            let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build().unwrap_or_default();
            let url = format!("{}/api/v3/series/lookup", primary.base_url.trim_end_matches('/'));
            if let Ok(resp) = client.get(&url).header("X-Api-Key", &primary.api_key).query(&[("term", query_str)]).send().await {
                if let Ok(results) = resp.json::<Vec<serde_json::Value>>().await {
                    if let Some(first) = results.first() {
                        return Some(first.clone());
                    }
                }
            }
        }
    }
    None
}

async fn lookup_radarr(cfg: &crate::config::AppConfig, query_str: &str) -> Option<serde_json::Value> {
    if cfg.radarr.enabled {
        if let Some(primary) = cfg.radarr.primary.as_ref() {
            let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build().unwrap_or_default();
            let url = format!("{}/api/v3/movie/lookup", primary.base_url.trim_end_matches('/'));
            if let Ok(resp) = client.get(&url).header("X-Api-Key", &primary.api_key).query(&[("term", query_str)]).send().await {
                if let Ok(results) = resp.json::<Vec<serde_json::Value>>().await {
                    if let Some(first) = results.first() {
                        return Some(first.clone());
                    }
                }
            }
        }
    }
    None
}

async fn lookup_lidarr(cfg: &crate::config::AppConfig, query_str: &str) -> Option<serde_json::Value> {
    if cfg.lidarr.enabled {
        if let Some(primary) = cfg.lidarr.primary.as_ref() {
            let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build().unwrap_or_default();
            let url = format!("{}/api/v1/search", primary.base_url.trim_end_matches('/'));
            if let Ok(resp) = client.get(&url).header("X-Api-Key", &primary.api_key).query(&[("term", query_str)]).send().await {
                if let Ok(results) = resp.json::<Vec<serde_json::Value>>().await {
                    if let Some(first) = results.first() {
                        return Some(first.clone());
                    }
                }
            }
        }
    }
    None
}

fn parse_media_metadata_from_name(name: &str) -> (Option<i64>, Option<String>, Option<String>) {
    // 1. Season and Episode
    let mut season_number = None;
    let mut episode_numbers = None;

    if let Some(caps) = SEASON_EPISODE_RE.captures(name) {
        if let Some(s) = caps.get(1).and_then(|m| m.as_str().parse::<i64>().ok()) {
            season_number = Some(s);
        }
        if let Some(e1) = caps.get(2).and_then(|m| m.as_str().parse::<i64>().ok()) {
            if let Some(e2) = caps.get(3).and_then(|m| m.as_str().parse::<i64>().ok()) {
                let eps: Vec<i64> = (e1..=e2).collect();
                episode_numbers = Some(serde_json::to_string(&eps).unwrap_or_default());
            } else {
                episode_numbers = Some(serde_json::to_string(&vec![e1]).unwrap_or_default());
            }
        }
    } else if let Some(caps) = XEP_RE.captures(name) {
        if let Some(s) = caps.get(1).and_then(|m| m.as_str().parse::<i64>().ok()) {
            season_number = Some(s);
        }
        if let Some(e) = caps.get(2).and_then(|m| m.as_str().parse::<i64>().ok()) {
            episode_numbers = Some(serde_json::to_string(&vec![e]).unwrap_or_default());
        }
    } else if let Some(caps) = SEASON_PACK_RE.captures(name) {
        if let Some(s) = caps.get(1).and_then(|m| m.as_str().parse::<i64>().ok()) {
            season_number = Some(s);
        }
    }

    // 2. Quality
    let quality = QUALITY_RE.captures(name).map(|caps| {
        let res = caps.get(1).map(|m| m.as_str().to_uppercase()).unwrap_or_default();
        if let Some(src) = caps.get(2) {
            format!("{} {}", res, src.as_str())
        } else {
            res
        }
    });

    (season_number, episode_numbers, quality)
}

pub async fn perform_enrichment_for_torrent(
    config: &crate::config::AppConfig,
    db: &Database,
    name: &str,
    hash: &str,
    node: &str,
    total_size: i64,
) -> Option<crate::db::ArrGrabRecord> {
    let now = chrono::Utc::now();
    let is_likely_series = LIKELY_SERIES_RE.is_match(name);
    let (parsed_season, parsed_episodes, parsed_quality) = parse_media_metadata_from_name(name);

    // ── Priority 1: Check Live Sonarr Queue & History ──
    if let Some(sonarr_match) = find_sonarr_queue_or_history(config, hash, name).await {
        let existing_ombi = db.find_ombi_request_by_title_or_ids(&sonarr_match.series_title, sonarr_match.imdb_id.as_deref(), None, sonarr_match.tvdb_id).ok().flatten();
        let maybe_post_id = existing_ombi.as_ref().and_then(|o| o.mattermost_post_id.clone());

        let final_quality = sonarr_match.quality.or(parsed_quality).unwrap_or_else(|| "HD".to_string());
        let final_season = sonarr_match.season_number.or(parsed_season);
        let final_episodes = sonarr_match.episode_numbers.or(parsed_episodes);

        let grab_record = crate::db::ArrGrabRecord {
            id: uuid::Uuid::new_v4().to_string(),
            scene_name: sonarr_match.release_title.clone(),
            release_title: sonarr_match.release_title.clone(),
            event_type: "Grab".to_string(),
            item_type: "series".to_string(),
            series_id: sonarr_match.series_id,
            movie_id: None,
            artist_id: None,
            album_id: None,
            zone_id: None,
            season_number: final_season,
            episode_numbers: final_episodes,
            episode_ids: None,
            indexer: Some(sonarr_match.indexer.clone()),
            download_client: Some(sonarr_match.download_client.unwrap_or_else(|| node.to_string())),
            download_id: Some(hash.to_string()),
            status: "fetched".to_string(),
            re_searched: false,
            re_search_count: 0,
            title: Some(sonarr_match.series_title.clone()),
            year: sonarr_match.year,
            overview: sonarr_match.overview.clone(),
            poster_url: sonarr_match.poster_url.clone(),
            genres: sonarr_match.genres.clone(),
            quality: Some(final_quality.clone()),
            size_bytes: sonarr_match.size_bytes.or(Some(total_size)),
            imdb_id: sonarr_match.imdb_id,
            tmdb_id: None,
            tvdb_id: sonarr_match.tvdb_id,
            runtime_mins: None,
            rating: None,
            mattermost_post_id: maybe_post_id.clone(),
            payload_json: sonarr_match.raw_json,
            created_at: now,
            updated_at: now,
        };

        let _ = db.save_arr_grab(&grab_record);
        let _ = db.log_event("pipeline", "info", &format!("🐕 Identified Sonarr Grab automation: '{}' -> '{}' ({})", name, sonarr_match.series_title, sonarr_match.indexer), None);

        // Dispatch Mattermost Living Card
        let config_clone = config.clone();
        let db_clone = db.clone();
        let title_fmt = format!("🐕 Conduit Grabbed • {} ({})", sonarr_match.series_title, sonarr_match.year.unwrap_or(0));
        let overview_fmt = sonarr_match.overview.unwrap_or_else(|| name.to_string());
        let node_fmt = node.to_string();
        let genres_fmt = sonarr_match.genres.unwrap_or_default();
        let hash_clone = hash.to_string();
        let name_clone = name.to_string();
        let indexer_clone = sonarr_match.indexer;
        let ep_display = match final_season {
            Some(s) => format!("Season {:02}", s),
            None => "Series".to_string(),
        };

        tokio::spawn(async move {
            let timeline_str = format!("📥 Grabbed: {}", now.format("%Y-%m-%d %H:%M:%S UTC"));
            let fields = vec![
                ("Quality", final_quality.as_str(), true),
                ("Indexer", indexer_clone.as_str(), true),
                ("Fetcher Node", node_fmt.as_str(), true),
                ("Episode", ep_display.as_str(), true),
                ("Genres", genres_fmt.as_str(), true),
                ("Pipeline Timeline", timeline_str.as_str(), false),
            ];
            let post_id_opt = crate::notify::NotificationManager::dispatch_rich_or_update(
                &config_clone.notifications,
                "sonarr.grab",
                None,
                maybe_post_id.as_deref(),
                None,
                &title_fmt,
                &overview_fmt,
                sonarr_match.poster_url.as_deref(),
                fields,
                Some("#10B981"),
            ).await;
            if let Some(pid) = post_id_opt {
                if let Ok(Some(mut g)) = db_clone.find_arr_grab_by_hash_or_name(&hash_clone, &name_clone) {
                    g.mattermost_post_id = Some(pid);
                    let _ = db_clone.save_arr_grab(&g);
                }
            }
        });

        return Some(grab_record);
    }

    // ── Priority 2: Check Live Radarr Queue & History ──
    if let Some(radarr_match) = find_radarr_queue_or_history(config, hash, name).await {
        let existing_ombi = db.find_ombi_request_by_title_or_ids(&radarr_match.movie_title, radarr_match.imdb_id.as_deref(), radarr_match.tmdb_id, None).ok().flatten();
        let maybe_post_id = existing_ombi.as_ref().and_then(|o| o.mattermost_post_id.clone());
        let final_quality = radarr_match.quality.or(parsed_quality).unwrap_or_else(|| "HD".to_string());

        let grab_record = crate::db::ArrGrabRecord {
            id: uuid::Uuid::new_v4().to_string(),
            scene_name: radarr_match.release_title.clone(),
            release_title: radarr_match.release_title.clone(),
            event_type: "Grab".to_string(),
            item_type: "movie".to_string(),
            series_id: None,
            movie_id: radarr_match.movie_id,
            artist_id: None,
            album_id: None,
            zone_id: None,
            season_number: None,
            episode_numbers: None,
            episode_ids: None,
            indexer: Some(radarr_match.indexer.clone()),
            download_client: Some(radarr_match.download_client.unwrap_or_else(|| node.to_string())),
            download_id: Some(hash.to_string()),
            status: "fetched".to_string(),
            re_searched: false,
            re_search_count: 0,
            title: Some(radarr_match.movie_title.clone()),
            year: radarr_match.year,
            overview: radarr_match.overview.clone(),
            poster_url: radarr_match.poster_url.clone(),
            genres: radarr_match.genres.clone(),
            quality: Some(final_quality.clone()),
            size_bytes: radarr_match.size_bytes.or(Some(total_size)),
            imdb_id: radarr_match.imdb_id,
            tmdb_id: radarr_match.tmdb_id,
            tvdb_id: None,
            runtime_mins: None,
            rating: None,
            mattermost_post_id: maybe_post_id.clone(),
            payload_json: radarr_match.raw_json,
            created_at: now,
            updated_at: now,
        };

        let _ = db.save_arr_grab(&grab_record);
        let _ = db.log_event("pipeline", "info", &format!("🐕 Identified Radarr Grab automation: '{}' -> '{}' ({})", name, radarr_match.movie_title, radarr_match.indexer), None);

        let config_clone = config.clone();
        let db_clone = db.clone();
        let title_fmt = format!("🐕 Conduit Grabbed • {} ({})", radarr_match.movie_title, radarr_match.year.unwrap_or(0));
        let overview_fmt = radarr_match.overview.unwrap_or_else(|| name.to_string());
        let node_fmt = node.to_string();
        let genres_fmt = radarr_match.genres.unwrap_or_default();
        let hash_clone = hash.to_string();
        let name_clone = name.to_string();
        let indexer_clone = radarr_match.indexer;

        tokio::spawn(async move {
            let timeline_str = format!("📥 Grabbed: {}", now.format("%Y-%m-%d %H:%M:%S UTC"));
            let fields = vec![
                ("Quality", final_quality.as_str(), true),
                ("Indexer", indexer_clone.as_str(), true),
                ("Fetcher Node", node_fmt.as_str(), true),
                ("Genres", genres_fmt.as_str(), true),
                ("Pipeline Timeline", timeline_str.as_str(), false),
            ];
            let post_id_opt = crate::notify::NotificationManager::dispatch_rich_or_update(
                &config_clone.notifications,
                "radarr.grab",
                None,
                maybe_post_id.as_deref(),
                None,
                &title_fmt,
                &overview_fmt,
                radarr_match.poster_url.as_deref(),
                fields,
                Some("#10B981"),
            ).await;
            if let Some(pid) = post_id_opt {
                if let Ok(Some(mut g)) = db_clone.find_arr_grab_by_hash_or_name(&hash_clone, &name_clone) {
                    g.mattermost_post_id = Some(pid);
                    let _ = db_clone.save_arr_grab(&g);
                }
            }
        });

        return Some(grab_record);
    }

    // ── Priority 3: Fallback Heuristic Enrichment (Lookup by Clean Title) ──
    let stripped = RELEASE_TAGS_RE.replace(name, "");
    let clean_title = stripped
        .replace(['.', '_', '-'], " ")
        .trim()
        .to_string();
    let query = if clean_title.is_empty() {
        name.split('.').next().unwrap_or(name).to_string()
    } else {
        clean_title
    };

    let inferred_indexer = infer_tracker_name_from_string(name, &config.queue_routing);

    if is_likely_series {
        if let Some(first) = lookup_sonarr(config, &query).await {
            let series_title = first.get("title").and_then(|v| v.as_str()).unwrap_or(&query);
            let year = first.get("year").and_then(|v| v.as_i64());
            let overview = first.get("overview").and_then(|v| v.as_str()).map(|s| s.to_string());
            let tvdb_id = first.get("tvdbId").and_then(|v| v.as_i64());
            let imdb_id = first.get("imdbId").and_then(|v| v.as_str()).map(|s| s.to_string());
            let genres = first.get("genres").and_then(|v| v.as_array()).map(|arr| {
                arr.iter().filter_map(|g| g.as_str()).collect::<Vec<_>>().join(", ")
            });
            let poster = first.get("images").and_then(|v| v.as_array()).and_then(|imgs| {
                imgs.iter().find(|img| img.get("coverType").and_then(|c| c.as_str()) == Some("poster"))
                    .and_then(|p| p.get("remoteUrl").or_else(|| p.get("url")))
                    .and_then(|u| u.as_str())
                    .map(|s| s.to_string())
            });

            let existing_ombi = db.find_ombi_request_by_title_or_ids(series_title, imdb_id.as_deref(), None, tvdb_id).ok().flatten();
            let maybe_post_id = existing_ombi.as_ref().and_then(|o| o.mattermost_post_id.clone());

            let grab_record = crate::db::ArrGrabRecord {
                id: uuid::Uuid::new_v4().to_string(),
                scene_name: name.to_string(),
                release_title: name.to_string(),
                event_type: "AutoEnrich".to_string(),
                item_type: "series".to_string(),
                series_id: first.get("id").and_then(|v| v.as_i64()),
                movie_id: None,
                artist_id: None,
                album_id: None,
                zone_id: None,
                season_number: parsed_season,
                episode_numbers: parsed_episodes.clone(),
                episode_ids: None,
                indexer: Some(inferred_indexer.clone()),
                download_client: Some(node.to_string()),
                download_id: Some(hash.to_string()),
                status: "fetched".to_string(),
                re_searched: false,
                re_search_count: 0,
                title: Some(series_title.to_string()),
                year,
                overview: overview.clone(),
                poster_url: poster.clone(),
                genres: genres.clone(),
                quality: parsed_quality.clone(),
                size_bytes: Some(total_size),
                imdb_id,
                tmdb_id: None,
                tvdb_id,
                runtime_mins: None,
                rating: None,
                mattermost_post_id: maybe_post_id.clone(),
                payload_json: first.to_string(),
                created_at: now,
                updated_at: now,
            };

            let _ = db.save_arr_grab(&grab_record);
            let _ = db.log_event("pipeline", "info", &format!("🐕 Auto-enriched intake download '{}' -> '{}' ({})", name, series_title, inferred_indexer), None);

            // Dispatch or evolve living Mattermost card
            let config_clone = config.clone();
            let db_clone = db.clone();
            let title_fmt = format!("🐕 Conduit Sniffed TV Kibble • {} ({})", series_title, year.unwrap_or(0));
            let overview_fmt = overview.unwrap_or_else(|| name.to_string());
            let node_fmt = node.to_string();
            let quality_fmt = parsed_quality.unwrap_or_else(|| "HD".to_string());
            let genres_fmt = genres.unwrap_or_default();
            let hash_clone = hash.to_string();
            let name_clone = name.to_string();
            let indexer_clone = inferred_indexer.clone();

            tokio::spawn(async move {
                let timeline_str = format!("📥 Intake Sniffed: {}", now.format("%Y-%m-%d %H:%M:%S UTC"));
                let fields = vec![
                    ("Quality", quality_fmt.as_str(), true),
                    ("Indexer", indexer_clone.as_str(), true),
                    ("Fetcher Node", node_fmt.as_str(), true),
                    ("Genres", genres_fmt.as_str(), true),
                    ("Pipeline Timeline", timeline_str.as_str(), false),
                ];
                let post_id_opt = crate::notify::NotificationManager::dispatch_rich_or_update(
                    &config_clone.notifications,
                    "sonarr.grab",
                    None,
                    maybe_post_id.as_deref(),
                    None,
                    &title_fmt,
                    &overview_fmt,
                    poster.as_deref(),
                    fields,
                    Some("#10B981"),
                ).await;
                if let Some(pid) = post_id_opt {
                    if let Ok(Some(mut g)) = db_clone.find_arr_grab_by_hash_or_name(&hash_clone, &name_clone) {
                        g.mattermost_post_id = Some(pid);
                        let _ = db_clone.save_arr_grab(&g);
                    }
                }
            });

            return Some(grab_record);
        }
    } else if let Some(first) = lookup_radarr(config, &query).await {
        let movie_title = first.get("title").and_then(|v| v.as_str()).unwrap_or(&query);
        let year = first.get("year").and_then(|v| v.as_i64());
        let overview = first.get("overview").and_then(|v| v.as_str()).map(|s| s.to_string());
        let tmdb_id = first.get("tmdbId").and_then(|v| v.as_i64());
        let imdb_id = first.get("imdbId").and_then(|v| v.as_str()).map(|s| s.to_string());
        let genres = first.get("genres").and_then(|v| v.as_array()).map(|arr| {
            arr.iter().filter_map(|g| g.as_str()).collect::<Vec<_>>().join(", ")
        });
        let poster = first.get("images").and_then(|v| v.as_array()).and_then(|imgs| {
            imgs.iter().find(|img| img.get("coverType").and_then(|c| c.as_str()) == Some("poster"))
                .and_then(|p| p.get("remoteUrl").or_else(|| p.get("url")))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string())
        });

        let existing_ombi = db.find_ombi_request_by_title_or_ids(movie_title, imdb_id.as_deref(), tmdb_id, None).ok().flatten();
        let maybe_post_id = existing_ombi.as_ref().and_then(|o| o.mattermost_post_id.clone());

        let grab_record = crate::db::ArrGrabRecord {
            id: uuid::Uuid::new_v4().to_string(),
            scene_name: name.to_string(),
            release_title: name.to_string(),
            event_type: "AutoEnrich".to_string(),
            item_type: "movie".to_string(),
            series_id: None,
            movie_id: first.get("id").and_then(|v| v.as_i64()),
            artist_id: None,
            album_id: None,
            zone_id: None,
            season_number: None,
            episode_numbers: None,
            episode_ids: None,
            indexer: Some(inferred_indexer.clone()),
            download_client: Some(node.to_string()),
            download_id: Some(hash.to_string()),
            status: "fetched".to_string(),
            re_searched: false,
            re_search_count: 0,
            title: Some(movie_title.to_string()),
            year,
            overview: overview.clone(),
            poster_url: poster.clone(),
            genres: genres.clone(),
            quality: parsed_quality.clone(),
            size_bytes: Some(total_size),
            imdb_id,
            tmdb_id,
            tvdb_id: None,
            runtime_mins: first.get("runtime").and_then(|v| v.as_i64()),
            rating: None,
            mattermost_post_id: maybe_post_id.clone(),
            payload_json: first.to_string(),
            created_at: now,
            updated_at: now,
        };

        let _ = db.save_arr_grab(&grab_record);
        let _ = db.log_event("pipeline", "info", &format!("🐕 Auto-enriched intake download '{}' -> '{}' ({})", name, movie_title, inferred_indexer), None);

        let config_clone = config.clone();
        let db_clone = db.clone();
        let title_fmt = format!("🐕 Conduit Sniffed Movie Kibble • {} ({})", movie_title, year.unwrap_or(0));
        let overview_fmt = overview.unwrap_or_else(|| name.to_string());
        let node_fmt = node.to_string();
        let quality_fmt = parsed_quality.unwrap_or_else(|| "HD".to_string());
        let genres_fmt = genres.unwrap_or_default();
        let hash_clone = hash.to_string();
        let name_clone = name.to_string();
        let indexer_clone = inferred_indexer.clone();

        tokio::spawn(async move {
            let timeline_str = format!("📥 Intake Sniffed: {}", now.format("%Y-%m-%d %H:%M:%S UTC"));
            let fields = vec![
                ("Quality", quality_fmt.as_str(), true),
                ("Indexer", indexer_clone.as_str(), true),
                ("Fetcher Node", node_fmt.as_str(), true),
                ("Genres", genres_fmt.as_str(), true),
                ("Pipeline Timeline", timeline_str.as_str(), false),
            ];
            let post_id_opt = crate::notify::NotificationManager::dispatch_rich_or_update(
                &config_clone.notifications,
                "radarr.grab",
                None,
                maybe_post_id.as_deref(),
                None,
                &title_fmt,
                &overview_fmt,
                poster.as_deref(),
                fields,
                Some("#10B981"),
            ).await;
            if let Some(pid) = post_id_opt {
                if let Ok(Some(mut g)) = db_clone.find_arr_grab_by_hash_or_name(&hash_clone, &name_clone) {
                    g.mattermost_post_id = Some(pid);
                    let _ = db_clone.save_arr_grab(&g);
                }
            }
        });

        return Some(grab_record);
    } else if let Some(first) = lookup_lidarr(config, &query).await {
        // Lidarr's `/api/v1/search` returns a `SearchResource` per hit: `{ id, foreignId, artist,
        // album }` where `id` is a synthetic 1-based list-index counter (NOT the real Lidarr
        // artist/album ID) and the actual data lives nested under `album` (with its own nested
        // `artist`) or `artist` alone — never at the top level.
        let album_obj = first.get("album").filter(|a| !a.is_null());
        let artist_obj = album_obj
            .and_then(|a| a.get("artist"))
            .filter(|a| !a.is_null())
            .or_else(|| first.get("artist").filter(|a| !a.is_null()));

        let artist_name = artist_obj.and_then(|a| a.get("artistName")).and_then(|v| v.as_str()).unwrap_or(&query);
        let album_title = album_obj.and_then(|a| a.get("title")).and_then(|v| v.as_str()).unwrap_or("");
        let title_display = if album_title.is_empty() { artist_name.to_string() } else { format!("{} - {}", artist_name, album_title) };
        let overview = album_obj.or(artist_obj).and_then(|o| o.get("overview")).and_then(|v| v.as_str()).map(|s| s.to_string());
        let poster = album_obj.or(artist_obj).and_then(|o| o.get("images")).and_then(|v| v.as_array()).and_then(|imgs| {
            imgs.iter().find(|img| img.get("coverType").and_then(|c| c.as_str()) == Some("cover") || img.get("coverType").and_then(|c| c.as_str()) == Some("poster"))
                .and_then(|p| p.get("remoteUrl").or_else(|| p.get("url")))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string())
        });
        let lidarr_album_id = album_obj.and_then(|a| a.get("id")).and_then(|v| v.as_i64());
        let lidarr_artist_id = album_obj.and_then(|a| a.get("artistId")).and_then(|v| v.as_i64())
            .or_else(|| artist_obj.and_then(|a| a.get("id")).and_then(|v| v.as_i64()));

        let grab_record = crate::db::ArrGrabRecord {
            id: uuid::Uuid::new_v4().to_string(),
            scene_name: name.to_string(),
            release_title: name.to_string(),
            event_type: "AutoEnrich".to_string(),
            item_type: "music".to_string(),
            series_id: None,
            movie_id: None,
            artist_id: lidarr_artist_id,
            album_id: lidarr_album_id,
            zone_id: None,
            season_number: None,
            episode_numbers: None,
            episode_ids: None,
            indexer: Some(inferred_indexer.clone()),
            download_client: Some(node.to_string()),
            download_id: Some(hash.to_string()),
            status: "fetched".to_string(),
            re_searched: false,
            re_search_count: 0,
            title: Some(title_display.clone()),
            year: None,
            overview: overview.clone(),
            poster_url: poster.clone(),
            genres: None,
            quality: parsed_quality.clone(),
            size_bytes: Some(total_size),
            imdb_id: None,
            tmdb_id: None,
            tvdb_id: None,
            runtime_mins: None,
            rating: None,
            mattermost_post_id: None,
            payload_json: first.to_string(),
            created_at: now,
            updated_at: now,
        };

        let _ = db.save_arr_grab(&grab_record);
        let _ = db.log_event("pipeline", "info", &format!("🐕 Auto-enriched intake download '{}' -> '{}' ({})", name, title_display, inferred_indexer), None);

        let config_clone = config.clone();
        let db_clone = db.clone();
        let title_fmt = format!("🐕 Conduit Sniffed Music Kibble • {}", title_display);
        let overview_fmt = overview.unwrap_or_else(|| name.to_string());
        let node_fmt = node.to_string();
        let quality_fmt = parsed_quality.unwrap_or_else(|| "Lossless".to_string());
        let hash_clone = hash.to_string();
        let name_clone = name.to_string();
        let indexer_clone = inferred_indexer.clone();

        tokio::spawn(async move {
            let timeline_str = format!("📥 Intake Sniffed: {}", now.format("%Y-%m-%d %H:%M:%S UTC"));
            let fields = vec![
                ("Quality", quality_fmt.as_str(), true),
                ("Indexer", indexer_clone.as_str(), true),
                ("Fetcher Node", node_fmt.as_str(), true),
                ("Pipeline Timeline", timeline_str.as_str(), false),
            ];
            let post_id_opt = crate::notify::NotificationManager::dispatch_rich_or_update(
                &config_clone.notifications,
                "lidarr.grab",
                None,
                None,
                None,
                &title_fmt,
                &overview_fmt,
                poster.as_deref(),
                fields,
                Some("#10B981"),
            ).await;
            if let Some(pid) = post_id_opt {
                if let Ok(Some(mut g)) = db_clone.find_arr_grab_by_hash_or_name(&hash_clone, &name_clone) {
                    g.mattermost_post_id = Some(pid);
                    let _ = db_clone.save_arr_grab(&g);
                }
            }
        });

        return Some(grab_record);
    }

    None
}

#[utoipa::path(
    post,
    path = "/api/torrents/{compound_id}/enrich",
    tag = "Torrents",
    summary = "Manually trigger Arr metadata enrichment for a torrent",
    description = "Queries configured primary Sonarr/Radarr instances for movie/series match and correlates rich poster, genres, and identifiers.",
    params(
        ("compound_id" = String, Path, description = "Compound ID (node:id)")
    ),
    responses(
        (status = 200, description = "Torrent enriched with rich metadata", body = DetailedTorrent),
        (status = 404, description = "Torrent not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn enrich_torrent(
    auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    State(db): State<Database>,
    State(config_mgr): State<crate::config::ConfigManager>,
    State(ip_asn_cache): State<crate::ip_asn::IpAsnCache>,
    Path(compound_id): Path<String>,
) -> Result<Json<DetailedTorrent>, StatusCode> {
    let detailed = get_torrent(auth.clone(), State(pool.clone()), State(db.clone()), State(ip_asn_cache), Path(compound_id.clone())).await?;
    let mut torrent = detailed.0;

    if torrent.arr_grab.is_some() {
        return Ok(Json(torrent));
    }

    let config = config_mgr.get().await;
    if let Some(grab) = perform_enrichment_for_torrent(
        &config,
        &db,
        &torrent.unified.name,
        &torrent.unified.hash_string,
        &torrent.unified.node,
        torrent.unified.total_size,
    ).await {
        torrent.arr_grab = Some(grab);
    }

    Ok(Json(torrent))
}

// Manual "enrich all" is bounded the same way the automatic background pass already is
// (see the throttling added in 0536e8a): a concurrency cap so we don't fire hundreds of
// simultaneous outbound Sonarr/Radarr/Lidarr requests, and a per-call item cap so a very
// large un-enriched backlog is worked off over several calls instead of one huge burst.
const ENRICH_ALL_MAX_CONCURRENCY: usize = 5;
const ENRICH_ALL_MAX_ITEMS_PER_CALL: usize = 100;

#[utoipa::path(
    post,
    path = "/api/torrents/enrich-all",
    tag = "Torrents",
    summary = "Batch enrich all un-enriched torrents from Sonarr/Radarr",
    description = "Scans all active torrents across all connected nodes that lack Arr metadata and queries configured Sonarr/Radarr instances to populate rich metadata. Processes at most 100 un-enriched torrents per call with bounded concurrency; call again to continue working through a larger backlog.",
    responses(
        (status = 200, description = "Batch enrichment summary", body = serde_json::Value),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn enrich_all_torrents(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    State(db): State<Database>,
    State(config_mgr): State<crate::config::ConfigManager>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let torrents = pool.get_torrents(None, None, None);
    let config = std::sync::Arc::new(config_mgr.get().await);
    let grab_map = db.get_arr_grabs_lookup_map();
    let total_scanned = torrents.len();

    let unenriched: Vec<_> = torrents
        .into_iter()
        .filter(|t| {
            let hash_lower = t.hash_string.to_lowercase();
            let name_lower = t.name.to_lowercase();
            grab_map.get(&hash_lower).or_else(|| grab_map.get(&name_lower)).is_none()
        })
        .collect();
    let total_unenriched = unenriched.len();
    let candidates: Vec<_> = unenriched.into_iter().take(ENRICH_ALL_MAX_ITEMS_PER_CALL).collect();
    let processed_count = candidates.len();

    let enriched_count = futures_util::stream::iter(candidates)
        .map(|t| {
            let config = config.clone();
            let db = db.clone();
            async move {
                perform_enrichment_for_torrent(&config, &db, &t.name, &t.hash_string, &t.node, t.total_size)
                    .await
                    .is_some()
            }
        })
        .buffer_unordered(ENRICH_ALL_MAX_CONCURRENCY)
        .fold(0usize, |acc, enriched| async move { if enriched { acc + 1 } else { acc } })
        .await;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "total_scanned": total_scanned,
        "processed_this_call": processed_count,
        "enriched_count": enriched_count,
        "remaining_unenriched": total_unenriched.saturating_sub(processed_count)
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct QueueMovePayload {
    /// "top", "up", "down", or "bottom"
    pub direction: String,
}

#[utoipa::path(
    post,
    path = "/api/torrents/{compound_id}/queue-move",
    tag = "Torrents",
    summary = "Move torrent position in fetcher download queue",
    params(
        ("compound_id" = String, Path, description = "Target torrent compound ID (node:id)")
    ),
    request_body = QueueMovePayload,
    responses(
        (status = 200, description = "Queue position moved"),
        (status = 404, description = "Torrent not found"),
        (status = 501, description = "This node's backend cannot reorder its queue")
    )
)]
pub async fn queue_move_torrent(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Path(compound_id): Path<String>,
    Json(payload): Json<QueueMovePayload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (node, id_str) = compound_id.split_once(':').ok_or_else(|| status_only(StatusCode::BAD_REQUEST))?;
    let id: i64 = id_str.parse().map_err(|_| status_only(StatusCode::BAD_REQUEST))?;

    let client = pool.get_client(node).ok_or_else(|| status_only(StatusCode::NOT_FOUND))?;
    client.queue_move(&[id], &payload.direction).await.map_err(|e| op_failure(&e))?;

    Ok(Json(json!({
        "compound_id": compound_id,
        "direction": payload.direction,
        "status": "success"
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BulkQueueMovePayload {
    pub compound_ids: Vec<String>,
    /// "top", "up", "down", or "bottom"
    pub direction: String,
}

#[utoipa::path(
    post,
    path = "/api/torrents/queue-move",
    tag = "Torrents",
    summary = "Bulk move torrent queue positions",
    request_body = BulkQueueMovePayload,
    responses(
        (status = 200, description = "Bulk queue movement executed"),
        (status = 501, description = "None of the selected nodes' backends can reorder their queue")
    )
)]
pub async fn queue_move_bulk(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Json(payload): Json<BulkQueueMovePayload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut node_groups: HashMap<String, Vec<i64>> = HashMap::new();
    for cid in &payload.compound_ids {
        if let Some((node, id_str)) = cid.split_once(':') {
            if let Ok(id) = id_str.parse::<i64>() {
                node_groups.entry(node.to_string()).or_default().push(id);
            }
        }
    }

    let mut moved_count = 0usize;
    let mut unsupported_nodes: Vec<String> = Vec::new();
    let mut failed_nodes: Vec<Value> = Vec::new();
    for (node, ids) in node_groups {
        if let Some(client) = pool.get_client(&node) {
            match client.queue_move(&ids, &payload.direction).await {
                Ok(()) => moved_count += ids.len(),
                Err(e) if is_unsupported(&e) => unsupported_nodes.push(node),
                Err(e) => failed_nodes.push(json!({ "node": node, "error": e.to_string() })),
            }
        }
    }

    // Nothing moved only because no selected node can: say so rather than report success.
    if moved_count == 0 && failed_nodes.is_empty() && !unsupported_nodes.is_empty() {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "error": "None of the selected nodes' backends can reorder their queue",
                "unsupported": true,
                "unsupported_nodes": unsupported_nodes,
            })),
        ));
    }

    Ok(Json(json!({
        "moved_torrents": moved_count,
        "direction": payload.direction,
        "unsupported_nodes": unsupported_nodes,
        "failed_nodes": failed_nodes,
        "status": if failed_nodes.is_empty() && unsupported_nodes.is_empty() { "success" } else { "partial" }
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SequentialDownloadPayload {
    pub enabled: bool,
}

#[utoipa::path(
    post,
    path = "/api/torrents/{compound_id}/sequential-download",
    tag = "Torrents",
    summary = "Toggle sequential downloading for a torrent",
    params(
        ("compound_id" = String, Path, description = "Target torrent compound ID (node:id)")
    ),
    request_body = SequentialDownloadPayload,
    responses(
        (status = 200, description = "Sequential download state updated"),
        (status = 404, description = "Torrent not found"),
        (status = 501, description = "This node's backend has no sequential download")
    )
)]
pub async fn set_sequential_download(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Path(compound_id): Path<String>,
    Json(payload): Json<SequentialDownloadPayload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (node, id_str) = compound_id.split_once(':').ok_or_else(|| status_only(StatusCode::BAD_REQUEST))?;
    let id: i64 = id_str.parse().map_err(|_| status_only(StatusCode::BAD_REQUEST))?;

    let client = pool.get_client(node).ok_or_else(|| status_only(StatusCode::NOT_FOUND))?;
    client.set_sequential_download(&[id], payload.enabled).await.map_err(|e| op_failure(&e))?;

    Ok(Json(json!({
        "compound_id": compound_id,
        "sequential_download": payload.enabled,
        "status": "success"
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RenamePathPayload {
    pub path: String,
    pub new_name: String,
}

#[utoipa::path(
    post,
    path = "/api/torrents/{compound_id}/rename-path",
    tag = "Torrents",
    summary = "Rename a file or folder within an active torrent",
    params(
        ("compound_id" = String, Path, description = "Target torrent compound ID (node:id)")
    ),
    request_body = RenamePathPayload,
    responses(
        (status = 200, description = "Path renamed successfully"),
        (status = 404, description = "Torrent not found"),
        (status = 501, description = "This node's backend cannot rename paths")
    )
)]
pub async fn rename_torrent_path(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Path(compound_id): Path<String>,
    Json(payload): Json<RenamePathPayload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (node, id_str) = compound_id.split_once(':').ok_or_else(|| status_only(StatusCode::BAD_REQUEST))?;
    let id: i64 = id_str.parse().map_err(|_| status_only(StatusCode::BAD_REQUEST))?;

    let client = pool.get_client(node).ok_or_else(|| status_only(StatusCode::NOT_FOUND))?;
    let res = client.rename_path(id, &payload.path, &payload.new_name).await
        .map_err(|e| op_failure(&e))?;

    Ok(Json(json!({
        "compound_id": compound_id,
        "path": payload.path,
        "new_name": payload.new_name,
        "result": res
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchReplaceTrackersPayload {
    pub node: Option<String>,
    pub compound_ids: Option<Vec<String>>,
    pub old_url: String,
    pub new_url: String,
}

#[utoipa::path(
    post,
    path = "/api/torrents/batch-replace-trackers",
    tag = "Torrents",
    summary = "Search and replace tracker announce URLs across torrents",
    request_body = BatchReplaceTrackersPayload,
    responses(
        (status = 200, description = "Replacement summary"),
        (status = 501, description = "The matching torrents' backends cannot change trackers")
    )
)]
pub async fn batch_replace_trackers(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Json(payload): Json<BatchReplaceTrackersPayload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let all_torrents = pool.get_torrents(payload.node.as_deref(), None, None);
    let target_cids = payload.compound_ids.as_ref();

    let old_lower = payload.old_url.trim().to_lowercase();
    let new_val = payload.new_url.trim();

    let mut replaced_torrents = 0usize;
    let mut unsupported_nodes: Vec<String> = Vec::new();
    let mut failed = 0usize;

    for t in all_torrents {
        if let Some(cids) = target_cids {
            if !cids.contains(&t.compound_id) {
                continue;
            }
        }
        if unsupported_nodes.contains(&t.node) {
            continue;
        }

        // Some backends (Synapse) leave a torrent's trackers out of the list view; ask for
        // them so the torrent can still be matched.
        let mut tracker_stats = t.tracker_stats.clone();
        if tracker_stats.is_empty() {
            if let Some(client) = pool.get_client(&t.node) {
                if let Ok(detail) = client.get_torrent_details(t.id).await {
                    tracker_stats = detail.tracker_stats;
                }
            }
        }

        // Check if any tracker matches old_url
        let mut matched = false;
        let mut new_tracker_lines: Vec<String> = Vec::new();

        for ts in &tracker_stats {
            let announce = &ts.announce;
            if announce.to_lowercase().contains(&old_lower) {
                matched = true;
                let replaced = announce.replace(&payload.old_url, new_val);
                new_tracker_lines.push(replaced);
            } else {
                new_tracker_lines.push(announce.clone());
            }
        }

        if matched {
            if let Some(client) = pool.get_client(&t.node) {
                let tracker_list = new_tracker_lines.join("\n\n");
                match client.replace_trackers(t.id, &tracker_list, &payload.old_url, new_val).await {
                    Ok(()) => replaced_torrents += 1,
                    Err(e) if is_unsupported(&e) => unsupported_nodes.push(t.node.clone()),
                    Err(_) => failed += 1,
                }
            }
        }
    }

    if replaced_torrents == 0 && failed == 0 && !unsupported_nodes.is_empty() {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "error": "The matching torrents are on nodes whose backend cannot change trackers",
                "unsupported": true,
                "unsupported_nodes": unsupported_nodes,
            })),
        ));
    }

    Ok(Json(json!({
        "status": if failed == 0 && unsupported_nodes.is_empty() { "success" } else { "partial" },
        "replaced_torrents": replaced_torrents,
        "failed_torrents": failed,
        "unsupported_nodes": unsupported_nodes,
        "old_url": payload.old_url,
        "new_url": payload.new_url
    })))
}
