// src/engines/poller.rs
use crate::config::ConfigManager;
use crate::db::Database;
use crate::engines::registry::{heartbeat_sleep, TaskHandle};
use crate::fetcher::FetcherPool;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

pub async fn run_poller_loop(config_mgr: ConfigManager, pool: Arc<FetcherPool>, db: Database, handle: TaskHandle) {
    info!("Starting Fetcher Poller Engine with adaptive anti-stacking backoff");

    let _ = db.log_event("poller", "info", "🐕 Fetcher Poller Engine started with adaptive latency backoff", None);

    let mut poll_cycle_count = 0u64;

    loop {
        let config = config_mgr.get().await;
        pool.sync_nodes_from_config(&config);

        let base_interval = config.system.poll_interval_secs.max(1);
        let clients = pool.list_clients();

        for client in clients {
            if pool.should_poll_node(client.node_name(), base_interval) {
                let pool_clone = pool.clone();
                let db_clone = db.clone();
                let node_name = client.node_name().to_string();
                tokio::spawn(async move {
                    if let Err(e) = pool_clone.poll_node(client).await {
                        let msg = format!("Failed to poll fetcher node '{}': {}", node_name, e);
                        let _ = db_clone.log_event("transmission_node", "warn", &msg, None);
                    }
                });
            }
        }

        poll_cycle_count += 1;

        // Record 1-second bandwidth snapshot (every 2nd 500ms cycle) for rolling 5-minute history
        if poll_cycle_count.is_multiple_of(2) {
            pool.record_bandwidth_snapshot();
        }

        // Periodic heartbeat log every ~60s (120 * 500ms)
        if poll_cycle_count.is_multiple_of(120) {
            let stats = pool.get_aggregate_stats();
            let msg = format!(
                "Cluster Telemetry: {}/{} nodes connected, {} total torrents (↓ {:.1} MB/s, ↑ {:.1} MB/s, Total: {:.2} TB)",
                stats.connected_nodes,
                stats.total_nodes,
                stats.total_torrents,
                (stats.total_download_speed as f64) / (1024.0 * 1024.0),
                (stats.total_upload_speed as f64) / (1024.0 * 1024.0),
                (stats.total_size_bytes as f64) / (1024.0 * 1024.0 * 1024.0 * 1024.0),
            );
            let _ = db.log_event("poller", "info", &msg, None);

            // Persist a minute-averaged bandwidth point for the 72h downsampled history (see
            // Database::insert_bandwidth_point) — smoothed over the last ~60 1s samples rather
            // than a single noisy instant.
            let (avg_down, avg_up) = pool.get_bandwidth_average(60);
            let _ = db.insert_bandwidth_point(chrono::Utc::now().timestamp_millis(), avg_down, avg_up);
        }

        // Fast 500ms scheduler check: immediately polls whichever nodes are eligible without queuing
        heartbeat_sleep(&handle, Duration::from_millis(500)).await;
    }
}
