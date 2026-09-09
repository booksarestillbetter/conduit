// tests/lidarr_pipeline_tests.rs
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use conduit::api::build_api_router;
use conduit::auth::AppState;
use conduit::config::{ArrNodeConfig, ConfigManager, QueueTargetConfig};
use conduit::db::Database;
use conduit::notify::NotificationManager;
use conduit::fetcher::FetcherPool;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tower::ServiceExt;

#[tokio::test]
async fn test_lidarr_webhook_lifecycle_and_classifier_routing() {
    let temp_db = NamedTempFile::new().expect("Failed to create temp db");
    let db = Database::init(temp_db.path(), Some("secret_db_pass")).expect("DB init failed");

    let temp_cfg = NamedTempFile::new().expect("Failed to create temp cfg");
    let cfg_path = temp_cfg.path().to_str().unwrap().to_string();
    let config_mgr = ConfigManager::load_or_init(&cfg_path).await.expect("Config init failed");

    // Configure Lidarr and Music Queue Target
    let mut config = config_mgr.get().await;
    config.lidarr.enabled = true;
    config.lidarr.primary = Some(ArrNodeConfig {
        name: "Lidarr-Master".to_string(),
        base_url: "http://127.0.0.1:8686".to_string(),
        api_key: "lidarr_secret_key".to_string(),
        base_path: Some("/music".to_string()),
        version: 1,
    });
    config.queue_routing.queues.insert("music".to_string(), QueueTargetConfig {
        directory: "/media/queue/musicQueue".to_string(),
        notify: true,
    });
    config_mgr.update(config).await.expect("Failed to update config");

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

    // 1. Ingest Lidarr Grab Webhook
    let grab_payload = serde_json::json!({
        "eventType": "Grab",
        "artist": {
            "id": 1,
            "name": "Pink Floyd",
            "path": "/music/Pink Floyd",
            "mbId": "83d91898-7763-47d7-b03b-b92132375c47",
            "genres": ["Progressive Rock", "Psychedelic Rock"],
            "overview": "English rock band formed in London in 1965.",
            "images": [
                {
                    "coverType": "poster",
                    "remoteUrl": "https://example.com/pink_floyd.jpg"
                }
            ]
        },
        "albums": [
            {
                "id": 10,
                "title": "The Dark Side of the Moon",
                "releaseDate": "1973-03-01",
                "genres": ["Progressive Rock"],
                "images": [
                    {
                        "coverType": "cover",
                        "remoteUrl": "https://example.com/dark_side.jpg"
                    }
                ]
            }
        ],
        "release": {
            "quality": "FLAC 24bit",
            "qualityVersion": 1,
            "releaseTitle": "Pink.Floyd-The.Dark.Side.Of.The.Moon.1973.FLAC.24bit-PERFECT",
            "indexer": "RED",
            "size": 850000000
        },
        "downloadClient": "Transmission",
        "downloadId": "lidarr_hash_12345"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/lidarr/inbound")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&grab_payload).unwrap()))
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify grab is stored in DB with music item_type
    let saved_grab = db.find_arr_grab_by_hash_or_name("lidarr_hash_12345", "Pink.Floyd-The.Dark.Side.Of.The.Moon.1973.FLAC.24bit-PERFECT")
        .unwrap()
        .expect("Grab should be found in DB");
    assert_eq!(saved_grab.item_type, "music");
    assert_eq!(saved_grab.status, "fetched");
    assert_eq!(saved_grab.title.as_deref(), Some("Pink Floyd - The Dark Side of the Moon"));
    assert_eq!(saved_grab.year, Some(1973));

    // 2. Classify Staged Music Torrent (Tier 1 Match)
    let classify_payload = serde_json::json!({
        "name": "Pink.Floyd-The.Dark.Side.Of.The.Moon.1973.FLAC.24bit-PERFECT",
        "hash": "lidarr_hash_12345",
        "node": "transmission-music"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/sync/classify")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&classify_payload).unwrap()))
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["media_type"], "music");
    assert_eq!(body_json["target_dir"], "/media/queue/musicQueue/");
    assert!(body_json["match_source"].as_str().unwrap().contains("arr_grab:lidarr"));

    // 3. Ingest Lidarr Download / Import Completed Webhook
    let download_payload = serde_json::json!({
        "eventType": "Download",
        "artist": {
            "name": "Pink Floyd"
        },
        "albums": [
            {
                "title": "The Dark Side of the Moon"
            }
        ],
        "release": {
            "releaseTitle": "Pink.Floyd-The.Dark.Side.Of.The.Moon.1973.FLAC.24bit-PERFECT"
        },
        "downloadId": "lidarr_hash_12345"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/lidarr/inbound")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&download_payload).unwrap()))
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let updated_grab = db.find_arr_grab_by_hash_or_name("lidarr_hash_12345", "Pink.Floyd-The.Dark.Side.Of.The.Moon.1973.FLAC.24bit-PERFECT")
        .unwrap()
        .expect("Grab should still be in DB");
    assert_eq!(updated_grab.status, "imported");
}

#[tokio::test]
async fn test_cross_app_and_tracker_assisted_music_classification() {
    let temp_db = NamedTempFile::new().expect("Failed to create temp db");
    let db = Database::init(temp_db.path(), Some("secret_db_pass")).expect("DB init failed");

    let temp_cfg = NamedTempFile::new().expect("Failed to create temp cfg");
    let cfg_path = temp_cfg.path().to_str().unwrap().to_string();
    let config_mgr = ConfigManager::load_or_init(&cfg_path).await.expect("Config init failed");

    let mut config = config_mgr.get().await;
    config.lidarr.enabled = true;
    config.queue_routing.queues.insert("music".to_string(), QueueTargetConfig {
        directory: "/media/queue/musicQueue/".to_string(),
        notify: true,
    });
    config_mgr.update(config).await.expect("Failed to update config");

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

    // Send a music release payload with indexer RED and [V0] to the /api/sonarr/inbound endpoint
    let sonarr_misrouted_payload = serde_json::json!({
        "eventType": "Grab",
        "release": {
            "quality": "V0",
            "qualityVersion": 1,
            "releaseTitle": "Dolly Parton - A Holly Dolly Christmas (Ultimate Deluxe Edition) (2020) {093624870272} [V0]",
            "indexer": "RED",
            "size": 125000000
        },
        "downloadClient": "Transmission",
        "downloadId": "dolly_hash_98765"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/sonarr/inbound")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&sonarr_misrouted_payload).unwrap()))
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify record was automatically classified as music with parsed metadata
    let saved_grab = db.find_arr_grab_by_hash_or_name("dolly_hash_98765", "Dolly Parton - A Holly Dolly Christmas (Ultimate Deluxe Edition) (2020) {093624870272} [V0]")
        .unwrap()
        .expect("Grab should be saved in DB");

    assert_eq!(saved_grab.item_type, "music");
    assert_eq!(saved_grab.title.as_deref(), Some("Dolly Parton - A Holly Dolly Christmas (Ultimate Deluxe Edition)"));
    assert_eq!(saved_grab.year, Some(2020));
    assert_eq!(saved_grab.quality.as_deref(), Some("V0"));

    // Verify staging classifier routes this torrent into the music queue
    let classify_payload = serde_json::json!({
        "name": "Dolly Parton - A Holly Dolly Christmas (Ultimate Deluxe Edition) (2020) {093624870272} [V0]",
        "hash": "dolly_hash_98765",
        "tracker": "flacsfor.me"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/sync/classify")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&classify_payload).unwrap()))
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["media_type"], "music");
    assert_eq!(body_json["target_dir"], "/media/queue/musicQueue/");
}
