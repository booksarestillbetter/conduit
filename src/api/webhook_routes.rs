// src/api/webhook_routes.rs
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::config::ConfigManager;
use crate::db::{ArrGrabRecord, Database, OmbiRequestRecord, PlexScrobbleRecord};

#[derive(Debug, Deserialize)]
pub struct ListScrobblesParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub event: Option<String>,
    pub user: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListOmbiParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub media_type: Option<String>,
    pub status: Option<String>,
}

/// Helper to parse Plex webhook payload from either multipart/form-data or application/json
fn extract_plex_json(headers: &HeaderMap, body_bytes: &[u8]) -> Option<serde_json::Value> {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // 1. Direct JSON
    if content_type.contains("application/json") || (!body_bytes.is_empty() && (body_bytes[0] == b'{' || body_bytes[0] == b'[')) {
        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(body_bytes) {
            return Some(val);
        }
    }

    // 2. Multipart form data (Plex standard: form field `payload` containing JSON)
    if let Ok(body_str) = std::str::from_utf8(body_bytes) {
        // Look for payload="{...}" or standard urlencoded payload=
        if let Some(pos) = body_str.find("name=\"payload\"") {
            let rest = &body_str[pos..];
            if let Some(start_json) = rest.find('{') {
                if let Some(end_json) = rest.rfind('}') {
                    let json_slice = &rest[start_json..=end_json];
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_slice) {
                        return Some(val);
                    }
                }
            }
        }

        // 3. Fallback: urlencoded payload=
        if let Some(encoded) = body_str.strip_prefix("payload=") {
            let decoded = urlencoding_decode(encoded);
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&decoded) {
                return Some(val);
            }
        }
    }

    None
}

fn urlencoding_decode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

#[utoipa::path(
    post,
    path = "/api/plex/inbound",
    tag = "Integrations",
    summary = "Plex Media Server Webhook Inbound",
    description = "Ingests Plex media playback events (media.scrobble, media.play, library.new) from Plex Media Server webhooks.",
    request_body = String,
    responses(
        (status = 200, description = "Webhook ingested successfully"),
        (status = 400, description = "Invalid payload")
    )
)]
pub async fn plex_inbound(
    State(config_mgr): State<ConfigManager>,
    State(db): State<Database>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = config_mgr.get().await;
    if !crate::api::arr_routes::verify_webhook_secret(&headers, Some(&config.plex.token)) {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"status": "error", "message": "Invalid Plex webhook token"}))));
    }

    let payload = extract_plex_json(&headers, body.as_bytes()).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "message": "Failed to parse Plex webhook payload" })),
        )
    })?;

    let event = payload.get("event").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let user_name = payload
        .get("Account")
        .and_then(|a| a.get("title"))
        .or_else(|| payload.get("user").and_then(|u| u.get("title")))
        .and_then(|t| t.as_str())
        .unwrap_or("PlexUser")
        .to_string();

    let meta = payload.get("Metadata").cloned().unwrap_or(json!({}));
    let media_type = meta.get("type").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let item_title = meta.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled").to_string();
    let series_title = meta.get("grandparentTitle").and_then(|v| v.as_str()).map(|s| s.to_string());
    let season_number = meta.get("parentIndex").and_then(|v| v.as_i64()).map(|n| n as i32);
    let episode_number = meta.get("index").and_then(|v| v.as_i64()).map(|n| n as i32);
    let year = meta.get("year").and_then(|v| v.as_i64()).map(|n| n as i32);
    let rating_key = meta.get("ratingKey").and_then(|v| v.as_str()).map(|s| s.to_string());
    let duration_ms = meta.get("duration").and_then(|v| v.as_i64());
    let view_offset_ms = meta.get("viewOffset").and_then(|v| v.as_i64());

    // Extract GUIDs (IMDb, TMDb, TVDb)
    let mut imdb_id = None;
    let mut tmdb_id = None;
    let mut tvdb_id = None;

    if let Some(guids) = meta.get("Guid").and_then(|v| v.as_array()) {
        for g in guids {
            if let Some(id_str) = g.get("id").and_then(|v| v.as_str()) {
                if id_str.starts_with("imdb://") {
                    imdb_id = Some(id_str.trim_start_matches("imdb://").to_string());
                } else if id_str.starts_with("tmdb://") {
                    tmdb_id = id_str.trim_start_matches("tmdb://").parse::<i64>().ok();
                } else if id_str.starts_with("tvdb://") {
                    tvdb_id = id_str.trim_start_matches("tvdb://").parse::<i64>().ok();
                }
            }
        }
    }

    // Every Plex webhook carries a Server object (docs/webhook_reference.md §4.4) — match its
    // permanent machine identifier (preferred) or display title against configured Plex nodes so
    // engines::watch_sync knows which server this scrobble came from, and can decide whether that
    // server opted into the watch-status sync at all.
    let webhook_server_uuid = payload.get("Server").and_then(|s| s.get("uuid")).and_then(|v| v.as_str());
    let webhook_server_title = payload.get("Server").and_then(|s| s.get("title")).and_then(|v| v.as_str());
    let source_node_name = config.plex.nodes.iter()
        .find(|n| webhook_server_uuid.is_some() && n.server_uuid.as_deref() == webhook_server_uuid)
        .or_else(|| config.plex.nodes.iter().find(|n| Some(n.name.as_str()) == webhook_server_title))
        .map(|n| n.name.clone());

    let record_id = uuid::Uuid::new_v4().to_string();
    let scrobble_record = PlexScrobbleRecord {
        id: record_id.clone(),
        event: event.clone(),
        user_name: user_name.clone(),
        media_type: media_type.clone(),
        title: item_title.clone(),
        series_title: series_title.clone(),
        season_number,
        episode_number,
        year,
        imdb_id: imdb_id.clone(),
        tmdb_id,
        tvdb_id,
        rating_key,
        duration_ms,
        view_offset_ms,
        trakt_synced: false,
        raw_json: payload.to_string(),
        created_at: Utc::now(),
        source_node_name,
    };

    let config = config_mgr.get().await;

    if config.plex.digest_scrobbles {
        if let Err(e) = db.save_plex_scrobble(&scrobble_record) {
            warn!("Failed to save Plex scrobble to database: {}", e);
        }
    }

    let _ = db.log_event(
        &format!("plex.{}", event),
        "INFO",
        &format!("Plex Webhook: {} by {} ({})", event, user_name, item_title),
        Some(&payload.to_string()),
    );

    // admin.database.corrupted is Plex's highest-severity event (server-level, not tied to a
    // media item) — always alert on it regardless of the scrobble/download notification gates,
    // same as Sonarr/Radarr/Lidarr's unconditional Health-event handling.
    if event == "admin.database.corrupted" {
        let server_title = payload.get("Server").and_then(|s| s.get("title")).and_then(|v| v.as_str()).unwrap_or("Plex Server");
        let _ = db.log_event("plex.admin", "error", &format!("Plex database corruption detected on {}", server_title), Some(&payload.to_string()));
        let notify_config = config.notifications.clone();
        let server_str = server_title.to_string();
        tokio::spawn(async move {
            let _ = crate::notify::NotificationManager::dispatch_rich(
                &notify_config,
                "plex.admin",
                None,
                &format!("🚨 Conduit Alert • Database Corruption on {}", server_str),
                "Plex reported that its database is corrupted. This usually requires restoring from a backup — check the Plex server logs immediately.",
                None,
                vec![("Server", server_str.as_str(), true)],
                Some("#EF4444"),
            ).await;
        });
    }

    // Which Plex event types are notify-eligible at all — media.play/pause/resume/stop,
    // admin.database.backup, and device.new stay logged-only by design (too frequent/low-signal
    // for a notification). Category filtering (does any target actually want "scrobble"/
    // "download" events) now happens per-target inside dispatch itself.
    let should_notify = matches!(event.as_str(), "media.scrobble" | "library.new" | "media.rate");

    if should_notify {
        let display_title = if let Some(ref show) = series_title {
            format!("{} - S{:02}E{:02} {}", show, season_number.unwrap_or(1), episode_number.unwrap_or(1), item_title)
        } else {
            format!("{} ({})", item_title, year.unwrap_or(0))
        };

        let card_title = if event == "media.scrobble" {
            format!("🐕🍿 Conduit Watched • {}", display_title)
        } else if event == "media.rate" {
            format!("🐕⭐ Conduit Noticed a Rating • {}", display_title)
        } else {
            format!("🐕📚 Plex Indexed • {}", display_title)
        };

        let user_rating = meta.get("userRating").and_then(|v| v.as_f64());
        let summary = if event == "media.rate" {
            match user_rating {
                Some(r) => format!("User **{}** rated **{}** {:.1}⭐ on Plex.", user_name, display_title, r),
                None => format!("User **{}** rated **{}** on Plex.", user_name, display_title),
            }
        } else {
            format!("User **{}** completed watching **{}** via Plex Media Server.", user_name, display_title)
        };
        let user_str = user_name.clone();
        let event_str = event.clone();
        let media_str = media_type.clone();
        let notify_config = config.notifications.clone();
        let summary_meta = meta.get("summary").and_then(|v| v.as_str()).map(|s| s.to_string());

        tokio::spawn(async move {
            let mut fields = vec![
                ("User", user_str.as_str(), true),
                ("Event", event_str.as_str(), true),
                ("Media", media_str.as_str(), true),
            ];
            if let Some(ref s) = summary_meta {
                fields.push(("Plot", s.as_str(), false));
            }

            let _ = crate::notify::NotificationManager::dispatch_rich(
                &notify_config,
                "plex.scrobble",
                None,
                &card_title,
                &summary,
                None,
                fields,
                Some("#8B5CF6"), // Purple
            )
            .await;
        });
    }

    info!("Ingested Plex webhook: event='{}', user='{}', title='{}'", event, user_name, item_title);

    Ok(Json(json!({
        "status": "ok",
        "scrobble_id": record_id,
        "event": event,
        "user": user_name,
        "title": item_title,
    })))
}

#[utoipa::path(
    get,
    path = "/api/plex/scrobbles",
    tag = "Integrations",
    summary = "List Plex Scrobble History",
    description = "Retrieves recent Plex playback scrobble events with optional event and user filtering.",
    params(
        ("limit" = Option<usize>, Query, description = "Max items to return (default: 50)"),
        ("offset" = Option<usize>, Query, description = "Offset index (default: 0)"),
        ("event" = Option<String>, Query, description = "Filter by event type (e.g. media.scrobble, media.play)"),
        ("user" = Option<String>, Query, description = "Filter by user name")
    ),
    responses(
        (status = 200, description = "List of scrobble records")
    )
)]
pub async fn list_plex_scrobbles(
    State(db): State<Database>,
    Query(params): Query<ListScrobblesParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    match db.list_plex_scrobbles(limit, offset, params.event.as_deref(), params.user.as_deref()) {
        Ok((items, total)) => Ok(Json(json!({
            "items": items,
            "total": total,
            "limit": limit,
            "offset": offset,
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to list Plex scrobbles: {}", e) })),
        )),
    }
}

#[utoipa::path(
    post,
    path = "/api/ombi/inbound",
    tag = "Integrations",
    summary = "Ombi Request Webhook Inbound",
    description = "Ingests media requests and status updates from Ombi request platform.",
    responses(
        (status = 200, description = "Ombi webhook ingested successfully"),
        (status = 400, description = "Invalid Ombi payload")
    )
)]
pub async fn ombi_inbound(
    State(config_mgr): State<ConfigManager>,
    State(db): State<Database>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = config_mgr.get().await;
    if !crate::api::arr_routes::verify_webhook_secret(&headers, config.ombi.webhook_secret.as_deref()) {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"status": "error", "message": "Invalid Ombi webhook secret"}))));
    }

    let event_type = payload
        .get("notificationType")
        .or_else(|| payload.get("event"))
        .or_else(|| payload.get("eventType"))
        .and_then(|v| v.as_str())
        .unwrap_or("NewRequest")
        .to_string();

    let requested_by = payload
        .get("requestedUser")
        .or_else(|| payload.get("requestedBy"))
        .or_else(|| payload.get("user"))
        .and_then(|v| v.as_str())
        .unwrap_or("OmbiUser")
        .to_string();

    let title = payload
        .get("title")
        .or_else(|| payload.get("mediaTitle"))
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled Request")
        .to_string();

    // Ombi's real `type` field is a humanized string ("Movie", "TV Show", "Album"), not the
    // normalized "movie"/"tv"/"music" values the rest of Conduit expects.
    let raw_media_type = payload
        .get("type")
        .or_else(|| payload.get("mediaType"))
        .and_then(|v| v.as_str())
        .unwrap_or("movie")
        .to_lowercase();
    let media_type = if raw_media_type.contains("tv") {
        "tv".to_string()
    } else if raw_media_type.contains("album") || raw_media_type.contains("music") {
        "music".to_string()
    } else {
        "movie".to_string()
    };

    let year = payload
        .get("year")
        .or_else(|| payload.get("releaseDate"))
        .and_then(|v| {
            if let Some(n) = v.as_i64() {
                Some(n as i32)
            } else if let Some(s) = v.as_str() {
                s.chars().take(4).collect::<String>().parse::<i32>().ok()
            } else {
                None
            }
        });

    let overview = payload
        .get("overview")
        .or_else(|| payload.get("description"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Ombi's actual webhook field is `posterImage` (a fully-resolved, ready-to-use URL) —
    // `posterPath`/`poster`/`banner` are kept as fallbacks for other senders but never match Ombi.
    let poster_url = payload
        .get("posterImage")
        .or_else(|| payload.get("posterPath"))
        .or_else(|| payload.get("poster"))
        .or_else(|| payload.get("banner"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Ombi has no separate imdbId/tmdbId/tvDbId fields — it sends a single overloaded
    // `providerId` whose meaning depends on media type and Ombi's own configured provider.
    let provider_id = payload.get("providerId").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let provider_id_num = provider_id.and_then(|s| s.parse::<i64>().ok());

    let imdb_id = payload.get("imdbId").and_then(|v| v.as_str()).map(|s| s.to_string());
    let tmdb_id = payload.get("theMovieDbId").or_else(|| payload.get("tmdbId")).and_then(|v| v.as_i64())
        .or_else(|| if media_type == "movie" { provider_id_num } else { None });
    let tvdb_id = payload.get("tvDbId").or_else(|| payload.get("tvdbId")).and_then(|v| v.as_i64())
        .or_else(|| if media_type == "tv" { provider_id_num } else { None });

    let deny_reason = payload.get("denyReason").and_then(|v| v.as_str()).filter(|s| !s.is_empty());

    let status = match event_type.to_lowercase().as_str() {
        "newrequest" | "requestadded" => "pending",
        "requestapproved" | "approved" => "approved",
        "requestavailable" | "available" | "partiallyavailable" => "available",
        // Ombi's real enum value is `RequestDeclined` ("requestdeclined") — "requestdenied" is
        // kept as a fallback alias but never actually matches a real Ombi payload.
        "requestdeclined" | "requestdenied" | "denied" => "denied",
        _ => "pending",
    };

    let req_id = payload
        .get("requestId")
        .or_else(|| payload.get("id"))
        .and_then(|v| {
            if let Some(n) = v.as_i64() {
                Some(n.to_string())
            } else {
                v.as_str().map(|s| s.to_string())
            }
        })
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let existing_ombi = db.find_ombi_request_by_id_or_title(&req_id, &title)
        .ok()
        .flatten()
        .or_else(|| db.find_ombi_request_by_title_or_ids(&title, imdb_id.as_deref(), tmdb_id, tvdb_id).ok().flatten());
    let existing_post_id = existing_ombi.as_ref().and_then(|r| r.mattermost_post_id.clone())
        .or_else(|| db.find_arr_grab_by_hash_or_name("", &title).ok().flatten().and_then(|g| g.mattermost_post_id));

    let request_record = OmbiRequestRecord {
        id: req_id.clone(),
        event_type: event_type.clone(),
        requested_by: requested_by.clone(),
        media_type: media_type.clone(),
        title: title.clone(),
        year,
        overview: overview.clone(),
        poster_url: poster_url.clone(),
        imdb_id: imdb_id.clone(),
        tmdb_id,
        tvdb_id,
        status: status.to_string(),
        mattermost_post_id: existing_post_id.clone(),
        raw_json: payload.to_string(),
        created_at: existing_ombi.as_ref().map(|o| o.created_at).unwrap_or_else(Utc::now),
        updated_at: Utc::now(),
    };

    if let Err(e) = db.save_ombi_request(&request_record) {
        warn!("Failed to save Ombi request to database: {}", e);
    }

    // Double log so audit filters for both "ombi" and "webhook" catch the event!
    let raw_payload_str = serde_json::to_string(&payload).unwrap_or_default();
    let _ = db.log_event(
        "ombi",
        "info",
        &format!("Kibble [{}] for '{}' (User: {})", event_type, title, requested_by),
        Some(&raw_payload_str),
    );
    let _ = db.log_event(
        "webhook",
        "info",
        &format!("Webhook Inbound: Kibble Request [{}] for '{}'", event_type, title),
        Some(&raw_payload_str),
    );

    // Dispatch dog-themed notification to Mattermost / Discord
    let (action_label, color_hex) = match event_type.to_lowercase().as_str() {
        "newrequest" | "requestadded" => ("🐕🦴 Conduit Sniffing for Kibble", "#EC4899"), // Pink
        "requestapproved" | "approved" => ("🐕🥣 Conduit Approved Kibble Bowl", "#8B5CF6"), // Purple
        "requestavailable" | "available" => ("🐕🎉 Conduit Delivered Kibble Bowl", "#10B981"), // Green
        "partiallyavailable" => ("🐕🎾 Conduit Fetched Part of the Kibble", "#0EA5E9"), // Blue
        "requestdeclined" | "requestdenied" | "denied" => ("🐕🚫 Conduit Dropped Kibble Request", "#EF4444"), // Red
        "issue" | "issueinprogress" => ("🐕🩹 Conduit Flagged a Kibble Issue", "#F59E0B"), // Amber
        "issuecomment" => ("🐕💬 Kibble Issue Update", "#F59E0B"),
        "issueresolved" => ("🐕✅ Kibble Issue Resolved", "#10B981"),
        _ => ("🐕 Conduit Kibble Update", "#F59E0B"),
    };

    let year_suffix = year.map(|y| format!(" ({})", y)).unwrap_or_default();
    let card_title = format!("{} • {}{}", action_label, title, year_suffix);
    let summary_text = if let Some(reason) = deny_reason {
        format!("Request declined: {}", reason)
    } else {
        overview.clone().unwrap_or_else(|| format!("User {} ordered kibble for {} '{}'.", requested_by, media_type, title))
    };
    let req_by_str = requested_by.clone();
    let status_str = status.to_string();
    let mtype_str = media_type.clone();
    let deny_reason_str = deny_reason.map(|s| s.to_string());
    let title_clone = title.clone();
    let overview_clone = overview.clone();
    let poster_url_clone = poster_url.clone();
    let imdb_id_clone = imdb_id.clone();
    let payload_str = payload.to_string();
    let config = config_mgr.get().await;
    let db_clone = db.clone();
    let req_id_clone = req_id.clone();
    let existing_post_id_clone = existing_post_id.clone();
    let event_type_clone = event_type.clone();

    tokio::spawn(async move {
        let mut fields = vec![
            ("Requested By", req_by_str.as_str(), true),
            ("Status", status_str.as_str(), true),
            ("Type", mtype_str.as_str(), true),
        ];
        if let Some(ref reason) = deny_reason_str {
            fields.push(("Deny Reason", reason.as_str(), false));
        }

        let post_id_opt = crate::notify::NotificationManager::dispatch_rich_or_update(
            &config.notifications,
            "ombi.request",
            None,
            existing_post_id_clone.as_deref(),
            None,
            &card_title,
            &summary_text,
            poster_url_clone.as_deref(),
            fields,
            Some(color_hex),
        )
        .await;

        let final_post_id = post_id_opt.or(existing_post_id_clone);

        if let Some(ref post_id) = final_post_id {
            if let Ok(Some(mut rec)) = db_clone.find_ombi_request_by_id_or_title(&req_id_clone, "") {
                rec.mattermost_post_id = Some(post_id.clone());
                let _ = db_clone.save_ombi_request(&rec);
            }

            // Also ensure an initial grab record exists or is updated with this post_id so Arr grabs inherit it seamlessly
            let grab_id = format!("ombi_{}", req_id_clone);
            let existing_grab = db_clone.find_arr_grab_by_ids_or_title(None, None, imdb_id_clone.as_deref(), tmdb_id, tvdb_id, Some(&title_clone)).ok().flatten();
            let grab_rec = ArrGrabRecord {
                id: existing_grab.as_ref().map(|g| g.id.clone()).unwrap_or(grab_id),
                scene_name: title_clone.clone(),
                release_title: title_clone.clone(),
                event_type: "OmbiRequest".to_string(),
                item_type: mtype_str.clone(),
                series_id: None,
                movie_id: None,
                artist_id: None,
                album_id: None,
                zone_id: None,
                season_number: None,
                episode_numbers: None,
                episode_ids: None,
                indexer: None,
                download_client: None,
                download_id: None,
                status: existing_grab.as_ref().map(|g| g.status.clone()).unwrap_or_else(|| "requested".to_string()),
                re_searched: false,
                re_search_count: 0,
                title: Some(title_clone.clone()),
                year: year.map(|y| y as i64),
                overview: overview_clone,
                poster_url: poster_url_clone,
                genres: None,
                quality: None,
                size_bytes: None,
                imdb_id: imdb_id_clone,
                tmdb_id,
                tvdb_id,
                runtime_mins: None,
                rating: None,
                mattermost_post_id: Some(post_id.clone()),
                payload_json: payload_str,
                created_at: existing_grab.as_ref().map(|g| g.created_at).unwrap_or_else(Utc::now),
                updated_at: Utc::now(),
            };
            let _ = db_clone.save_arr_grab(&grab_rec);

            // If updating an existing card for approved/available, thread a comment
            if event_type_clone.eq_ignore_ascii_case("requestapproved") || event_type_clone.eq_ignore_ascii_case("requestavailable") {
                let thread_msg = format!("🐕 Kibble bowl update: `{}` by **{}** -> Status: `{}`", event_type_clone, req_by_str, status_str);
                let _ = crate::notify::NotificationManager::dispatch_rich_or_update(
                    &config.notifications,
                    "ombi.request.detail",
                    None,
                    None,
                    Some(post_id.as_str()),
                    "Kibble Detail",
                    &thread_msg,
                    None,
                    Vec::new(),
                    Some(color_hex),
                ).await;
            }
        }
    });

    info!("Ingested Ombi webhook: event='{}', user='{}', title='{}'", event_type, requested_by, title);

    Ok(Json(json!({
        "status": "ok",
        "request_id": req_id,
        "event": event_type,
        "requested_by": requested_by,
        "title": title,
    })))
}

#[utoipa::path(
    get,
    path = "/api/ombi/requests",
    tag = "Integrations",
    summary = "List Ombi Requests",
    description = "Retrieves recent Ombi media requests with optional media type and status filtering.",
    params(
        ("limit" = Option<usize>, Query, description = "Max items to return (default: 50)"),
        ("offset" = Option<usize>, Query, description = "Offset index (default: 0)"),
        ("media_type" = Option<String>, Query, description = "Filter by media type (e.g. movie, tv, music)"),
        ("status" = Option<String>, Query, description = "Filter by status (pending, approved, available, denied)")
    ),
    responses(
        (status = 200, description = "List of Ombi requests")
    )
)]
pub async fn list_ombi_requests(
    State(db): State<Database>,
    Query(params): Query<ListOmbiParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    match db.list_ombi_requests(limit, offset, params.media_type.as_deref(), params.status.as_deref()) {
        Ok((items, total)) => Ok(Json(json!({
            "items": items,
            "total": total,
            "limit": limit,
            "offset": offset,
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to list Ombi requests: {}", e) })),
        )),
    }
}

// ---------------------------------------------------------------------------
// Bazarr Subtitle Webhooks
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/bazarr/inbound",
    tag = "Integrations",
    summary = "Bazarr Subtitle Webhook Inbound",
    description = "Ingests subtitle acquisition and synchronization events from Bazarr.",
    request_body = String,
    responses(
        (status = 200, description = "Webhook ingested successfully"),
        (status = 400, description = "Invalid payload")
    )
)]
pub async fn bazarr_inbound(
    State(state): State<crate::auth::AppState>,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let payload: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Invalid JSON payload: {}", e)}))))?;

    let event_type = payload.get("eventType").or_else(|| payload.get("event")).and_then(|v| v.as_str()).unwrap_or("Download");
    let series_title = payload.get("seriesTitle").or_else(|| payload.get("series_title")).and_then(|v| v.as_str());
    let movie_title = payload.get("movieTitle").or_else(|| payload.get("movie_title")).and_then(|v| v.as_str());
    let title = series_title.or(movie_title).unwrap_or("Media");
    let language = payload.get("subtitleLanguage").or_else(|| payload.get("language")).and_then(|v| v.as_str()).unwrap_or("English");
    let provider = payload.get("provider").and_then(|v| v.as_str()).unwrap_or("Bazarr");

    let log_msg = format!("Bazarr {}: Subtitles for '{}' ({}) via {}", event_type, title, language, provider);
    let _ = state.db.log_event("bazarr", "info", &log_msg, None);

    let config = state.config.get().await;
    let notif_title = format!("💬 Conduit Subtitles • {}", title);
    let notif_body = format!("**{}** subtitles acquired via **{}** for `{}`", language, provider, title);

    let notif_config = config.notifications.clone();
    tokio::spawn(async move {
        crate::notify::NotificationManager::dispatch(
            &notif_config,
            "bazarr.download",
            None,
            &notif_title,
            &notif_body,
            Some("#3B82F6"),
        ).await;
    });

    info!("Ingested Bazarr webhook: event='{}', title='{}', lang='{}'", event_type, title, language);
    Ok(Json(json!({"status": "ok", "event": event_type, "title": title, "language": language})))
}

// ---------------------------------------------------------------------------
// Overseerr / Jellyseerr Webhooks
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/overseerr/inbound",
    tag = "Integrations",
    summary = "Overseerr/Jellyseerr Webhook Inbound",
    description = "Ingests media requests and approval notifications from Overseerr and Jellyseerr.",
    request_body = String,
    responses(
        (status = 200, description = "Webhook ingested successfully"),
        (status = 400, description = "Invalid payload")
    )
)]
pub async fn overseerr_inbound(
    State(state): State<crate::auth::AppState>,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let payload: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Invalid JSON payload: {}", e)}))))?;

    let event_type = payload.get("notification_type").or_else(|| payload.get("event")).and_then(|v| v.as_str()).unwrap_or("MEDIA_PENDING");
    let subject = payload.get("subject").and_then(|v| v.as_str()).unwrap_or("Media Request");
    let message = payload.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let media = payload.get("media");
    let media_type = media.and_then(|m| m.get("media_type")).and_then(|v| v.as_str()).unwrap_or("movie");
    let tmdb_id = media.and_then(|m| m.get("tmdbId")).and_then(|v| v.as_i64());
    let tvdb_id = media.and_then(|m| m.get("tvdbId")).and_then(|v| v.as_i64());
    let requested_by = payload.get("request").and_then(|r| r.get("requestedBy_username")).and_then(|v| v.as_str()).unwrap_or("Overseerr User");

    let req_id = format!("overseerr-{}", uuid::Uuid::new_v4());
    let status_str = match event_type {
        "MEDIA_APPROVED" | "MEDIA_AUTO_APPROVED" => "approved",
        "MEDIA_AVAILABLE" => "available",
        "MEDIA_FAILED" | "MEDIA_DECLINED" => "denied",
        _ => "pending",
    };

    let ombi_rec = OmbiRequestRecord {
        id: req_id.clone(),
        event_type: event_type.to_string(),
        requested_by: requested_by.to_string(),
        media_type: media_type.to_string(),
        title: subject.to_string(),
        year: None,
        overview: Some(message.to_string()),
        poster_url: payload.get("image").and_then(|v| v.as_str()).map(|s| s.to_string()),
        imdb_id: None,
        tmdb_id,
        tvdb_id,
        status: status_str.to_string(),
        mattermost_post_id: None,
        raw_json: body,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let _ = state.db.save_ombi_request(&ombi_rec);

    let log_msg = format!("Overseerr {}: '{}' by {}", event_type, subject, requested_by);
    let _ = state.db.log_event("overseerr", "info", &log_msg, None);

    let config = state.config.get().await;
    let notif_title = format!("🥣 Kibble Bowl • Overseerr Request: {}", subject);
    let notif_body = format!("**{}** requested by **{}**\n\n• **Status:** `{}`\n• **Details:** {}", subject, requested_by, status_str, message);

    let notif_config = config.notifications.clone();
    tokio::spawn(async move {
        crate::notify::NotificationManager::dispatch(
            &notif_config,
            "ombi.request",
            None,
            &notif_title,
            &notif_body,
            Some("#8B5CF6"),
        ).await;
    });

    info!("Ingested Overseerr webhook: event='{}', user='{}', title='{}'", event_type, requested_by, subject);
    Ok(Json(json!({"status": "ok", "request_id": req_id, "event": event_type, "title": subject})))
}

// ---------------------------------------------------------------------------
// Jellyfin / Emby Webhooks
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/jellyfin/inbound",
    tag = "Integrations",
    summary = "Jellyfin/Emby Webhook Inbound",
    description = "Ingests playback and library events from Jellyfin and Emby media servers.",
    request_body = String,
    responses(
        (status = 200, description = "Webhook ingested successfully"),
        (status = 400, description = "Invalid payload")
    )
)]
pub async fn jellyfin_inbound(
    State(state): State<crate::auth::AppState>,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let payload: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Invalid JSON payload: {}", e)}))))?;

    let event_type = payload.get("NotificationType").or_else(|| payload.get("Event")).and_then(|v| v.as_str()).unwrap_or("PlaybackStart");
    let name = payload.get("Name").or_else(|| payload.get("ItemName")).and_then(|v| v.as_str()).unwrap_or("Media Item");
    let user = payload.get("NotificationUsername").or_else(|| payload.get("User")).and_then(|v| v.as_str()).unwrap_or("Jellyfin User");
    let server = payload.get("ServerName").and_then(|v| v.as_str()).unwrap_or("Jellyfin");

    let log_msg = format!("Jellyfin {}: '{}' by {} on {}", event_type, name, user, server);
    let _ = state.db.log_event("jellyfin", "info", &log_msg, None);

    let config = state.config.get().await;
    if event_type.to_lowercase().contains("playbackstop") || event_type.to_lowercase().contains("scrobble") {
        let notif_title = format!("🍿 Conduit Scrobble • {}", name);
        let notif_body = format!("**{}** finished watching `{}` on **{}**", user, name, server);
        let notif_config = config.notifications.clone();
        tokio::spawn(async move {
            crate::notify::NotificationManager::dispatch(
                &notif_config,
                "media.scrobble",
                None,
                &notif_title,
                &notif_body,
                Some("#EAB308"),
            ).await;
        });
    }

    info!("Ingested Jellyfin webhook: event='{}', user='{}', title='{}'", event_type, user, name);
    Ok(Json(json!({"status": "ok", "event": event_type, "name": name, "user": user})))
}
