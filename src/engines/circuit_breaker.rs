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

/// True if every node touching this tracker host runs its own native breaker — Conduit
/// hands off management entirely rather than double-managing it externally. Empty
/// candidate lists are never "all passive" (nothing to hand off to).
fn is_host_all_passive(pool: &FetcherPool, t_list: &[UnifiedTorrent]) -> bool {
    !t_list.is_empty() && t_list.iter().all(|t| pool.is_passive_mode(&t.node))
}

/// Ensures the canary probe is running, then reports whether its most recent announce to
/// `tracker_host` succeeded. A missing canary (removed/completed) is treated as recovered,
/// matching this engine's long-standing "nothing left to watch, stop blocking" behavior.
async fn check_canary_recovered(pool: &FetcherPool, tracker_host: &str, canary_opt: Option<&UnifiedTorrent>) -> bool {
    let Some(canary) = canary_opt else {
        return true;
    };
    if canary.raw_status == 0 {
        if let Some(client) = pool.get_client(&canary.node) {
            let _ = client.start_torrents(&[canary.id], false).await;
        }
    }
    for ts in &canary.tracker_stats {
        if ts.host == *tracker_host {
            let result = ts.last_announce_result.trim().to_lowercase();
            return ts.last_announce_succeeded
                || result.contains("success")
                || result == "ok"
                || result.starts_with("ok ")
                || result.ends_with(" ok");
        }
    }
    false
}

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

            let now = chrono::Utc::now().timestamp();
            let active_breakers = pool.get_active_breakers();

            // 1. Check existing active circuit breakers for recovery/ramp-up/relapse
            let mut recovered_trackers = Vec::new();
            for (tracker_host, breaker) in &active_breakers {
                let candidate_torrents = tracker_map.get(tracker_host);

                // Passive handoff: the owning node(s) now manage this tracker natively
                // (freshly detected capability, or a config change) — resume everything
                // and drop Conduit's own record rather than keep fighting the daemon.
                if let Some(list) = candidate_torrents {
                    if is_host_all_passive(&pool, list) {
                        for cid in &breaker.paused_torrents {
                            if let Some((node, id_str)) = cid.split_once(':') {
                                if let Ok(id) = id_str.parse::<i64>() {
                                    if let Some(client) = pool.get_client(node) {
                                        let _ = client.start_torrents(&[id], false).await;
                                    }
                                }
                            }
                        }
                        let msg = format!(
                            "⚡ Tracker '{}' is now managed natively by its node — handing off from Conduit's external breaker and resuming {} torrents.",
                            tracker_host,
                            breaker.paused_torrents.len()
                        );
                        let _ = db.log_event("tracker_circuit_breaker", "info", &msg, None);
                        let _ = db.remove_circuit_breaker(tracker_host);
                        recovered_trackers.push(tracker_host.clone());
                        continue;
                    }
                }

                let canary_opt = candidate_torrents.and_then(|list| {
                    list.iter().find(|t| t.compound_id == breaker.canary_compound_id)
                });

                let tripped_secs = (now - breaker.tripped_at).max(0);

                // Stuck-open guard: applies regardless of state.
                if cb_cfg.max_tripped_secs > 0 && tripped_secs >= cb_cfg.max_tripped_secs {
                    warn!(
                        "⚡ Tracker '{}' circuit breaker exceeded max tripped duration ({}s >= {}s). Force-recovering.",
                        tracker_host, tripped_secs, cb_cfg.max_tripped_secs
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
                    let msg = format!(
                        "⚡ Tracker Circuit Breaker Force-Cleared: '{}' exceeded the max tripped duration ({}s) without a confirmed canary recovery — resuming {} torrents anyway so they aren't paused forever. Verify the tracker manually if it's still down.",
                        tracker_host, cb_cfg.max_tripped_secs, breaker.paused_torrents.len()
                    );
                    let _ = db.log_event("tracker_circuit_breaker", "warn", &msg, None);
                    NotificationManager::dispatch(
                        &config.notifications,
                        "tracker_circuit_breaker",
                        None,
                        "Tracker Breaker Force-Cleared",
                        &msg,
                        Some("#F59E0B"),
                    )
                    .await;
                    let _ = db.remove_circuit_breaker(tracker_host);
                    recovered_trackers.push(tracker_host.clone());
                    continue;
                }

                if breaker.state == "recovering" {
                    // Ramp-up phase: any failure aborts immediately with doubled backoff;
                    // otherwise stagger resumption across the ramp window and graduate once
                    // both the window has elapsed and enough consecutive successes landed.
                    let recovered_this_tick = check_canary_recovered(&pool, tracker_host, canary_opt).await;

                    if !recovered_this_tick {
                        let prev_backoff = if breaker.backoff_secs > 0 { breaker.backoff_secs } else { cb_cfg.initial_backoff_secs };
                        let new_backoff = (prev_backoff * 2).min(cb_cfg.max_tripped_secs.max(prev_backoff));
                        // Re-pause everyone but the canary, however far the ramp had gotten.
                        let mut re_paused = Vec::new();
                        if let Some(candidates) = candidate_torrents {
                            for t in candidates {
                                if t.compound_id != breaker.canary_compound_id {
                                    if t.raw_status != 0 {
                                        if let Some(client) = pool.get_client(&t.node) {
                                            let _ = client.stop_torrents(&[t.id]).await;
                                        }
                                    }
                                    re_paused.push(format!("{}:{}", t.node, t.id));
                                }
                            }
                        }
                        let mut relapsed = breaker.clone();
                        relapsed.state = "tripped".to_string();
                        relapsed.tripped_at = now;
                        relapsed.backoff_secs = new_backoff;
                        relapsed.recovery_started_at = None;
                        relapsed.consecutive_successes = 0;
                        relapsed.paused_torrents = re_paused;
                        relapsed.failing_error = "Relapsed during recovery ramp-up".to_string();
                        warn!(
                            "⚡ Tracker '{}' relapsed during recovery ramp-up. Re-tripping with backoff {}s.",
                            tracker_host, new_backoff
                        );
                        let msg = format!(
                            "⚡ Tracker Circuit Breaker Relapsed: '{}' failed again during its recovery ramp-up. Re-tripped with a {}s cooldown before the next attempt.",
                            tracker_host, new_backoff
                        );
                        let _ = db.log_event("tracker_circuit_breaker", "warn", &msg, None);
                        let _ = db.save_circuit_breaker(&relapsed);
                        pool.set_active_breaker(tracker_host.clone(), relapsed);
                        continue;
                    }

                    let recovery_started_at = breaker.recovery_started_at.unwrap_or(now);
                    let elapsed_secs = (now - recovery_started_at).max(0) as u64;
                    let ramp_secs = cb_cfg.recovery_ramp_secs.max(1);
                    let consecutive_successes = breaker.consecutive_successes + 1;

                    let graduated = elapsed_secs >= ramp_secs && consecutive_successes >= cb_cfg.recovery_success_threshold;

                    if graduated {
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
                        info!(
                            "⚡ Tracker '{}' completed recovery ramp-up with {} consecutive successes. Fully healthy.",
                            tracker_host, consecutive_successes
                        );
                        let msg = format!(
                            "⚡ Tracker Recovered: '{}' completed its recovery ramp-up and is back to full speed. Resumed {} remaining torrents.",
                            tracker_host,
                            breaker.paused_torrents.len()
                        );
                        let _ = db.log_event("tracker_circuit_breaker", "info", &msg, None);
                        NotificationManager::dispatch(
                            &config.notifications,
                            "tracker_circuit_breaker",
                            None,
                            "Tracker Recovered",
                            &msg,
                            Some("#10B981"),
                        )
                        .await;
                        let _ = db.remove_circuit_breaker(tracker_host);
                        recovered_trackers.push(tracker_host.clone());
                    } else {
                        // Stagger: resume paused torrents proportionally to ramp progress.
                        let ramp_fraction = (elapsed_secs as f64 / ramp_secs as f64).clamp(0.0, 1.0);
                        let total_ever_paused = breaker.paused_torrents.len();
                        let target_still_paused = if total_ever_paused == 0 {
                            0
                        } else {
                            let target_resumed = ((total_ever_paused as f64) * ramp_fraction).round() as usize;
                            total_ever_paused.saturating_sub(target_resumed)
                        };

                        let mut still_paused = breaker.paused_torrents.clone();
                        while still_paused.len() > target_still_paused {
                            let Some(cid) = still_paused.pop() else { break };
                            if let Some((node, id_str)) = cid.split_once(':') {
                                if let Ok(id) = id_str.parse::<i64>() {
                                    if let Some(client) = pool.get_client(node) {
                                        let _ = client.start_torrents(&[id], false).await;
                                    }
                                }
                            }
                        }

                        let mut updated = breaker.clone();
                        updated.paused_torrents = still_paused;
                        updated.consecutive_successes = consecutive_successes;
                        let _ = db.save_circuit_breaker(&updated);
                        pool.set_active_breaker(tracker_host.clone(), updated);
                    }
                } else {
                    // "tripped" (or legacy/empty state): stay in backoff cooldown before
                    // even considering the canary's recovery announce.
                    let effective_backoff = if breaker.backoff_secs > 0 { breaker.backoff_secs } else { cb_cfg.initial_backoff_secs };

                    if tripped_secs < effective_backoff {
                        // Still cooling down — just keep enforcing pause on anything newly active.
                        if let Some(candidates) = candidate_torrents {
                            let mut updated_paused = breaker.paused_torrents.clone();
                            for t in candidates {
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
                        continue;
                    }

                    let recovered_this_tick = check_canary_recovered(&pool, tracker_host, canary_opt).await;

                    if recovered_this_tick {
                        info!(
                            "⚡ Canary probe succeeded for '{}'! Entering recovery ramp-up ({}s window).",
                            tracker_host, cb_cfg.recovery_ramp_secs
                        );
                        let msg = format!(
                            "⚡ Tracker Recovering: '{}' canary probe succeeded. Ramping traffic back up over the next {}s to avoid slamming it.",
                            tracker_host, cb_cfg.recovery_ramp_secs
                        );
                        let _ = db.log_event("tracker_circuit_breaker", "info", &msg, None);
                        NotificationManager::dispatch(
                            &config.notifications,
                            "tracker_circuit_breaker",
                            None,
                            "Tracker Recovering",
                            &msg,
                            Some("#3B82F6"),
                        )
                        .await;

                        let mut updated = breaker.clone();
                        updated.state = "recovering".to_string();
                        updated.recovery_started_at = Some(now);
                        updated.consecutive_successes = 1;
                        let _ = db.save_circuit_breaker(&updated);
                        pool.set_active_breaker(tracker_host.clone(), updated);
                    } else if let Some(candidates) = candidate_torrents {
                        // Still failing: keep enforcing pause on anything newly active.
                        let mut updated_paused = breaker.paused_torrents.clone();
                        for t in candidates {
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

            // 2. Scan all trackers for failures to trip breaker (skip passive-mode hosts)
            let current_breakers = pool.get_active_breakers();
            for (tracker_host, t_list) in &tracker_map {
                if current_breakers.contains_key(tracker_host) {
                    continue; // Already in circuit breaker state
                }
                if is_host_all_passive(&pool, t_list) {
                    continue; // Owning node(s) manage this tracker natively
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
                                tripped_at: now,
                                state: "tripped".to_string(),
                                recovery_started_at: None,
                                consecutive_successes: 0,
                                backoff_secs: cb_cfg.initial_backoff_secs,
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
