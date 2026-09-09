// src/engines/space_manager.rs
use crate::config::ConfigManager;
use crate::db::Database;
use crate::engines::registry::{heartbeat_sleep, TaskHandle};
use crate::notify::NotificationManager;
use crate::fetcher::{TorrentStatus, FetcherPool};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

pub async fn run_space_manager_loop(
    config_mgr: ConfigManager,
    pool: Arc<FetcherPool>,
    db: Database,
    handle: TaskHandle,
) {
    info!("Starting Space Management & Auto-Purge Engine");
    let _ = db.log_event("space_manager", "info", "🐕 Space Management & Storage Guard Engine started", None);

    loop {
        let config = config_mgr.get().await;
        if config.space_manager.enabled {
            for (node_name, node_cfg) in &config.nodes {
                if !node_cfg.enabled || !node_cfg.auto_purge_enabled {
                    continue;
                }

                if let Some(client) = pool.get_client(node_name) {
                    let min_space_gb = node_cfg
                        .auto_purge_min_space_gb
                        .unwrap_or(config.space_manager.global_min_space_gb);

                    let torrents = pool.get_torrents(Some(node_name), None, None);
                    // Measure free space on the actual download mount, not the root filesystem —
                    // fall back to "/" only if no torrents exist yet to source a real path from.
                    let free_space_path = torrents
                        .first()
                        .map(|t| t.download_dir.clone())
                        .filter(|d| !d.is_empty())
                        .unwrap_or_else(|| "/".to_string());

                    // Check free space on node
                    if let Ok(free_bytes) = client.get_free_space(&free_space_path).await {
                        let free_gb = (free_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
                        if free_gb < (min_space_gb as f64) {
                            info!(
                                "Node {} free space ({:.1} GB at {}) below threshold ({} GB). Evaluating auto-purge candidates.",
                                node_name, free_gb, free_space_path, min_space_gb
                            );

                            let target_ratio = node_cfg.auto_purge_ratio.unwrap_or(config.space_manager.default_target_ratio);
                            let target_age_days = node_cfg.auto_purge_age_days.unwrap_or(config.space_manager.default_target_age_days);
                            let target_seeds = node_cfg.auto_purge_seeds.unwrap_or(config.space_manager.default_target_seeds);
                            let required_matches = node_cfg.auto_purge_match_count.unwrap_or(config.space_manager.default_required_match_count);

                            let now_ts = Utc::now().timestamp();
                            // (torrent id, display name+ratio, reclaimable bytes)
                            let mut candidates: Vec<(i64, String, i64)> = Vec::new();

                            for t in torrents {
                                // Only consider seeding or stopped completed torrents
                                if t.percent_done < 1.0 || (t.status != TorrentStatus::Seeding && t.status != TorrentStatus::Stopped) {
                                    continue;
                                }

                                let mut matches = 0u32;

                                // 1. Upload Ratio check
                                if t.upload_ratio >= target_ratio {
                                    matches += 1;
                                }

                                // 2. Age check
                                if t.done_date > 0 && now_ts >= t.done_date {
                                    let age_secs = (now_ts - t.done_date) as u64;
                                    let age_days = age_secs / 86400;
                                    if age_days >= target_age_days {
                                        matches += 1;
                                    }
                                }

                                // 3. Seeder count check
                                let seeder_count = t.tracker_stats.first().map(|ts| ts.seeder_count).unwrap_or(0);
                                if seeder_count as u64 >= target_seeds {
                                    matches += 1;
                                }

                                if matches >= required_matches {
                                    candidates.push((t.id, format!("{} (ratio: {:.2})", t.name, t.upload_ratio), t.total_size));
                                }
                            }

                            // Reclaim the most space first: sort largest-to-smallest before capping.
                            candidates.sort_by_key(|a| std::cmp::Reverse(a.2));

                            let max_purges = config.space_manager.max_purges_per_cycle;
                            candidates.truncate(max_purges);

                            if !candidates.is_empty() {
                                info!(
                                    "Auto-purging up to {} torrents on node {} (largest reclaimable first): {:?}",
                                    candidates.len(),
                                    node_name,
                                    candidates.iter().map(|c| c.1.clone()).collect::<Vec<_>>()
                                );

                                // Remove one at a time, rechecking free space between removals so we
                                // don't purge more than actually needed to clear the deficit.
                                let mut purged_ids: Vec<i64> = Vec::new();
                                let mut purged_names: Vec<String> = Vec::new();
                                let mut last_free_gb = free_gb;
                                for (id, display_name, _size) in &candidates {
                                    if client.remove_torrents(&[*id], true).await.is_ok() {
                                        purged_ids.push(*id);
                                        purged_names.push(display_name.clone());
                                    }

                                    if let Ok(bytes) = client.get_free_space(&free_space_path).await {
                                        last_free_gb = (bytes as f64) / (1024.0 * 1024.0 * 1024.0);
                                        if last_free_gb >= (min_space_gb as f64) {
                                            break;
                                        }
                                    }
                                }

                                if !purged_ids.is_empty() {
                                    let summary = format!(
                                        "Auto-purged {} torrents on node {} to reclaim disk space ({:.1} GB now free at {}).\nPurged items:\n- {}",
                                        purged_ids.len(),
                                        node_name,
                                        last_free_gb,
                                        free_space_path,
                                        purged_names.join("\n- ")
                                    );

                                    let _ = db.log_event(
                                        "space_manager",
                                        "info",
                                        &format!("Auto-purged {} torrents on {}", purged_ids.len(), node_name),
                                        Some(&serde_json::to_string(&purged_names).unwrap_or_default()),
                                    );

                                    // Category filtering is now per-target inside dispatch itself.
                                    NotificationManager::dispatch(
                                        &config.notifications,
                                        "space.autopurge",
                                        None,
                                        &format!("Space Manager: Purged {} on {}", purged_ids.len(), node_name),
                                        &summary,
                                        Some("#F59E0B"), // Amber
                                    ).await;
                                }
                            }
                        }
                    }
                }
            }
        }

        let interval = config.space_manager.check_interval_secs.max(30);
        heartbeat_sleep(&handle, Duration::from_secs(interval)).await;
    }
}
