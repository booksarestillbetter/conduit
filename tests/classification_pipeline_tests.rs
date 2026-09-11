// tests/classification_pipeline_tests.rs
use axum::{body::Body, http::{Request, StatusCode}};
use chrono::Utc;
use conduit::api::build_api_router;
use conduit::auth::AppState;
use conduit::config::{AppConfig, ConfigManager, MediaTypeDefinition, TrackerMappingRule, FetcherNodeConfig};
use conduit::db::{ArrGrabRecord, Database};
use conduit::notify::NotificationManager;
use conduit::fetcher::FetcherPool;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tower::ServiceExt;

#[tokio::test]
async fn test_four_tier_classification_pipeline() {
    let tmp_db = NamedTempFile::new().unwrap();
    let db = Database::init(tmp_db.path(), Some("secret-pass")).unwrap();

    let tmp_cfg = NamedTempFile::new().unwrap();
    let cfg_path = tmp_cfg.path().to_str().unwrap().to_string();

    let jwt_secret = "test-classification-pipeline-jwt-secret".to_string();
    let mut config = AppConfig::default();
    config.system.jwt_secret = jwt_secret.clone();

    // Add custom media type
    config.queue_routing.media_types.push(MediaTypeDefinition {
        id: "anime".to_string(),
        name: "Anime Releases".to_string(),
        default_queue_dir: "/media/queue/animeQueue/".to_string(),
        uhd_queue_dir: Some("/media/queue/animeUHDqueue/".to_string()),
        description: None,
    });

    // Add custom tracker rules
    config.queue_routing.tracker_mappings.push(TrackerMappingRule {
        id: "rule_ab".to_string(),
        pattern: "*animebytes*".to_string(),
        media_type: "anime".to_string(),
        priority: 20,
        comment: Some("AnimeBytes rule".to_string()),
    });

    // Add a transmission node with media_dir_overrides
    let mut overrides = HashMap::new();
    overrides.insert("tv".to_string(), "/mnt/custom/tv_post/".to_string());
    overrides.insert("tvUHD".to_string(), "/mnt/custom/tv_post_4k/".to_string());

    config.nodes.insert("node_custom".to_string(), FetcherNodeConfig {
        client_type: conduit::config::RetrieverClientType::Transmission,
        name: "node_custom".to_string(),
        host: "127.0.0.1".to_string(),
        port: 9091,
        rpc_path: "/transmission/rpc".to_string(),
        username: None,
        password: None,
        use_ssl: false,
        verify_tls: true,
        enabled: true,
        fetcher_only: false,
        tv_pre: None,
        movie_pre: None,
        music_pre: None,
        auto_purge_min_space_gb: None,
        auto_purge_ratio: None,
        auto_purge_age_days: None,
        auto_purge_seeds: None,
        auto_purge_match_count: None,
        auto_purge_enabled: false,
        media_dir_overrides: overrides,
    });

    let config_mgr = ConfigManager::load_or_init(&cfg_path).await.unwrap();
    config_mgr.update(config).await.unwrap();

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

    let admin = db.create_user("admin", &conduit::auth::hash_password("AdminPass123").unwrap(), true).unwrap();
    let admin_token = conduit::auth::create_jwt(&admin.id, &admin.username, true, admin.token_version, &jwt_secret, 7).unwrap();

    // 1. Tier 1 Test: Exact Arr Grab Association
    let grab = ArrGrabRecord {
        id: "grab-001".to_string(),
        scene_name: "Severance.S02E01.1080p.WEB-DL".to_string(),
        release_title: "Severance.S02E01.1080p.WEB-DL".to_string(),
        event_type: "Grab".to_string(),
        item_type: "series".to_string(),
        series_id: Some(123),
        movie_id: None,
        artist_id: None,
        album_id: None,
        zone_id: None,
        season_number: Some(2),
        episode_numbers: Some("[1]".to_string()),
        episode_ids: None,
        indexer: None,
        download_client: None,
        download_id: Some("grab-001".to_string()),
        status: "fetched".to_string(),
        re_searched: false,
        re_search_count: 0,
        title: Some("Severance".to_string()),
        year: Some(2024),
        overview: Some("Mark leads a team of office workers.".to_string()),
        poster_url: None,
        genres: Some("Sci-Fi, Drama".to_string()),
        quality: Some("1080p WEB-DL".to_string()),
        size_bytes: Some(2000000000),
        imdb_id: None,
        tmdb_id: None,
        tvdb_id: None,
        runtime_mins: Some(50),
        rating: Some(8.7),
        mattermost_post_id: None,
        payload_json: "{}".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    db.save_arr_grab(&grab).unwrap();

    let req = Request::builder()
        .method("POST")
        .uri("/api/sync/classify")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "name": "Severance.S02E01.1080p.WEB-DL",
            "hash": "grab-001"
        })).unwrap()))
        .unwrap();

    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let val: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["media_type"], "tv");
    assert_eq!(val["queue"], "tv");
    assert_eq!(val["match_source"], "arr_grab:sonarr");

    // 2. Tier 2 Test: Tracker Registry Matching (Non-Arr manual download)
    let req2 = Request::builder()
        .method("POST")
        .uri("/api/sync/classify")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "name": "Frieren.Beyond.Journeys.End.BD",
            "tracker": "https://tracker.animebytes.tv/announce"
        })).unwrap()))
        .unwrap();

    let resp2 = router.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let bytes2 = axum::body::to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
    let val2: serde_json::Value = serde_json::from_slice(&bytes2).unwrap();
    assert_eq!(val2["media_type"], "anime");
    assert_eq!(val2["target_dir"], "/media/queue/animeQueue/");
    assert_eq!(val2["match_source"], "tracker_rule:*animebytes*");

    // 3. Tier 3 Test: Filename Regex Heuristic Matching
    let req3 = Request::builder()
        .method("POST")
        .uri("/api/sync/classify")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "name": "Artist.Name.-.Album.(2024).[FLAC]",
        })).unwrap()))
        .unwrap();

    let resp3 = router.clone().oneshot(req3).await.unwrap();
    assert_eq!(resp3.status(), StatusCode::OK);
    let bytes3 = axum::body::to_bytes(resp3.into_body(), usize::MAX).await.unwrap();
    let val3: serde_json::Value = serde_json::from_slice(&bytes3).unwrap();
    assert_eq!(val3["media_type"], "music");
    assert_eq!(val3["target_dir"], "/media/queue/musicQueue/");

    // 4. Node Folder Override Test with UHD Marker
    let req4 = Request::builder()
        .method("POST")
        .uri("/api/sync/classify")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "name": "Succession.S04E01.2160p.UHD.mkv",
            "node": "node_custom"
        })).unwrap()))
        .unwrap();

    let resp4 = router.clone().oneshot(req4).await.unwrap();
    assert_eq!(resp4.status(), StatusCode::OK);
    let bytes4 = axum::body::to_bytes(resp4.into_body(), usize::MAX).await.unwrap();
    let val4: serde_json::Value = serde_json::from_slice(&bytes4).unwrap();
    assert_eq!(val4["media_type"], "tv");
    assert_eq!(val4["media"], "tv");
    assert_eq!(val4["queue"], "tvUHD");
    assert_eq!(val4["target_dir"], "/mnt/custom/tv_post_4k/");
    assert!(val4["decision_trace"].as_array().unwrap().len() >= 2);

    // 5. Test TV series parsing (Lanterns S01E02 on LandOf.tv tracker)
    let req5 = Request::builder()
        .method("POST")
        .uri("/api/sync/classify")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "name": "Lanterns.S01E02.Trust.Fall.1080p.AMZN.WEB-DL.DDP5.1.H.264-NTb.mkv",
            "tracker": "http://landof.tv:80/announce",
            "node": "idyll-gen"
        })).unwrap()))
        .unwrap();

    let resp5 = router.clone().oneshot(req5).await.unwrap();
    assert_eq!(resp5.status(), StatusCode::OK);
    let bytes5 = axum::body::to_bytes(resp5.into_body(), usize::MAX).await.unwrap();
    let val5: serde_json::Value = serde_json::from_slice(&bytes5).unwrap();
    assert_eq!(val5["media_type"], "tv");
    assert_eq!(val5["media"], "tv");
    assert_eq!(val5["queue"], "tv");
    assert_eq!(val5["target_dir"], "/media/queue/tvQueue/");

    // 6. Test Movie release parsing (Hadestown 2026 1080p)
    let req6 = Request::builder()
        .method("POST")
        .uri("/api/sync/classify")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "name": "Hadestown.The.Musical.2026.REPACK.1080p.AMZN.WEB-DL.DDP5.1.H.264-SCOPE.mkv",
            "node": "idyll-gen"
        })).unwrap()))
        .unwrap();

    let resp6 = router.clone().oneshot(req6).await.unwrap();
    assert_eq!(resp6.status(), StatusCode::OK);
    let bytes6 = axum::body::to_bytes(resp6.into_body(), usize::MAX).await.unwrap();
    let val6: serde_json::Value = serde_json::from_slice(&bytes6).unwrap();
    assert_eq!(val6["media_type"], "movie");
    assert_eq!(val6["media"], "movie");
    assert_eq!(val6["queue"], "movie");
    assert_eq!(val6["target_dir"], "/media/queue/movieQueue/");

    // 7. Test Resilient JSON Payload (numeric IDs, string priorities, multiline trackers)
    let req7 = Request::builder()
        .method("POST")
        .uri("/api/sync/classify")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{
            "name": "The.Last.Sunrise.2026.1080p.AMZN.WEB-DL.DDP5.1.H.264-FLUX.mkv",
            "hash": "4984cd58ba7bfdf88770606393b074917fcf5db3",
            "node": "idyll-gen",
            "id": 42,
            "priority": "0",
            "bytes_downloaded": "12345678",
            "tracker": "http://tracker.example.com/announce\nhttp://backup.tracker.com",
            "trackers": "http://extra.tracker.com"
        }"#))
        .unwrap();

    let resp7 = router.clone().oneshot(req7).await.unwrap();
    assert_eq!(resp7.status(), StatusCode::OK, "Resilient payload parsing must succeed with 200 OK");
    let bytes7 = axum::body::to_bytes(resp7.into_body(), usize::MAX).await.unwrap();
    let val7: serde_json::Value = serde_json::from_slice(&bytes7).unwrap();
    assert_eq!(val7["media_type"], "movie");
    assert_eq!(val7["queue"], "movie");

    // 8. Test Notify-Download with Fetcher Node Recording
    let req8 = Request::builder()
        .method("POST")
        .uri("/api/sync/notify-download")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {admin_token}"))
        .body(Body::from(r#"{
            "name": "Severance.S02E01.1080p.WEB-DL",
            "hash": "grab-001",
            "node": "idyll-gen",
            "id": 101,
            "queue": "tv",
            "target_dir": "/media/queue/tvQueue/"
        }"#))
        .unwrap();

    let resp8 = router.clone().oneshot(req8).await.unwrap();
    assert_eq!(resp8.status(), StatusCode::OK);

    // Verify DB grab record now stores actual fetcher node 'idyll-gen'
    let updated_grab = db.find_arr_grab_by_hash_or_name("grab-001", "Severance.S02E01.1080p.WEB-DL").unwrap().unwrap();
    assert_eq!(updated_grab.status, "staged");
    assert_eq!(updated_grab.download_client.as_deref(), Some("idyll-gen"));
}
