// src/engines/telemetry.rs
//! Computes the always-on cluster telemetry snapshot (aggregate stats + grab-enriched torrent
//! list) once per tick and publishes it on the `telemetry` WebSocket topic.
//!
//! Previously every connected WebSocket client ran its own 1s timer and recomputed this
//! snapshot independently — cloning the full torrent list and re-running the grab-map
//! hash/name/stem matching loop over every torrent, once per connection, for an identical
//! result each time. With N connected clients (browser tabs, the mobile app, a second admin)
//! and M torrents that was O(N·M) work per second for data that's the same for everyone. This
//! engine computes it exactly once; `api::ws::handle_socket` just forwards the published event
//! to each connection unconditionally (see `events::TOPIC_TELEMETRY`).

use super::registry::TaskHandle;
use crate::db::Database;
use crate::events::{self, EventBus, TOPIC_TELEMETRY};
use crate::fetcher::FetcherPool;
use std::sync::Arc;
use std::time::Duration;

/// Builds one telemetry snapshot. Pulled out as its own function so `api::ws::handle_socket`
/// can call it once on connect (to send an immediate first frame, matching the old
/// per-connection-timer behavior of not making a fresh client wait up to 1s for its first
/// update) without duplicating the enrichment logic or reintroducing the per-tick recompute
/// this engine exists to eliminate — the recurring 1s publish below is the only steady-state
/// caller.
pub fn compute_snapshot(pool: &FetcherPool, db: &Database) -> serde_json::Value {
    let stats = pool.get_aggregate_stats();
    let mut torrents = pool.get_torrents(None, None, None);

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
        if let Some(grab) = grab_map
            .get(&hash_lower)
            .or_else(|| grab_map.get(&name_lower))
            .or_else(|| grab_map.get(&stem_lower))
        {
            t.arr_grab = Some(grab.clone());
        }
    }

    serde_json::json!({
        "type": "telemetry",
        "stats": stats,
        "torrents": torrents,
    })
}

pub async fn run_telemetry_publisher(pool: Arc<FetcherPool>, db: Database, bus: EventBus, handle: TaskHandle) {
    loop {
        let snapshot = compute_snapshot(&pool, &db);
        events::publish(&bus, TOPIC_TELEMETRY, &snapshot);
        super::registry::heartbeat_sleep(&handle, Duration::from_secs(1)).await;
    }
}
