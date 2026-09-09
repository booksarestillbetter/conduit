// src/engines/pipeline.rs
use crate::config::ConfigManager;
use crate::db::Database;
use crate::engines::registry::{heartbeat_sleep, TaskHandle};
use crate::notify::NotificationManager;
use crate::fetcher::FetcherPool;
use regex::Regex;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

pub async fn run_pipeline_loop(
    config_mgr: ConfigManager,
    fetcher_pool: Arc<FetcherPool>,
    db: Database,
    handle: TaskHandle,
) {
    info!("Starting Error Pipeline & MediaReplacer Engine");
    let _ = db.log_event("pipeline", "info", "🐕 Error Pipeline & MediaReplacer Engine started", None);

    // Tracks compound_id -> time when verify was triggered to allow verification to complete before purging
    let mut verified_attempts: HashMap<String, Instant> = HashMap::new();
    let mut checked_enrichment_hashes: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Cache the compiled unregistered-error regex and only rebuild it when the configured
    // pattern actually changes, rather than recompiling on every 30s tick.
    let mut cached_pattern: Option<String> = None;
    let mut cached_regex: Option<Regex> = None;

    // Built once, reused for every MediaReplacer re-search command below — previously this was
    // `reqwest::Client::new()` rebuilt per broken torrent found, with no timeout at all, so a
    // dead Sonarr/Radarr could hang this whole engine loop indefinitely.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    loop {
        heartbeat_sleep(&handle, Duration::from_secs(30)).await;

        let config = config_mgr.get().await;
        if !config.rule_pipeline.enabled {
            continue;
        }

        let pattern = &config.rule_pipeline.unregistered_pattern;
        if cached_pattern.as_deref() != Some(pattern.as_str()) {
            match Regex::new(&format!("(?i){}", pattern)) {
                Ok(r) => {
                    cached_regex = Some(r);
                    cached_pattern = Some(pattern.clone());
                }
                Err(e) => {
                    warn!("Invalid pipeline error regex '{}': {}", pattern, e);
                    cached_regex = None;
                    cached_pattern = None;
                    continue;
                }
            }
        }
        let regex = match &cached_regex {
            Some(r) => r,
            None => continue,
        };

        let torrents = fetcher_pool.get_torrents(None, None, None);

        // 0. Auto-enrich ONLY actively downloading or recently added intake torrents (last 15m) that haven't been checked yet
        let now_ts = chrono::Utc::now().timestamp();
        let mut candidates = Vec::new();
        for t in &torrents {
            let is_recent_or_downloading = (now_ts - t.added_date) < 900 || t.percent_done < 1.0;
            if is_recent_or_downloading && !checked_enrichment_hashes.contains(&t.hash_string) {
                checked_enrichment_hashes.insert(t.hash_string.clone());
                candidates.push(t.clone());
                if candidates.len() >= 2 {
                    break;
                }
            }
        }

        if checked_enrichment_hashes.len() > 10000 {
            checked_enrichment_hashes.clear();
        }

        if !candidates.is_empty() {
            let config_clone = config.clone();
            let db_clone = db.clone();
            tokio::spawn(async move {
                for t in candidates {
                    let exists = db_clone.find_arr_grab_by_hash_or_name(&t.hash_string, &t.name).ok().flatten().is_some();
                    if !exists {
                        let _ = crate::api::torrent_routes::perform_enrichment_for_torrent(
                            &config_clone,
                            &db_clone,
                            &t.name,
                            &t.hash_string,
                            &t.node,
                            t.total_size,
                        ).await;
                    }
                }
            });
        }

        // Prune old verify timestamps
        verified_attempts.retain(|_, timestamp| timestamp.elapsed() < Duration::from_secs(600));

        for t in torrents {
            let mut is_matched = false;
            let mut is_local_data_error = false;
            let mut error_reason = String::new();

            // Check if torrent has Transmission error 3 (TR_STAT_LOCAL_ERROR) or missing data text
            if t.error == 3
                || t.error_string.to_lowercase().contains("no data found")
                || t.error_string.to_lowercase().contains("ensure your drives are connected")
                || t.error_string.to_lowercase().contains("verify local data")
            {
                is_local_data_error = true;
                is_matched = true;
                error_reason = if !t.error_string.is_empty() {
                    t.error_string.clone()
                } else {
                    "No data found (Local Data Error 3)".to_string()
                };
            } else if !t.error_string.is_empty() && regex.is_match(&t.error_string) {
                is_matched = true;
                error_reason = t.error_string.clone();
            }

            // Check tracker announce results for unregistered/trumped messages
            if !is_matched {
                for ts in &t.tracker_stats {
                    if !ts.last_announce_result.is_empty() && regex.is_match(&ts.last_announce_result) {
                        is_matched = true;
                        error_reason = ts.last_announce_result.clone();
                        break;
                    }
                }
            }

            if is_matched {
                // If it's a local data error and we haven't attempted verification yet:
                if is_local_data_error && !verified_attempts.contains_key(&t.compound_id) {
                    info!(
                        "🛠️ Attempting automated 'Verify Local Data' for torrent with missing files: '{}' on node '{}'",
                        t.name, t.node
                    );

                    if let Some(client) = fetcher_pool.get_client(&t.node) {
                        let _ = client.verify_torrents(&[t.id]).await;
                        let _ = client.start_torrents(&[t.id], false).await;
                    }

                    verified_attempts.insert(t.compound_id.clone(), Instant::now());
                    continue;
                }

                // If verification was already attempted (or if unregistered/trumped):
                warn!(
                    "Pipeline resolving dead/corrupted torrent: '{}' on node '{}' (Reason: {})",
                    t.name, t.node, error_reason
                );

                // 1. Check if we have a grab record to trigger MediaReplacer in Sonarr / Radarr / Lidarr
                let mut re_searched = false;
                if config.rule_pipeline.auto_trigger_media_replacer {
                    if let Ok(Some(grab)) = db.find_arr_grab_by_hash_or_name(&t.hash_string, &t.name) {
                        if grab.item_type == "series" {
                            if let Some(ref primary) = config.sonarr.primary {
                                let ep_ids: Vec<i64> = grab.episode_ids
                                    .as_deref()
                                    .and_then(|s| serde_json::from_str(s).ok())
                                    .unwrap_or_default();

                                // Step 1: Find in Sonarr Queue, Blocklist release, and delete queue item
                                let queue_url = format!("{}/api/v3/queue?pageSize=1000&includeUnknownSeriesItems=true", primary.base_url.trim_end_matches('/'));
                                if let Ok(res) = client.get(&queue_url).header("X-Api-Key", &primary.api_key).send().await {
                                    if let Ok(queue_json) = res.json::<serde_json::Value>().await {
                                        let records = queue_json.get("records").and_then(|r| r.as_array()).or_else(|| queue_json.as_array());
                                        if let Some(recs) = records {
                                            for item in recs {
                                                let item_id = item.get("id").and_then(|v| v.as_i64());
                                                let download_id = item.get("downloadId").and_then(|v| v.as_str()).unwrap_or("");
                                                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");

                                                let matches_hash = !t.hash_string.is_empty() && download_id.to_lowercase().contains(&t.hash_string.to_lowercase());
                                                let matches_grab_dl = grab.download_id.as_deref().map(|d| download_id.eq_ignore_ascii_case(d)).unwrap_or(false);
                                                let matches_name = !t.name.is_empty() && title.eq_ignore_ascii_case(&t.name);
                                                let matches_grab_title = title.eq_ignore_ascii_case(&grab.release_title);

                                                if let (true, Some(q_id)) = (matches_hash || matches_grab_dl || matches_name || matches_grab_title, item_id) {
                                                    let del_url = format!("{}/api/v3/queue/{}?removeFromClient=false&blocklist=true&skipRedownload=false", primary.base_url.trim_end_matches('/'), q_id);
                                                    if let Ok(del_res) = client.delete(&del_url).header("X-Api-Key", &primary.api_key).send().await {
                                                        if del_res.status().is_success() {
                                                            info!("MediaReplacer blocklisted dead release and purged item #{} from Sonarr queue", q_id);
                                                            re_searched = true;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Step 2: Cutoff Resolution (Purge bad/corrupted imported file so cutoff profile allows re-search)
                                for ep_id in &ep_ids {
                                    let ep_url = format!("{}/api/v3/episode/{}", primary.base_url.trim_end_matches('/'), ep_id);
                                    if let Ok(ep_res) = client.get(&ep_url).header("X-Api-Key", &primary.api_key).send().await {
                                        if let Ok(ep_json) = ep_res.json::<serde_json::Value>().await {
                                            if ep_json.get("hasFile").and_then(|v| v.as_bool()).unwrap_or(false) {
                                                if let Some(file_id) = ep_json.get("episodeFileId").and_then(|v| v.as_i64()) {
                                                    if file_id > 0 {
                                                        let del_file_url = format!("{}/api/v3/episodefile/{}", primary.base_url.trim_end_matches('/'), file_id);
                                                        let _ = client.delete(&del_file_url).header("X-Api-Key", &primary.api_key).send().await;
                                                        info!("MediaReplacer purged corrupted episodeFile #{} from Sonarr library to clear quality cutoff lock", file_id);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Step 3: Trigger Explicit Re-Search Command
                                let cmd_url = format!("{}/api/v3/command", primary.base_url.trim_end_matches('/'));
                                let cmd_body = if !ep_ids.is_empty() {
                                    json!({ "name": "EpisodeSearch", "episodeIds": ep_ids })
                                } else if let Some(series_id) = grab.series_id {
                                    json!({ "name": "SeriesSearch", "seriesId": series_id })
                                } else {
                                    json!({})
                                };

                                if cmd_body.get("name").is_some() {
                                    match client.post(&cmd_url)
                                        .header("X-Api-Key", &primary.api_key)
                                        .json(&cmd_body)
                                        .send()
                                        .await
                                    {
                                        Ok(res) if res.status().is_success() => {
                                            info!("Successfully triggered Sonarr MediaReplacer search for '{}'", grab.release_title);
                                            let _ = db.mark_grab_status(&grab.id, "replaced", true);
                                            re_searched = true;
                                        }
                                        Ok(res) => {
                                            warn!("Sonarr MediaReplacer command returned status {}", res.status());
                                        }
                                        Err(e) => {
                                            error!("Failed to contact Sonarr primary for MediaReplacer: {}", e);
                                        }
                                    }
                                }
                            }
                        } else if grab.item_type == "movie" {
                            if let Some(ref primary) = config.radarr.primary {
                                if let Some(movie_id) = grab.movie_id {
                                    // Step 1: Find in Radarr Queue, Blocklist release, and delete queue item
                                    let queue_url = format!("{}/api/v3/queue?pageSize=1000", primary.base_url.trim_end_matches('/'));
                                    if let Ok(res) = client.get(&queue_url).header("X-Api-Key", &primary.api_key).send().await {
                                        if let Ok(queue_json) = res.json::<serde_json::Value>().await {
                                            let records = queue_json.get("records").and_then(|r| r.as_array()).or_else(|| queue_json.as_array());
                                            if let Some(recs) = records {
                                                for item in recs {
                                                    let item_id = item.get("id").and_then(|v| v.as_i64());
                                                    let download_id = item.get("downloadId").and_then(|v| v.as_str()).unwrap_or("");
                                                    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");

                                                    let matches_hash = !t.hash_string.is_empty() && download_id.to_lowercase().contains(&t.hash_string.to_lowercase());
                                                    let matches_grab_dl = grab.download_id.as_deref().map(|d| download_id.eq_ignore_ascii_case(d)).unwrap_or(false);
                                                    let matches_name = !t.name.is_empty() && title.eq_ignore_ascii_case(&t.name);
                                                    let matches_grab_title = title.eq_ignore_ascii_case(&grab.release_title);

                                                    if let (true, Some(q_id)) = (matches_hash || matches_grab_dl || matches_name || matches_grab_title, item_id) {
                                                        let del_url = format!("{}/api/v3/queue/{}?removeFromClient=false&blocklist=true&skipRedownload=false", primary.base_url.trim_end_matches('/'), q_id);
                                                        if let Ok(del_res) = client.delete(&del_url).header("X-Api-Key", &primary.api_key).send().await {
                                                            if del_res.status().is_success() {
                                                                info!("MediaReplacer blocklisted dead release and purged item #{} from Radarr queue", q_id);
                                                                re_searched = true;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Step 2: Cutoff Resolution (Purge bad/corrupted movie file)
                                    let movie_url = format!("{}/api/v3/movie/{}", primary.base_url.trim_end_matches('/'), movie_id);
                                    if let Ok(mov_res) = client.get(&movie_url).header("X-Api-Key", &primary.api_key).send().await {
                                        if let Ok(mov_json) = mov_res.json::<serde_json::Value>().await {
                                            if mov_json.get("hasFile").and_then(|v| v.as_bool()).unwrap_or(false) {
                                                if let Some(file_id) = mov_json.get("movieFileId").and_then(|v| v.as_i64()) {
                                                    if file_id > 0 {
                                                        let del_file_url = format!("{}/api/v3/moviefile/{}", primary.base_url.trim_end_matches('/'), file_id);
                                                        let _ = client.delete(&del_file_url).header("X-Api-Key", &primary.api_key).send().await;
                                                        info!("MediaReplacer purged corrupted movieFile #{} from Radarr library to clear quality cutoff lock", file_id);
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Step 3: Trigger Explicit MoviesSearch
                                    let cmd_url = format!("{}/api/v3/command", primary.base_url.trim_end_matches('/'));
                                    let cmd_body = json!({ "name": "MoviesSearch", "movieIds": [movie_id] });

                                    match client.post(&cmd_url)
                                        .header("X-Api-Key", &primary.api_key)
                                        .json(&cmd_body)
                                        .send()
                                        .await
                                    {
                                        Ok(res) if res.status().is_success() => {
                                            info!("Successfully triggered Radarr MediaReplacer search for '{}'", grab.release_title);
                                            let _ = db.mark_grab_status(&grab.id, "replaced", true);
                                            re_searched = true;
                                        }
                                        Ok(res) => {
                                            warn!("Radarr MediaReplacer command returned status {}", res.status());
                                        }
                                        Err(e) => {
                                            error!("Failed to contact Radarr primary for MediaReplacer: {}", e);
                                        }
                                    }
                                }
                            }
                        } else if grab.item_type == "music" {
                            if let Some(ref primary) = config.lidarr.primary {
                                // Step 1: Find in Lidarr Queue, Blocklist release, and delete queue item
                                let queue_url = format!("{}/api/v1/queue?pageSize=1000", primary.base_url.trim_end_matches('/'));
                                if let Ok(res) = client.get(&queue_url).header("X-Api-Key", &primary.api_key).send().await {
                                    if let Ok(queue_json) = res.json::<serde_json::Value>().await {
                                        let records = queue_json.get("records").and_then(|r| r.as_array()).or_else(|| queue_json.as_array());
                                        if let Some(recs) = records {
                                            for item in recs {
                                                let item_id = item.get("id").and_then(|v| v.as_i64());
                                                let download_id = item.get("downloadId").and_then(|v| v.as_str()).unwrap_or("");
                                                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");

                                                let matches_hash = !t.hash_string.is_empty() && download_id.to_lowercase().contains(&t.hash_string.to_lowercase());
                                                let matches_grab_dl = grab.download_id.as_deref().map(|d| download_id.eq_ignore_ascii_case(d)).unwrap_or(false);
                                                let matches_name = !t.name.is_empty() && title.eq_ignore_ascii_case(&t.name);
                                                let matches_grab_title = title.eq_ignore_ascii_case(&grab.release_title);

                                                if let (true, Some(q_id)) = (matches_hash || matches_grab_dl || matches_name || matches_grab_title, item_id) {
                                                    let del_url = format!("{}/api/v1/queue/{}?removeFromClient=false&blocklist=true&skipRedownload=false", primary.base_url.trim_end_matches('/'), q_id);
                                                    if let Ok(del_res) = client.delete(&del_url).header("X-Api-Key", &primary.api_key).send().await {
                                                        if del_res.status().is_success() {
                                                            info!("MediaReplacer blocklisted dead release and purged item #{} from Lidarr queue", q_id);
                                                            re_searched = true;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Step 2: Trigger Explicit AlbumSearch or ArtistSearch
                                let cmd_url = format!("{}/api/v1/command", primary.base_url.trim_end_matches('/'));
                                let cmd_body = if let Some(album_id) = grab.album_id {
                                    json!({ "name": "AlbumSearch", "albumIds": [album_id] })
                                } else if let Some(artist_id) = grab.artist_id {
                                    json!({ "name": "ArtistSearch", "artistId": artist_id })
                                } else {
                                    json!({})
                                };

                                if cmd_body.get("name").is_some() {
                                    match client.post(&cmd_url)
                                        .header("X-Api-Key", &primary.api_key)
                                        .json(&cmd_body)
                                        .send()
                                        .await
                                    {
                                        Ok(res) if res.status().is_success() => {
                                            info!("Successfully triggered Lidarr MediaReplacer search for '{}'", grab.release_title);
                                            let _ = db.mark_grab_status(&grab.id, "replaced", true);
                                            re_searched = true;
                                        }
                                        Ok(res) => {
                                            warn!("Lidarr MediaReplacer command returned status {}", res.status());
                                        }
                                        Err(e) => {
                                            error!("Failed to contact Lidarr primary for MediaReplacer: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 2. Auto-delete from Transmission if enabled (or if local data error cleanup enabled)
                if config.rule_pipeline.auto_delete_unregistered || (is_local_data_error && config.rule_pipeline.auto_fix_missing_data) {
                    info!("Auto-removing broken/dead torrent '{}' from node '{}'", t.name, t.node);
                    if let Some(client) = fetcher_pool.get_client(&t.node) {
                        let _ = client.remove_torrents(&[t.id], false).await;
                    }

                    let msg = if is_local_data_error {
                        if re_searched {
                            format!(
                                "🛠️ **Local Data Error Self-Corrected**: Torrent `{}` on node `{}` had missing files.\nRemoved broken torrent from daemon and triggered automatic replacement search in Sonarr/Radarr.",
                                t.name, t.node
                            )
                        } else {
                            format!(
                                "🛠️ **Local Data Error Cleared**: Torrent `{}` on node `{}` had missing files (_No data found_).\nRemoved broken torrent from daemon to clear error state.",
                                t.name, t.node
                            )
                        }
                    } else if re_searched {
                        format!(
                            "🔄 **MediaReplacer Activated**: Torrent `{}` on `{}` was trumped/unregistered (_{}_).\nPurged dead torrent and triggered automated re-search in Sonarr/Radarr.",
                            t.name, t.node, error_reason
                        )
                    } else {
                        format!(
                            "🗑️ **Error Pipeline**: Torrent `{}` on `{}` was unregistered (_{}_) and auto-purged.",
                            t.name, t.node, error_reason
                        )
                    };

                    NotificationManager::dispatch(&config.notifications, "torrent.error_fixed", None, "Conduit Self-Healing Alert", &msg, Some("#e63946")).await;
                    let _ = db.log_event(
                        "pipeline_error_fixed",
                        "warn",
                        &format!("Auto-resolved broken torrent '{}' on '{}' (reason: {})", t.name, t.node, error_reason),
                        None,
                    );
                }
            }
        }
    }
}
