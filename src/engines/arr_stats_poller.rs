// src/engines/arr_stats_poller.rs
use crate::api::arr_routes::{compute_arr_stats, compute_zone_arr_stats, ArrStatsResponse};
use crate::config::ConfigManager;
use crate::db::Database;
use crate::engines::registry::TaskHandle;
use crate::events::EventBus;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

/// `global` mirrors the legacy single-instance stats (`config.sonarr.primary` etc.) exactly as
/// before zones existed. `zones` holds one additional entry per configured zone, keyed by
/// `ZoneConfig.id`, computed against that zone's own Sonarr/Radarr/Lidarr instance and grabs
/// tagged with that `zone_id` — empty when no zones are configured.
#[derive(Debug, Clone, Default)]
pub struct ArrStatsSnapshot {
    pub global: Option<ArrStatsResponse>,
    pub zones: HashMap<String, ArrStatsResponse>,
}

/// Shared cache read instantly by `GET /api/arr/stats` and written by this poller — replaces
/// making ~15 sequential live HTTP calls to Sonarr/Radarr/Lidarr on every request from the
/// dashboard, which was the source of multi-second load times on that endpoint.
pub type ArrStatsCache = Arc<RwLock<ArrStatsSnapshot>>;

pub fn new_cache() -> ArrStatsCache {
    Arc::new(RwLock::new(ArrStatsSnapshot::default()))
}

const POLL_INTERVAL_SECS: u64 = 30;

pub async fn run_arr_stats_poller_loop(config_mgr: ConfigManager, db: Database, cache: ArrStatsCache, bus: EventBus, handle: TaskHandle) {
    info!("Starting Arr Ecosystem Stats Poller (refreshing every {}s)", POLL_INTERVAL_SECS);
    let _ = db.log_event("arr_stats_poller", "info", "🐕 Arr Ecosystem Stats Poller started", None);

    loop {
        let stats = compute_arr_stats(&config_mgr, &db).await;

        // Report degraded (not just "alive") when an enabled+configured app failed to respond —
        // this shows up distinctly from a plain crash in the Platform Health panel.
        let errors: Vec<&str> = [&stats.sonarr_error, &stats.radarr_error, &stats.lidarr_error]
            .into_iter()
            .filter_map(|e| e.as_deref())
            .collect();
        if errors.is_empty() {
            handle.tick();
        } else {
            handle.tick_err(errors.join("; "));
        }

        // WS `arr_stats` topic stays global-only (unchanged wire contract) — zone-scoped stats
        // are fetched on demand via `GET /api/arr/stats?zone=<id>` (see arr_routes::get_arr_stats)
        // rather than pushed, since only a zone actively being viewed needs live refreshes.
        crate::events::publish(&bus, crate::events::TOPIC_ARR_STATS, &stats);

        let zones = config_mgr.get().await.zones.clone();
        let mut zone_stats = HashMap::with_capacity(zones.len());
        for zone in &zones {
            zone_stats.insert(zone.id.clone(), compute_zone_arr_stats(&db, zone).await);
        }

        *cache.write() = ArrStatsSnapshot { global: Some(stats), zones: zone_stats };

        sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
    }
}
