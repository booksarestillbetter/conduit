// src/engines/watch_sync.rs
use crate::config::{AppConfig, ConfigManager, PlexNodeConfig};
use crate::db::Database;
use crate::engines::registry::{heartbeat_sleep, TaskHandle};
use crate::plex_client::{self, GuidMatch};
use crate::trakt_client;
use chrono::{DateTime, Utc};
use std::time::Duration;
use tracing::{debug, info, warn};

/// `sync_state` key for the watermark gating Trakt -> Plex push-back — see
/// `pull_watched_back_to_plex`. Deliberately advanced to the newest `last_watched_at` actually
/// seen in a cycle, not wall-clock "now": Trakt can expose a watch event slightly after it
/// actually happened, and using "now" as the watermark would permanently skip anything that
/// lagged behind a poll cycle.
const WATERMARK_KEY: &str = "trakt_watch_sync:last_checked";

pub async fn run_watch_sync_loop(config_mgr: ConfigManager, db: Database, handle: TaskHandle) {
    info!("Starting Plex & Trakt Watch History Sync Engine");

    loop {
        let config = config_mgr.get().await;
        if config.trakt.enabled && !config.trakt.client_id.is_empty() {
            if let Some(access_token) = config.trakt.access_token.clone().filter(|t| !t.is_empty()) {
                ensure_server_uuids(&config_mgr, &config).await;
                push_scrobbles_to_trakt(&config, &db, &access_token).await;
                if config.trakt.sync_watched_back_to_plex {
                    pull_watched_back_to_plex(&config, &db, &access_token).await;
                }
            }
        }

        let interval = config.trakt.sync_interval_mins.max(15);
        heartbeat_sleep(&handle, Duration::from_secs(interval * 60)).await;
    }
}

fn node_token<'a>(node: &'a PlexNodeConfig, shared_token: &'a str) -> &'a str {
    node.token_override.as_deref().filter(|t| !t.is_empty()).unwrap_or(shared_token)
}

/// Lazily detects and persists `server_uuid` for any node that opted into the sync but doesn't
/// have one yet — same one-time-detect-and-persist shape as `plex.client_identifier` in
/// `config/mod.rs::load_or_init`. Self-heals within one cycle of enabling `sync_watch_status`;
/// no manual "detect" step needed in Settings.
async fn ensure_server_uuids(config_mgr: &ConfigManager, config: &AppConfig) {
    let needs_detection = config.plex.nodes.iter().any(|n| n.sync_watch_status && n.server_uuid.is_none());
    if !needs_detection {
        return;
    }

    let mut updated = config.clone();
    let mut changed = false;
    for node in updated.plex.nodes.iter_mut() {
        if !node.sync_watch_status || node.server_uuid.is_some() {
            continue;
        }
        let token = node_token(node, &config.plex.token).to_string();
        if token.is_empty() {
            continue;
        }
        match plex_client::fetch_server_identity(&node.url, &token, &config.plex.client_identifier).await {
            Ok(uuid) => {
                info!("Detected Plex server identity for node '{}'", node.name);
                node.server_uuid = Some(uuid);
                changed = true;
            }
            Err(e) => {
                debug!("Failed to detect Plex server identity for node '{}' (will retry next cycle): {}", node.name, e);
            }
        }
    }

    if changed {
        if let Err(e) = config_mgr.update(updated).await {
            warn!("Failed to persist detected Plex server UUID(s): {}", e);
        }
    }
}

/// Push direction: local Plex scrobbles -> Trakt history. Batched into a single `/sync/history`
/// call (Trakt accepts arrays of movies/episodes in one request) instead of one round trip per
/// scrobble. Only scrobbles from `sync_watch_status=true` nodes are eligible — see
/// `Database::list_unsynced_scrobbles_from_nodes`.
async fn push_scrobbles_to_trakt(config: &AppConfig, db: &Database, access_token: &str) {
    let participating: Vec<String> = config.plex.nodes.iter().filter(|n| n.sync_watch_status).map(|n| n.name.clone()).collect();
    if participating.is_empty() {
        return;
    }

    let unsynced = match db.list_unsynced_scrobbles_from_nodes(&participating, 50) {
        Ok(list) => list,
        Err(e) => {
            debug!("Failed to query unsynced Plex scrobbles: {}", e);
            return;
        }
    };
    if unsynced.is_empty() {
        return;
    }
    info!("Found {} unsynced Plex scrobble(s) from participating server(s) to sync with Trakt", unsynced.len());

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    let mut movie_payload = Vec::new();
    let mut episode_payload = Vec::new();

    for scrobble in &unsynced {
        if scrobble.media_type == "movie" {
            let mut ids = serde_json::Map::new();
            if let Some(ref imdb) = scrobble.imdb_id {
                ids.insert("imdb".to_string(), serde_json::Value::String(imdb.clone()));
            }
            if let Some(tmdb) = scrobble.tmdb_id {
                ids.insert("tmdb".to_string(), serde_json::Value::Number(tmdb.into()));
            }
            movie_payload.push(serde_json::json!({
                "title": scrobble.title,
                "year": scrobble.year,
                "ids": ids,
                "watched_at": scrobble.created_at.to_rfc3339()
            }));
        } else if scrobble.media_type == "episode" {
            let mut ids = serde_json::Map::new();
            if let Some(ref imdb) = scrobble.imdb_id {
                ids.insert("imdb".to_string(), serde_json::Value::String(imdb.clone()));
            }
            if let Some(tmdb) = scrobble.tmdb_id {
                ids.insert("tmdb".to_string(), serde_json::Value::Number(tmdb.into()));
            }
            if let Some(tvdb) = scrobble.tvdb_id {
                ids.insert("tvdb".to_string(), serde_json::Value::Number(tvdb.into()));
            }
            episode_payload.push(serde_json::json!({
                "season": scrobble.season_number.unwrap_or(1),
                "number": scrobble.episode_number.unwrap_or(1),
                "ids": ids,
                "watched_at": scrobble.created_at.to_rfc3339()
            }));
        }
    }

    let trakt_body = serde_json::json!({
        "movies": movie_payload,
        "episodes": episode_payload
    });

    let trakt_res = client
        .post("https://api.trakt.tv/sync/history")
        .header("Content-Type", "application/json")
        .header("trakt-api-version", "2")
        .header("trakt-api-key", &config.trakt.client_id)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&trakt_body)
        .send()
        .await;

    match trakt_res {
        Ok(res) if res.status().is_success() => {
            info!("Successfully synced {} Plex scrobble(s) to Trakt in one batch", unsynced.len());
            for scrobble in &unsynced {
                let _ = db.mark_plex_scrobble_trakt_synced(&scrobble.id);
            }
        }
        Ok(res) => {
            debug!("Trakt batch sync response HTTP {} for {} scrobble(s)", res.status(), unsynced.len());
        }
        Err(e) => {
            debug!("Failed to sync scrobbles to Trakt: {}", e);
        }
    }
}

fn advance_watermark(current_max: &mut Option<DateTime<Utc>>, candidate: Option<DateTime<Utc>>) {
    if let Some(c) = candidate {
        if current_max.is_none_or(|m| c > m) {
            *current_max = Some(c);
        }
    }
}

/// Pull direction: Trakt's watched history -> every participating Plex server, marking watched
/// items on servers that don't already know about them. Only runs when
/// `trakt.sync_watched_back_to_plex` is explicitly enabled (separate from, and off by default
/// unlike, the push direction above) — this is Conduit writing to a live Plex server on its own
/// initiative based on a best-effort imdb/tmdb/tvdb ID match against that server's own library.
async fn pull_watched_back_to_plex(config: &AppConfig, db: &Database, access_token: &str) {
    let participating: Vec<&PlexNodeConfig> = config.plex.nodes.iter().filter(|n| n.sync_watch_status).collect();
    if participating.is_empty() {
        return;
    }

    let watermark: Option<DateTime<Utc>> = db.get_sync_state(WATERMARK_KEY).ok().flatten()
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    let mut new_watermark = watermark;

    match trakt_client::fetch_watched_movies(&config.trakt.client_id, access_token).await {
        Ok(movies) => {
            for entry in movies {
                let watched_at = DateTime::parse_from_rfc3339(&entry.last_watched_at).ok().map(|dt| dt.with_timezone(&Utc));
                if let (Some(wm), Some(wa)) = (watermark, watched_at) {
                    if wa <= wm {
                        continue;
                    }
                }
                advance_watermark(&mut new_watermark, watched_at);

                let target = GuidMatch {
                    imdb_id: entry.movie.ids.imdb.as_deref(),
                    tmdb_id: entry.movie.ids.tmdb,
                    tvdb_id: None,
                };
                if target.imdb_id.is_none() && target.tmdb_id.is_none() {
                    continue;
                }

                for node in &participating {
                    let token = node_token(node, &config.plex.token);
                    if token.is_empty() {
                        continue;
                    }
                    match plex_client::find_movie_rating_key(&node.url, token, &config.plex.client_identifier, &target).await {
                        Ok(Some(rating_key)) => {
                            match plex_client::mark_watched(&node.url, token, &config.plex.client_identifier, &rating_key).await {
                                Ok(()) => {
                                    let _ = db.log_event("watch_sync", "info", &format!("Marked a movie watched on '{}' via Trakt sync", node.name), None);
                                }
                                Err(e) => warn!("Failed to mark movie watched on Plex node '{}': {}", node.name, e),
                            }
                        }
                        Ok(None) => {
                            debug!("Plex node '{}' has no matching movie for this Trakt watched entry — skipping", node.name);
                        }
                        Err(e) => {
                            debug!("Failed to search Plex node '{}' library for a movie match: {}", node.name, e);
                        }
                    }
                }
            }
        }
        Err(e) => debug!("Failed to fetch Trakt watched movies: {}", e),
    }

    match trakt_client::fetch_watched_shows(&config.trakt.client_id, access_token).await {
        Ok(shows) => {
            for entry in shows {
                let show_target = GuidMatch {
                    imdb_id: entry.show.ids.imdb.as_deref(),
                    tmdb_id: entry.show.ids.tmdb,
                    tvdb_id: entry.show.ids.tvdb,
                };
                if show_target.imdb_id.is_none() && show_target.tmdb_id.is_none() && show_target.tvdb_id.is_none() {
                    continue;
                }

                for season in &entry.seasons {
                    for ep in &season.episodes {
                        let watched_at = DateTime::parse_from_rfc3339(&ep.last_watched_at).ok().map(|dt| dt.with_timezone(&Utc));
                        if let (Some(wm), Some(wa)) = (watermark, watched_at) {
                            if wa <= wm {
                                continue;
                            }
                        }
                        advance_watermark(&mut new_watermark, watched_at);

                        for node in &participating {
                            let token = node_token(node, &config.plex.token);
                            if token.is_empty() {
                                continue;
                            }
                            match plex_client::find_episode_rating_key(&node.url, token, &config.plex.client_identifier, &show_target, season.number, ep.number).await {
                                Ok(Some(rating_key)) => {
                                    match plex_client::mark_watched(&node.url, token, &config.plex.client_identifier, &rating_key).await {
                                        Ok(()) => {
                                            let msg = format!("Marked S{:02}E{:02} watched on '{}' via Trakt sync", season.number, ep.number, node.name);
                                            let _ = db.log_event("watch_sync", "info", &msg, None);
                                        }
                                        Err(e) => warn!("Failed to mark episode watched on Plex node '{}': {}", node.name, e),
                                    }
                                }
                                Ok(None) => {
                                    debug!("Plex node '{}' has no matching S{:02}E{:02} for this Trakt watched entry — skipping", node.name, season.number, ep.number);
                                }
                                Err(e) => {
                                    debug!("Failed to search Plex node '{}' library for an episode match: {}", node.name, e);
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => debug!("Failed to fetch Trakt watched shows: {}", e),
    }

    if let Some(wm) = new_watermark {
        let _ = db.set_sync_state(WATERMARK_KEY, &wm.to_rfc3339());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn dt(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    #[test]
    fn advances_to_the_latest_candidate() {
        let mut current = None;
        advance_watermark(&mut current, Some(dt(100)));
        assert_eq!(current, Some(dt(100)));
        advance_watermark(&mut current, Some(dt(200)));
        assert_eq!(current, Some(dt(200)));
    }

    #[test]
    fn never_moves_backward() {
        // A later-processed entry with an earlier watched_at (e.g. Trakt returning results
        // out of order) must not regress the watermark past what's already been seen.
        let mut current = Some(dt(200));
        advance_watermark(&mut current, Some(dt(50)));
        assert_eq!(current, Some(dt(200)));
    }

    #[test]
    fn none_candidate_is_a_no_op() {
        let mut current = Some(dt(100));
        advance_watermark(&mut current, None);
        assert_eq!(current, Some(dt(100)));
    }
}
