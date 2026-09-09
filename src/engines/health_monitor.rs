// src/engines/health_monitor.rs
use crate::config::ConfigManager;
use crate::db::Database;
use crate::engines::arr_stats_poller::ArrStatsCache;
use crate::engines::registry::{heartbeat_sleep, TaskHandle};
use crate::notify::NotificationManager;
use crate::fetcher::FetcherPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ServiceStatus {
    Online,
    Offline(String),
}

pub async fn run_health_monitor_loop(
    config_mgr: ConfigManager,
    pool: Arc<FetcherPool>,
    db: Database,
    _notifiers: Arc<NotificationManager>,
    arr_stats_cache: ArrStatsCache,
    handle: TaskHandle,
) {
    info!("Starting Conduit Service Health Monitor & Failure Alert Engine");
    let _ = db.log_event("health_monitor", "info", "🐕 Conduit Service Health Monitor Engine started", None);

    // Map of service_key -> previous known status
    let mut status_map: HashMap<String, ServiceStatus> = HashMap::new();
    let mut failure_counts: HashMap<String, usize> = HashMap::new();

    loop {
        heartbeat_sleep(&handle, Duration::from_secs(30)).await;

        let config = config_mgr.get().await;
        let mut current_checks: Vec<(String, String, ServiceStatus)> = Vec::new();

        // 1. Check Fetcher Nodes via Pool Health
        let health = pool.get_system_health();
        for node in &health.nodes {
            let key = format!("fetcher:{}", node.name);
            let display = format!("Fetcher Node '{}'", node.name);

            if node.connected {
                current_checks.push((key, display, ServiceStatus::Online));
            } else {
                let err = node.last_error.clone().unwrap_or_else(|| "RPC Connection Refused / Timed Out".to_string());
                current_checks.push((key, display, ServiceStatus::Offline(err)));
            }
        }

        // 2-4. Sonarr/Radarr/Lidarr — read from the shared arr_stats_poller cache instead of
        // independently polling system/status ourselves. That poller already hits these same
        // endpoints every 30s for the dashboard; this used to duplicate that exact traffic on
        // its own uncoordinated timer. A stale-by-up-to-30s read is fine here since alerts only
        // fire after 2 consecutive Offline observations (60s) anyway.
        if let Some(stats) = arr_stats_cache.read().global.clone() {
            if config.sonarr.enabled {
                if let Some(ref primary) = config.sonarr.primary {
                    let display = format!("Sonarr TV ({})", primary.base_url);
                    let status = match stats.sonarr_error {
                        Some(err) => ServiceStatus::Offline(err),
                        None => ServiceStatus::Online,
                    };
                    current_checks.push(("arr:sonarr".to_string(), display, status));
                }
            }
            if config.radarr.enabled {
                if let Some(ref primary) = config.radarr.primary {
                    let display = format!("Radarr Movies ({})", primary.base_url);
                    let status = match stats.radarr_error {
                        Some(err) => ServiceStatus::Offline(err),
                        None => ServiceStatus::Online,
                    };
                    current_checks.push(("arr:radarr".to_string(), display, status));
                }
            }
            if config.lidarr.enabled {
                if let Some(ref primary) = config.lidarr.primary {
                    let display = format!("Lidarr Music ({})", primary.base_url);
                    let status = match stats.lidarr_error {
                        Some(err) => ServiceStatus::Offline(err),
                        None => ServiceStatus::Online,
                    };
                    current_checks.push(("arr:lidarr".to_string(), display, status));
                }
            }
        }

        // 5. Evaluate state transitions and dispatch alerts
        for (key, display_name, status) in current_checks {
            let prev_status = status_map.get(&key).cloned().unwrap_or(ServiceStatus::Online);

            match &status {
                ServiceStatus::Online => {
                    failure_counts.insert(key.clone(), 0);
                    if let ServiceStatus::Offline(err_msg) = prev_status {
                        info!("✅ Service Restored: {} (was offline: {})", display_name, err_msg);
                        let log_msg = format!("✅ Service Restored: {} is back online and responding normally", display_name);
                        let _ = db.log_event("health_monitor", "info", &log_msg, None);

                        let notif_title = format!("✅ Conduit Health Restored • {}", display_name);
                        let notif_body = format!("Service **{}** has recovered and is communicating normally.", display_name);
                        let config_notif = config.notifications.clone();
                        tokio::spawn(async move {
                            NotificationManager::dispatch(
                                &config_notif,
                                "health.recovery",
                                None,
                                &notif_title,
                                &notif_body,
                                Some("#10B981"),
                            ).await;
                        });
                    }
                    status_map.insert(key, ServiceStatus::Online);
                }
                ServiceStatus::Offline(err) => {
                    let count = failure_counts.entry(key.clone()).or_insert(0);
                    *count += 1;

                    // Require 2 consecutive failures before triggering high-priority alert to prevent flapping
                    if *count == 2 {
                        warn!("🚨 Service Alert: {} unreachable: {}", display_name, err);
                        let log_msg = format!("🚨 Service Alert: {} is UNREACHABLE: {}", display_name, err);
                        let _ = db.log_event("health_monitor", "error", &log_msg, None);

                        let notif_title = format!("🚨 Conduit Health Alert • {}", display_name);
                        let notif_body = format!(
                            "Service **{}** is currently UNREACHABLE / FAILING.\n\n• **Error:** `{}`\n• **Timestamp:** {}",
                            display_name,
                            err,
                            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                        );
                        let config_notif = config.notifications.clone();
                        tokio::spawn(async move {
                            NotificationManager::dispatch(
                                &config_notif,
                                "health.alert",
                                None,
                                &notif_title,
                                &notif_body,
                                Some("#EF4444"),
                            ).await;
                        });
                        status_map.insert(key, ServiceStatus::Offline(err.clone()));
                    }
                }
            }
        }
    }
}
