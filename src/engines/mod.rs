// src/engines/mod.rs
pub mod arr_stats_poller;
pub mod arr_sync;
pub mod circuit_breaker;
pub mod file_sync;
pub mod health_monitor;
pub mod influx_pusher;
pub mod ip_asn_updater;
pub mod pipeline;
pub mod poller;
pub mod registry;
pub mod space_manager;
mod supervisor;
pub mod telemetry;
pub mod watch_sync;

use crate::config::ConfigManager;
use crate::db::Database;
use crate::events::EventBus;
use crate::notify::NotificationManager;
use crate::fetcher::FetcherPool;
use registry::{EngineRegistry, TaskHandle};
use std::sync::Arc;
use supervisor::spawn_supervised;

#[allow(clippy::too_many_arguments)]
pub fn start_all_engines(
    config_mgr: ConfigManager,
    pool: Arc<FetcherPool>,
    db: Database,
    notifiers: Arc<NotificationManager>,
    arr_stats_cache: arr_stats_poller::ArrStatsCache,
    ip_asn_cache: crate::ip_asn::IpAsnCache,
    registry: EngineRegistry,
    event_bus: EventBus,
) {
    // 1. Transmission Poller
    {
        let (config_mgr, pool, db, registry) = (config_mgr.clone(), pool.clone(), db.clone(), registry.clone());
        spawn_supervised("poller", registry.clone(), move || {
            poller::run_poller_loop(config_mgr.clone(), pool.clone(), db.clone(), TaskHandle::new(registry.clone(), "poller"))
        });
    }

    // 2. Space Manager & Auto-Purge
    {
        let (config_mgr, pool, db, registry) = (config_mgr.clone(), pool.clone(), db.clone(), registry.clone());
        spawn_supervised("space_manager", registry.clone(), move || {
            space_manager::run_space_manager_loop(config_mgr.clone(), pool.clone(), db.clone(), TaskHandle::new(registry.clone(), "space_manager"))
        });
    }

    // 3. Error Pipeline & MediaReplacer
    {
        let (config_mgr, pool, db, registry) = (config_mgr.clone(), pool.clone(), db.clone(), registry.clone());
        spawn_supervised("pipeline", registry.clone(), move || {
            pipeline::run_pipeline_loop(config_mgr.clone(), pool.clone(), db.clone(), TaskHandle::new(registry.clone(), "pipeline"))
        });
    }

    // 4. Tracker Circuit Breaker & Swarm Pressure Relief
    {
        let (config_mgr, pool, db, notifiers, registry) = (config_mgr.clone(), pool.clone(), db.clone(), notifiers.clone(), registry.clone());
        spawn_supervised("circuit_breaker", registry.clone(), move || {
            circuit_breaker::run_circuit_breaker_loop(config_mgr.clone(), pool.clone(), db.clone(), notifiers.clone(), TaskHandle::new(registry.clone(), "circuit_breaker"))
        });
    }

    // 5. File Sync & Queue Cleaner
    {
        let (config_mgr, db, registry) = (config_mgr.clone(), db.clone(), registry.clone());
        spawn_supervised("file_sync", registry.clone(), move || {
            file_sync::run_file_sync_loop(config_mgr.clone(), db.clone(), TaskHandle::new(registry.clone(), "file_sync"))
        });
    }

    // 6. Sonarr / Radarr Multi-Node Sync
    {
        let (config_mgr, db, registry) = (config_mgr.clone(), db.clone(), registry.clone());
        spawn_supervised("arr_sync", registry.clone(), move || {
            arr_sync::run_arr_sync_loop(config_mgr.clone(), db.clone(), TaskHandle::new(registry.clone(), "arr_sync"))
        });
    }

    // 7. Trakt & Plex Watch History Sync
    {
        let (config_mgr, db, registry) = (config_mgr.clone(), db.clone(), registry.clone());
        spawn_supervised("watch_sync", registry.clone(), move || {
            watch_sync::run_watch_sync_loop(config_mgr.clone(), db.clone(), TaskHandle::new(registry.clone(), "watch_sync"))
        });
    }

    // 8. InfluxDB Metrics Pusher
    {
        let (config_mgr, pool, registry) = (config_mgr.clone(), pool.clone(), registry.clone());
        spawn_supervised("influx_pusher", registry.clone(), move || {
            influx_pusher::run_influx_pusher_loop(config_mgr.clone(), pool.clone(), TaskHandle::new(registry.clone(), "influx_pusher"))
        });
    }

    // 9. Service Health Monitor & Failure Alerts
    {
        let (config_mgr, pool, db, notifiers, arr_stats_cache, registry) = (config_mgr.clone(), pool.clone(), db.clone(), notifiers.clone(), arr_stats_cache.clone(), registry.clone());
        spawn_supervised("health_monitor", registry.clone(), move || {
            health_monitor::run_health_monitor_loop(config_mgr.clone(), pool.clone(), db.clone(), notifiers.clone(), arr_stats_cache.clone(), TaskHandle::new(registry.clone(), "health_monitor"))
        });
    }

    // 10. Arr Ecosystem Stats Poller (backs the /api/arr/stats cache and the arr_stats topic)
    {
        let (config_mgr, db, arr_stats_cache, event_bus, registry) = (config_mgr.clone(), db.clone(), arr_stats_cache.clone(), event_bus.clone(), registry.clone());
        spawn_supervised("arr_stats_poller", registry.clone(), move || {
            arr_stats_poller::run_arr_stats_poller_loop(config_mgr.clone(), db.clone(), arr_stats_cache.clone(), event_bus.clone(), TaskHandle::new(registry.clone(), "arr_stats_poller"))
        });
    }

    // 11. IP-to-ASN Enrichment Updater (opt-in; see config.ip_asn.enabled)
    {
        let (config_mgr, ip_asn_cache, registry) = (config_mgr.clone(), ip_asn_cache.clone(), registry.clone());
        spawn_supervised("ip_asn_updater", registry.clone(), move || {
            ip_asn_updater::run_ip_asn_updater_loop(config_mgr.clone(), ip_asn_cache.clone(), TaskHandle::new(registry.clone(), "ip_asn_updater"))
        });
    }

    // 12. Platform Health Publisher (pushes the engine registry snapshot on the
    // platform_health topic — see registry::run_platform_health_publisher for why this is a
    // small standalone poll rather than threading the bus through every other engine above)
    {
        let (event_bus, registry) = (event_bus.clone(), registry.clone());
        spawn_supervised("platform_health_publisher", registry.clone(), move || {
            registry::run_platform_health_publisher(registry.clone(), event_bus.clone(), TaskHandle::new(registry.clone(), "platform_health_publisher"))
        });
    }

    // 13. Cluster Telemetry Publisher (computes the always-on aggregate-stats + enriched-torrent
    // snapshot once per second and publishes it on the telemetry topic — see
    // engines::telemetry for why this replaced N per-WebSocket-connection timers)
    {
        let (pool, db, event_bus, registry) = (pool.clone(), db.clone(), event_bus.clone(), registry.clone());
        spawn_supervised("telemetry_publisher", registry.clone(), move || {
            telemetry::run_telemetry_publisher(pool.clone(), db.clone(), event_bus.clone(), TaskHandle::new(registry.clone(), "telemetry_publisher"))
        });
    }
}
