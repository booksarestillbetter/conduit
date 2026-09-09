// src/engines/circuit_breaker.rs
use crate::config::ConfigManager;
use crate::db::Database;
use crate::engines::registry::{heartbeat_sleep, TaskHandle};
use crate::notify::NotificationManager;
use crate::fetcher::{ActiveCircuitBreaker, FetcherPool, UnifiedTorrent};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub async fn run_circuit_breaker_loop(
    config_mgr: ConfigManager,
    pool: Arc<FetcherPool>,
    db: Database,
    _notifiers: Arc<NotificationManager>,
    handle: TaskHandle,
) {
    info!("Starting Tracker Circuit Breaker & Swarm Pressure Relief Engine");

    let mut check_counter = 0u64;

    loop {
        let config = config_mgr.get().await;
        let cb_cfg = config.tracker_circuit_breaker.clone();

        if cb_cfg.enabled {
            let torrents = pool.get_torrents(None, None, None);

            // Group torrents by tracker announce host
            let mut tracker_map: HashMap<String, Vec<UnifiedTorrent>> = HashMap::new();
            for t in torrents {
                for ts in &t.tracker_stats {
                    let host = ts.host.clone();
                    if !host.is_empty() && host != "DHT" && host != "PEX" && host != "LPD" {
                        tracker_map.entry(host).or_default().push(t.clone());
                    }
                }
            }

            let active_breakers = pool.get_active_breakers();

            // 1. Check existing active circuit breakers for recovery
            let mut recovered_trackers = Vec::new();
            for (tracker_host, breaker) in &active_breakers {
                let candidate_torrents = tracker_map.get(tracker_host);

                // Find canary torrent
                let canary_opt = candidate_torrents.and_then(|list| {
                    list.iter().find(|t| t.compound_id == breaker.canary_compound_id)
                });

                let mut is_recovered = false;
                let mut forced_timeout_recovery = false;

                let tripped_secs = (chrono::Utc::now().timestamp() - breaker.tripped_at).max(0);
                if cb_cfg.max_tripped_secs > 0 && tripped_secs >= cb_cfg.max_tripped_secs {
                    // Stuck-open guard: don't trust canary-announce parsing forever — if a breaker
                    // has been tripped longer than the configured ceiling, force it back open so a
                    // canary that silently lost its tracker-stats entry can't wedge it shut permanently.
                    warn!(
                        "⚡ Tracker '{}' circuit breaker exceeded max tripped duration ({}s >= {}s). Force-recovering.",
                        tracker_host, tripped_secs, cb_cfg.max_tripped_secs
                    );
                    is_recovered = true;
                    forced_timeout_recovery = true;
                } else if let Some(canary) = canary_opt {
                    // Ensure the canary probe is unpaused/running so it can actively query the tracker
                    if canary.raw_status == 0 {
                        if let Some(client) = pool.get_client(&canary.node) {
                            let _ = client.start_torrents(&[canary.id], false).await;
                        }
                    }

                    // Check if canary announce succeeded
                    for ts in &canary.tracker_stats {
                        if ts.host == *tracker_host {
                            let result = ts.last_announce_result.trim().to_lowercase();
                            if ts.last_announce_succeeded
                                || result.contains("success")
                                || result == "ok"
                                || result.starts_with("ok ")
                                || result.ends_with(" ok")
                            {
                                is_recovered = true;
                                break;
                            }
                        }
                    }
                } else {
                    // Canary no longer exists or tracker has no torrents
                    is_recovered = true;
                }

                if is_recovered {
                    info!(
                        "⚡ Tracker '{}' has recovered! Resuming {} paused torrents.",
                        tracker_host,
                        breaker.paused_torrents.len()
                    );

                    if cb_cfg.auto_resume_on_recovery {
                        for cid in &breaker.paused_torrents {
                            if let Some((node, id_str)) = cid.split_once(':') {
                                if let Ok(id) = id_str.parse::<i64>() {
                                    if let Some(client) = pool.get_client(node) {
                                        let _ = client.start_torrents(&[id], false).await;
                                    }
                                }
                            }
                        }
                    }

                    // Record event in DB & notify
                    let msg = if forced_timeout_recovery {
                        format!(
                            "⚡ Tracker Circuit Breaker Force-Cleared: '{}' exceeded the max tripped duration ({}s) without a confirmed canary recovery — resuming {} torrents anyway so they aren't paused forever. Verify the tracker manually if it's still down.",
                            tracker_host,
                            cb_cfg.max_tripped_secs,
                            breaker.paused_torrents.len()
                        )
                    } else {
                        format!(
                            "⚡ Tracker Recovered: '{}' is back online! Resumed {} torrents across swarms.",
                            tracker_host,
                            breaker.paused_torrents.len()
                        )
                    };
                    let _ = db.log_event(
                        "tracker_circuit_breaker",
                        if forced_timeout_recovery { "warn" } else { "info" },
                        &msg,
                        None,
                    );
                    NotificationManager::dispatch(
                        &config.notifications,
                        "tracker_circuit_breaker",
                        None,
                        if forced_timeout_recovery { "Tracker Breaker Force-Cleared" } else { "Tracker Recovered" },
                        &msg,
                        Some(if forced_timeout_recovery { "#F59E0B" } else { "#10B981" }),
                    )
                    .await;

                    let _ = db.remove_circuit_breaker(tracker_host);
                    recovered_trackers.push(tracker_host.clone());
                } else {
                    // Breaker is STILL active and unrecovered!
                    // Enforce swarm pressure relief on any active non-canary torrents that may have been started
                    if let Some(candidate_torrents) = candidate_torrents {
                        let mut updated_paused = breaker.paused_torrents.clone();
                        for t in candidate_torrents {
                            if t.compound_id != breaker.canary_compound_id && t.raw_status != 0 {
                                if let Some(client) = pool.get_client(&t.node) {
                                    if client.stop_torrents(&[t.id]).await.is_ok() {
                                        let cid = format!("{}:{}", t.node, t.id);
                                        if !updated_paused.contains(&cid) {
                                            updated_paused.push(cid);
                                        }
                                    }
                                }
                            }
                        }
                        if updated_paused.len() != breaker.paused_torrents.len() {
                            let mut updated_breaker = breaker.clone();
                            updated_breaker.paused_torrents = updated_paused;
                            let _ = db.save_circuit_breaker(&updated_breaker);
                            pool.set_active_breaker(tracker_host.clone(), updated_breaker);
                        }
                    }
                }
            }

            for tr in recovered_trackers {
                let _ = db.remove_circuit_breaker(&tr);
                pool.remove_active_breaker(&tr);
            }

            // 2. Scan all trackers for failures to trip breaker
            let current_breakers = pool.get_active_breakers();
            for (tracker_host, t_list) in &tracker_map {
                if current_breakers.contains_key(tracker_host) {
                    continue; // Already in circuit breaker state
                }

                // Check torrents reporting an error matching error_patterns
                let mut failing_count = 0usize;
                let mut failing_error: Option<String> = None;

                for t in t_list {
                    for ts in &t.tracker_stats {
                        if ts.host == *tracker_host {
                            let full_err = ts.last_announce_result.trim();
                            if !full_err.is_empty() {
                                let mut matched = false;
                                for pat in &cb_cfg.error_patterns {
                                    if full_err.to_lowercase().contains(&pat.to_lowercase()) {
                                        matched = true;
                                        break;
                                    }
                                }
                                if matched {
                                    failing_count += 1;
                                    if failing_error.is_none() {
                                        failing_error = Some(full_err.to_string());
                                    }
                                }
                            }
                            break;
                        }
                    }
                }

                let total_count = t_list.len();
                let failure_ratio = if total_count > 0 { failing_count as f64 / total_count as f64 } else { 0.0 };

                let should_trip = total_count > 1
                    && failing_count >= cb_cfg.min_failures
                    && failure_ratio >= cb_cfg.failure_ratio_threshold;

                if should_trip {
                    if let Some(error_msg) = failing_error {
                        if cb_cfg.canary_probe_enabled {
                            let canary = &t_list[0];
                            let canary_id = canary.compound_id.clone();
                            let canary_name = canary.name.clone();

                            if canary.raw_status == 0 {
                                if let Some(client) = pool.get_client(&canary.node) {
                                    let _ = client.start_torrents(&[canary.id], false).await;
                                }
                            }

                            let mut paused_list = Vec::new();
                            for t in &t_list[1..] {
                                if t.raw_status != 0 {
                                    if let Some(client) = pool.get_client(&t.node) {
                                        if client.stop_torrents(&[t.id]).await.is_ok() {
                                            paused_list.push(format!("{}:{}", t.node, t.id));
                                        }
                                    }
                                }
                            }

                            warn!(
                                "⚡ Tracker Circuit Breaker Tripped for '{}' ({}/{} failing, {:.1}% >= {:.1}% threshold, Error: {}). Paused {} active torrents, maintaining canary probe '{}'.",
                                tracker_host,
                                failing_count,
                                total_count,
                                failure_ratio * 100.0,
                                cb_cfg.failure_ratio_threshold * 100.0,
                                error_msg,
                                paused_list.len(),
                                canary_name
                            );

                            let msg = format!(
                                "⚡ Tracker Circuit Breaker Tripped: '{}' is unreachable ({}/{} failing, {:.1}%, Error: {}). Paused {} torrents, keeping 1 canary probe ('{}') active to relieve swarm pressure.",
                                tracker_host,
                                failing_count,
                                total_count,
                                failure_ratio * 100.0,
                                error_msg,
                                paused_list.len(),
                                canary_name
                            );
                            let _ = db.log_event("tracker_circuit_breaker", "warn", &msg, None);
                            NotificationManager::dispatch(
                                &config.notifications,
                                "tracker_circuit_breaker",
                                None,
                                "Tracker Circuit Breaker",
                                &msg,
                                Some("#F59E0B"),
                            )
                            .await;

                            let breaker_record = ActiveCircuitBreaker {
                                tracker_host: tracker_host.clone(),
                                canary_compound_id: canary_id,
                                canary_name,
                                paused_torrents: paused_list,
                                failing_error: error_msg,
                                tripped_at: chrono::Utc::now().timestamp(),
                            };

                            let _ = db.save_circuit_breaker(&breaker_record);
                            pool.set_active_breaker(tracker_host.clone(), breaker_record);
                        }
                    }
                }
            }

            // Periodic audit log heartbeat (every ~60s)
            check_counter += 1;
            if check_counter.is_multiple_of(12) {
                let breakers = pool.get_active_breakers();
                if !breakers.is_empty() {
                    let msg = format!(
                        "⚡ Swarm Pressure Relief Active: Monitoring {} tripped tracker(s) with active canary probes.",
                        breakers.len()
                    );
                    let _ = db.log_event("tracker_circuit_breaker", "info", &msg, None);
                }
            }
        }

        let interval = cb_cfg.check_interval_secs.max(5);
        heartbeat_sleep(&handle, Duration::from_secs(interval)).await;
    }
}
