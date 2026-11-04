// src/stats.rs
//! Aggregated cluster statistics for the Stats page: totals and breakdowns by fetcher node,
//! tracker, and media type, plus a separate, far-less-frequent breakdown by remote peer
//! client. `compute_stats_snapshot` is cheap -- it only reuses data the regular poll cycle
//! already pulled in (see `engines::poller`) -- so it's used both for the live
//! `/api/stats/current` endpoint and, on a timer, by `engines::stats_history` to persist a
//! point into `Database::stats_history`. `sample_peer_clients` is the one expensive path
//! here: the regular poll never requests peer lists (see
//! `fetcher_transmission::TransmissionClient::list_fields`), so a full breakdown would mean
//! one extra RPC per torrent. It's bounded to the most active torrents per node and run on a
//! much slower cadence.

use crate::db::Database;
use crate::fetcher::{FetcherPool, UnifiedTorrent};
use fetcher_core::RetrieverClientType;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use utoipa::ToSchema;

/// How many of a node's most active (by connected-peer count) torrents `sample_peer_clients`
/// fetches full peer lists for. Bounds the cost at `max_per_node * connected_nodes` detail
/// RPCs per call regardless of library size.
pub const DEFAULT_PEER_CLIENT_SAMPLE_SIZE: usize = 15;
const PEER_CLIENT_SAMPLE_CONCURRENCY: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeBreakdown {
    pub node: String,
    pub client_type: RetrieverClientType,
    pub connected: bool,
    pub torrents: usize,
    pub size_bytes: i64,
    pub rate_download: i64,
    pub rate_upload: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrackerBreakdown {
    pub host: String,
    pub torrents: usize,
    pub seeders: i64,
    pub leechers: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MediaTypeBreakdown {
    /// `"tv"` | `"movie"` | `"music"` | `"unknown"` -- see `normalize_media_type`.
    pub media_type: String,
    pub torrents: usize,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PeerClientBreakdown {
    pub client_name: String,
    pub peer_count: usize,
    pub torrents_seen_on: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StatsSnapshot {
    pub time: i64,
    pub total_torrents: usize,
    pub total_size_bytes: i64,
    /// Sum, across torrents, of each torrent's *best* non-backup tracker's seeder count --
    /// not a sum across all of a torrent's trackers, which would multiply-count the same
    /// swarm for every tracker it's announced to. See `by_tracker` for the per-host view.
    pub total_seeders: i64,
    pub total_leechers: i64,
    pub total_download_speed: i64,
    pub total_upload_speed: i64,
    pub by_node: Vec<NodeBreakdown>,
    pub by_tracker: Vec<TrackerBreakdown>,
    pub by_media_type: Vec<MediaTypeBreakdown>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PeerClientSnapshot {
    pub time: i64,
    pub by_client: Vec<PeerClientBreakdown>,
}

/// `arr_grabs.item_type` has accumulated a few spellings over time ("series", "tv", "movie",
/// "music", and blank -- see the healing `UPDATE` statements in `Database::init`). Collapses
/// them to the four buckets the Stats page shows; anything unmatched (including torrents with
/// no `arr_grab` at all, e.g. added outside Sonarr/Radarr/Lidarr) is "unknown" rather than
/// silently dropped, since those torrents still count toward totals.
fn normalize_media_type(item_type: Option<&str>) -> &'static str {
    match item_type.unwrap_or("").to_lowercase().as_str() {
        "series" | "tv" => "tv",
        "movie" => "movie",
        "music" => "music",
        _ => "unknown",
    }
}

/// Builds one aggregate snapshot from whatever the pool already has cached -- no network
/// calls, same cost class as `engines::telemetry::compute_snapshot`, which this mirrors
/// (including the hash/name join against `arr_grabs` for media-type classification).
pub fn compute_stats_snapshot(pool: &FetcherPool, db: &Database) -> StatsSnapshot {
    let torrents = pool.get_torrents(None, None, None);
    let grab_map = db.get_arr_grabs_lookup_map();
    let agg = pool.get_aggregate_stats();

    let mut by_node: HashMap<String, NodeBreakdown> = HashMap::new();
    for n in &agg.nodes {
        by_node.insert(n.node.clone(), NodeBreakdown {
            node: n.node.clone(),
            client_type: n.client_type,
            connected: n.connected,
            torrents: 0,
            size_bytes: 0,
            rate_download: n.rate_download,
            rate_upload: n.rate_upload,
        });
    }

    let mut by_tracker: HashMap<String, TrackerBreakdown> = HashMap::new();
    let mut by_media: HashMap<&'static str, MediaTypeBreakdown> = HashMap::new();
    let mut total_seeders = 0i64;
    let mut total_leechers = 0i64;

    for t in &torrents {
        let node_entry = by_node.entry(t.node.clone()).or_insert_with(|| NodeBreakdown {
            node: t.node.clone(),
            client_type: RetrieverClientType::default(),
            connected: false,
            torrents: 0,
            size_bytes: 0,
            rate_download: 0,
            rate_upload: 0,
        });
        node_entry.torrents += 1;
        node_entry.size_bytes += t.total_size;

        // Backup trackers (`is_backup`) aren't actively announced to -- their counts would be
        // stale/irrelevant for both the per-host breakdown and the global totals below.
        let active_trackers: Vec<_> = t.tracker_stats.iter().filter(|ts| !ts.is_backup).collect();

        let mut seen_hosts: HashSet<&str> = HashSet::new();
        for ts in &active_trackers {
            let tracker_entry = by_tracker.entry(ts.host.clone()).or_insert_with(|| TrackerBreakdown {
                host: ts.host.clone(),
                torrents: 0,
                seeders: 0,
                leechers: 0,
            });
            tracker_entry.seeders += ts.seeder_count.max(0);
            tracker_entry.leechers += ts.leecher_count.max(0);
            if seen_hosts.insert(ts.host.as_str()) {
                tracker_entry.torrents += 1;
            }
        }
        // For the global totals, take this torrent's single best-reporting tracker rather than
        // summing across all of them -- a torrent announced to 3 trackers isn't 3x the seeders.
        if let Some(best) = active_trackers.iter().max_by_key(|ts| ts.seeder_count) {
            total_seeders += best.seeder_count.max(0);
            total_leechers += best.leecher_count.max(0);
        }

        let grab = Database::match_grab_in_map(&grab_map, &t.hash_string, &t.name);
        let media_type = normalize_media_type(grab.map(|g| g.item_type.as_str()));
        let media_entry = by_media.entry(media_type).or_insert_with(|| MediaTypeBreakdown {
            media_type: media_type.to_string(),
            torrents: 0,
            size_bytes: 0,
        });
        media_entry.torrents += 1;
        media_entry.size_bytes += t.total_size;
    }

    let mut by_node: Vec<NodeBreakdown> = by_node.into_values().collect();
    by_node.sort_by(|a, b| a.node.cmp(&b.node));
    let mut by_tracker: Vec<TrackerBreakdown> = by_tracker.into_values().collect();
    by_tracker.sort_by(|a, b| b.torrents.cmp(&a.torrents).then_with(|| a.host.cmp(&b.host)));
    let mut by_media_type: Vec<MediaTypeBreakdown> = by_media.into_values().collect();
    by_media_type.sort_by(|a, b| b.torrents.cmp(&a.torrents).then_with(|| a.media_type.cmp(&b.media_type)));

    StatsSnapshot {
        time: chrono::Utc::now().timestamp_millis(),
        total_torrents: agg.total_torrents,
        total_size_bytes: agg.total_size_bytes,
        total_seeders,
        total_leechers,
        total_download_speed: agg.total_download_speed,
        total_upload_speed: agg.total_upload_speed,
        by_node,
        by_tracker,
        by_media_type,
    }
}

/// Transmission/qBittorrent/Deluge/Synapse all report a free-form peer `client_name` like
/// `"qBittorrent/4.6.0"` or `"Transmission 4.0.5 (abcdef)"` -- strips the version/build suffix
/// so the breakdown groups by client family instead of fragmenting across every point release.
fn normalize_client_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "Unknown".to_string();
    }
    let cut = trimmed.find(|c: char| c.is_ascii_digit() || c == '/').unwrap_or(trimmed.len());
    let name = trimmed[..cut].trim();
    if name.is_empty() { trimmed.to_string() } else { name.to_string() }
}

/// Samples remote peer client names across the most active torrents on each connected node.
/// Bounded to `max_per_node` torrents per node (by `peers_connected` descending) -- see the
/// module doc comment for why this can't just be folded into `compute_stats_snapshot`.
pub async fn sample_peer_clients(pool: &FetcherPool, max_per_node: usize) -> PeerClientSnapshot {
    let torrents = pool.get_torrents(None, None, None);
    let mut by_node: HashMap<String, Vec<&UnifiedTorrent>> = HashMap::new();
    for t in &torrents {
        by_node.entry(t.node.clone()).or_default().push(t);
    }

    let mut targets: Vec<(String, i64)> = Vec::new();
    for (node, mut node_torrents) in by_node {
        node_torrents.sort_by_key(|t| std::cmp::Reverse(t.peers_connected));
        for t in node_torrents.into_iter().take(max_per_node) {
            if t.peers_connected > 0 {
                targets.push((node.clone(), t.id));
            }
        }
    }

    let per_torrent_names: Vec<Vec<String>> = futures_util::stream::iter(targets)
        .map(|(node, id)| {
            let client = pool.get_client(&node);
            async move {
                let Some(client) = client else {
                    return Vec::new();
                };
                match client.get_torrent_details(id).await {
                    Ok(t) => t
                        .peers
                        .unwrap_or_default()
                        .into_iter()
                        .map(|p| normalize_client_name(&p.client_name))
                        .collect(),
                    Err(_) => Vec::new(),
                }
            }
        })
        .buffer_unordered(PEER_CLIENT_SAMPLE_CONCURRENCY)
        .collect()
        .await;

    let mut peer_counts: HashMap<String, usize> = HashMap::new();
    let mut torrents_seen: HashMap<String, usize> = HashMap::new();
    for names in per_torrent_names {
        let mut unique_this_torrent: HashSet<String> = HashSet::new();
        for name in names {
            *peer_counts.entry(name.clone()).or_insert(0) += 1;
            unique_this_torrent.insert(name);
        }
        for name in unique_this_torrent {
            *torrents_seen.entry(name).or_insert(0) += 1;
        }
    }

    let mut by_client: Vec<PeerClientBreakdown> = peer_counts
        .into_iter()
        .map(|(client_name, peer_count)| {
            let torrents_seen_on = *torrents_seen.get(&client_name).unwrap_or(&0);
            PeerClientBreakdown { client_name, peer_count, torrents_seen_on }
        })
        .collect();
    by_client.sort_by(|a, b| b.peer_count.cmp(&a.peer_count).then_with(|| a.client_name.cmp(&b.client_name)));

    PeerClientSnapshot { time: chrono::Utc::now().timestamp_millis(), by_client }
}
