// tests/webhook_pipeline_tests.rs
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use conduit::api::build_api_router;
use conduit::auth::AppState;
use conduit::config::ConfigManager;
use conduit::db::Database;
use conduit::notify::NotificationManager;
use conduit::fetcher::FetcherPool;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tower::ServiceExt;

#[tokio::test]
async fn test_plex_webhook_json_and_scrobble_listing() {
    let temp_db = NamedTempFile::new().expect("Failed to create temp db");
    let db = Database::init(temp_db.path(), Some("secret_db_pass")).expect("DB init failed");

    let temp_cfg = NamedTempFile::new().expect("Failed to create temp cfg");
    let cfg_path = temp_cfg.path().to_str().unwrap().to_string();
    let config_mgr = ConfigManager::load_or_init(&cfg_path).await.expect("Config init failed");

    let fetcher_pool = Arc::new(FetcherPool::new());
    let notifiers = Arc::new(NotificationManager);

    let state = AppState {
        config: config_mgr,
        db: db.clone(),
        fetcher_pool,
        notifiers,
        rate_limiter: conduit::auth::RateLimiter::new(),
        ws_tickets: conduit::auth::WsTicketStore::new(),
        mobile_pairs: conduit::auth::MobilePairingStore::new(),
        arr_stats_cache: conduit::engines::arr_stats_poller::new_cache(),
        ip_asn_db: conduit::ip_asn::new_cache(),
        engine_registry: conduit::engines::registry::new_registry(),
        event_bus: conduit::events::new_bus(),
    };

    let router = build_api_router(state);

    // 1. Ingest Plex JSON webhook
    let plex_payload = serde_json::json!({
        "event": "media.scrobble",
        "user": { "title": "testuser" },
        "Account": { "title": "testuser" },
        "Metadata": {
            "type": "episode",
            "title": "A Fight Once Begun",
            "grandparentTitle": "Avatar: The Last Airbender",
            "parentIndex": 2,
            "index": 2,
            "year": 2024,
            "duration": 3600000,
            "viewOffset": 3500000,
            "Guid": [
                { "id": "imdb://tt14452776" },
                { "id": "tmdb://12345" },
                { "id": "tvdb://67890" }
            ]
        }
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/plex/inbound")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&plex_payload).unwrap()))
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 2. Query Plex scrobble history
    let req = Request::builder()
        .method("GET")
        .uri("/api/plex/scrobbles?event=media.scrobble")
        .body(Body::empty())
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["total"], 1);
    assert_eq!(body_json["items"][0]["title"], "A Fight Once Begun");
    assert_eq!(body_json["items"][0]["series_title"], "Avatar: The Last Airbender");
    assert_eq!(body_json["items"][0]["imdb_id"], "tt14452776");
    assert_eq!(body_json["items"][0]["season_number"], 2);

    // 3. Ingest Plex Multipart Webhook (Plex standard format)
    let multipart_body = format!(
        "--boundary123\r\nContent-Disposition: form-data; name=\"payload\"\r\n\r\n{}\r\n--boundary123--\r\n",
        serde_json::json!({
            "event": "media.scrobble",
            "user": { "title": "testuser" },
            "Account": { "title": "testuser" },
            "Metadata": {
                "type": "movie",
                "title": "Dune: Part Two",
                "year": 2024,
                "duration": 9600000,
                "Guid": [{ "id": "imdb://tt15239678" }]
            }
        })
    );

    let req = Request::builder()
        .method("POST")
        .uri("/api/plex/inbound")
        .header("Content-Type", "multipart/form-data; boundary=boundary123")
        .body(Body::from(multipart_body))
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json["items"][0]["episode_number"], 2);
}

#[tokio::test]
async fn test_ombi_webhook_and_request_listing() {
    let temp_db = NamedTempFile::new().expect("Failed to create temp db");
    let db = Database::init(temp_db.path(), Some("secret_db_pass")).expect("DB init failed");

    let temp_cfg = NamedTempFile::new().expect("Failed to create temp cfg");
    let cfg_path = temp_cfg.path().to_str().unwrap().to_string();
    let config_mgr = ConfigManager::load_or_init(&cfg_path).await.expect("Config init failed");

    let fetcher_pool = Arc::new(FetcherPool::new());
    let notifiers = Arc::new(NotificationManager);

    let state = AppState {
        config: config_mgr,
        db: db.clone(),
        fetcher_pool,
        notifiers,
        rate_limiter: conduit::auth::RateLimiter::new(),
        ws_tickets: conduit::auth::WsTicketStore::new(),
        mobile_pairs: conduit::auth::MobilePairingStore::new(),
        arr_stats_cache: conduit::engines::arr_stats_poller::new_cache(),
        ip_asn_db: conduit::ip_asn::new_cache(),
        engine_registry: conduit::engines::registry::new_registry(),
        event_bus: conduit::events::new_bus(),
    };

    let router = build_api_router(state);

    // 1. Ingest Ombi request webhook
    let ombi_payload = serde_json::json!({
        "notificationType": "NewRequest",
        "requestedUser": "alice",
        "title": "Dune: Part Two",
        "type": "Movie",
        "year": 2024,
        "overview": "Paul Atreides unites with Chani and the Fremen.",
        "theMovieDbId": 693134,
        "imdbId": "tt15239678",
        "status": "pending"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/ombi/inbound")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&ombi_payload).unwrap()))
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 2. Query Ombi requests
    let req = Request::builder()
        .method("GET")
        .uri("/api/ombi/requests")
        .body(Body::empty())
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["total"], 1);
    assert_eq!(body_json["items"][0]["title"], "Dune: Part Two");
    assert_eq!(body_json["items"][0]["requested_by"], "alice");
    assert_eq!(body_json["items"][0]["media_type"], "movie");
    assert_eq!(body_json["items"][0]["status"], "pending");
    assert_eq!(body_json["items"][0]["imdb_id"], "tt15239678");

    // 3. Verify event logs were created for both "ombi" and "webhook"
    let logs = db.search_event_logs(None, Some("ombi"), None, 10).unwrap();
    assert!(!logs.is_empty(), "Ombi event must be logged in database");
    assert!(logs[0].message.contains("Dune: Part Two"));

    // 4. Ingest Ombi RequestAvailable update
    let ombi_avail = serde_json::json!({
        "notificationType": "RequestAvailable",
        "requestedUser": "alice",
        "title": "Dune: Part Two",
        "type": "Movie",
        "imdbId": "tt15239678"
    });

    let req_avail = Request::builder()
        .method("POST")
        .uri("/api/ombi/inbound")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&ombi_avail).unwrap()))
        .unwrap();

    let res_avail = router.clone().oneshot(req_avail).await.unwrap();
    assert_eq!(res_avail.status(), StatusCode::OK);

    // Verify status updated to available in DB
    let req_lookup = db.find_ombi_request_by_id_or_title("", "Dune: Part Two").unwrap().unwrap();
    assert_eq!(req_lookup.status, "available");

    // 5. Test Radarr grab linking to Ombi request
    let radarr_grab = serde_json::json!({
        "eventType": "Grab",
        "movie": {
            "title": "Dune: Part Two",
            "year": 2024,
            "imdbId": "tt15239678",
            "tmdbId": 693134
        },
        "release": {
            "releaseTitle": "Dune.Part.Two.2024.1080p.AMZN.WEB-DL.DDP5.1.H.264-FLUX",
            "indexer": "PTP",
            "size": 7500000000_i64
        },
        "downloadClient": "Transmission",
        "downloadId": "dune2hash123"
    });

    let req_radarr = Request::builder()
        .method("POST")
        .uri("/api/radarr/inbound")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&radarr_grab).unwrap()))
        .unwrap();

    let res_radarr = router.clone().oneshot(req_radarr).await.unwrap();
    assert_eq!(res_radarr.status(), StatusCode::OK);
}
