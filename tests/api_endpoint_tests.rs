// tests/api_endpoint_tests.rs
use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, Response, StatusCode},
    Router,
};
use chrono::Utc;
use conduit::api::build_api_router;
use conduit::auth::{create_jwt, hash_password, AppState};
use conduit::config::{AppConfig, ConfigManager, FetcherNodeConfig};
use conduit::db::Database;
use conduit::notify::NotificationManager;
use conduit::fetcher::{ActiveCircuitBreaker, Torrent, TrackerStat, FetcherPool};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tower::ServiceExt;

#[allow(dead_code)]
struct TestContext {
    router: Router,
    admin_token: String,
    jwt_secret: String,
    db: Database,
    pool: Arc<FetcherPool>,
    config_mgr: ConfigManager,
    _tmp_db: NamedTempFile,
    _tmp_cfg: NamedTempFile,
}

async fn setup_test_context() -> TestContext {
    let tmp_db = NamedTempFile::new().unwrap();
    let db = Database::init(tmp_db.path(), Some("test-vault-password")).unwrap();

    let tmp_cfg = NamedTempFile::new().unwrap();
    let cfg_path = tmp_cfg.path().to_str().unwrap().to_string();

    let jwt_secret = "super_secure_test_jwt_secret_key_123456789".to_string();
    let mut config = AppConfig::default();
    config.system.jwt_secret = jwt_secret.clone();

    // Add a transmission node configuration
    config.nodes.insert(
        "test_node".to_string(),
        FetcherNodeConfig {
            client_type: conduit::config::RetrieverClientType::Transmission,
            name: "test_node".to_string(),
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
            media_dir_overrides: HashMap::new(),
        },
    );

    let config_mgr = ConfigManager::load_or_init(&cfg_path).await.unwrap();
    config_mgr.update(config).await.unwrap();

    let pool = Arc::new(FetcherPool::new());
    let notifiers = Arc::new(NotificationManager);

    // Create an admin user
    let admin = db
        .create_user("admin", &hash_password("AdminPass123").unwrap(), true)
        .unwrap();

    let admin_token = create_jwt(&admin.id, &admin.username, true, admin.token_version, &jwt_secret, 7).unwrap();

    // Seed mock torrent data in transmission pool cache
    let mock_torrent = Torrent {
        id: 42,
        name: "Test.Release.2026.1080p.WEB-DL".to_string(),
        hash_string: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
        status: 4, // Downloading
        rate_upload: 1024 * 500,
        rate_download: 1024 * 1024 * 5,
        uploaded_ever: 1024 * 1024 * 500,
        downloaded_ever: 1024 * 1024 * 1200,
        upload_ratio: 0.41,
        total_size: 1024 * 1024 * 1200,
        size_when_done: 1024 * 1024 * 1200,
        left_until_done: 0,
        percent_done: 1.0,
        eta: 0,
        eta_idle: None,
        error: 0,
        error_string: String::new(),
        peers_connected: 4,
        peers_sending_to_us: 0,
        peers_getting_from_us: 0,
        added_date: Utc::now().timestamp() - 3600,
        done_date: Utc::now().timestamp() - 1800,
        download_dir: "/media/tv_post".to_string(),
        tracker_stats: vec![TrackerStat {
            announce: "http://landof.tv:80/announce".to_string(),
            host: "landof.tv:80".to_string(),
            seeder_count: 10,
            leecher_count: 2,
            download_count: 50,
            last_announce_succeeded: false,
            last_announce_result: "Tracker HTTP response 530 (Unknown Error)".to_string(),
            is_backup: false,
        }],
        files: None,
        peers: None,
        comment: None,
        creator: None,
        date_created: None,
        piece_count: None,
        piece_size: None,
        is_private: None,
        magnet_link: None,
        corrupt_ever: None,
        seconds_downloading: None,
        seconds_seeding: None,
        activity_date: None,
        queue_position: 1,
        sequential_download: false,
        pieces: None,
        availability: None,
    };
    pool.update_node_cache("test_node", vec![mock_torrent], 1024 * 1024 * 1024 * 50, 12);

    // Set an active circuit breaker on the pool
    pool.set_active_breaker(
        "landof.tv:80".to_string(),
        ActiveCircuitBreaker {
            tracker_host: "landof.tv:80".to_string(),
            canary_compound_id: "test_node:999".to_string(),
            canary_name: "Canary.Probe.Release".to_string(),
            paused_torrents: vec!["test_node:42".to_string()],
            tripped_at: Utc::now().timestamp(),
            failing_error: "Tracker HTTP response 530 (Unknown Error)".to_string(),
            ..Default::default()
        },
    );

    let state = AppState {
        config: config_mgr.clone(),
        db: db.clone(),
        fetcher_pool: pool.clone(),
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

    TestContext {
        router,
        admin_token,
        jwt_secret,
        db,
        pool,
        config_mgr,
        _tmp_db: tmp_db,
        _tmp_cfg: tmp_cfg,
    }
}

async fn send_request(
    router: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> Response<Body> {
    let mut builder = Request::builder().method(method).uri(uri);

    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {}", t));
    }

    let req_body = if let Some(b) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&b).unwrap())
    } else {
        Body::empty()
    };

    let req = builder.body(req_body).unwrap();
    router.clone().oneshot(req).await.unwrap()
}

async fn response_json(resp: Response<Body>) -> Value {
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

#[tokio::test]
async fn test_auth_and_token_lifecycle_endpoints() {
    let ctx = setup_test_context().await;

    // 1. GET /api/auth/setup-status (Admin exists, should be false)
    let resp = send_request(&ctx.router, Method::GET, "/api/auth/setup-status", None, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = response_json(resp).await;
    assert_eq!(json["setup_needed"], false);

    // 2. POST /api/auth/login (Valid credentials)
    let login_payload = json!({
        "username": "admin",
        "password": "AdminPass123"
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/auth/login", None, Some(login_payload)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let login_json = response_json(resp).await;
    assert!(login_json["token"].is_string());
    assert_eq!(login_json["user"]["username"], "admin");

    // 3. POST /api/auth/login (Invalid credentials -> 401)
    let bad_login = json!({
        "username": "admin",
        "password": "WrongPassword!"
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/auth/login", None, Some(bad_login)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 4. GET /api/auth/me (With valid token -> 200)
    let resp = send_request(&ctx.router, Method::GET, "/api/auth/me", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let me_json = response_json(resp).await;
    assert_eq!(me_json["username"], "admin");
    assert_eq!(me_json["is_admin"], true);

    // 5. GET /api/auth/me (Without token -> 401)
    let resp = send_request(&ctx.router, Method::GET, "/api/auth/me", None, None).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 6. POST /api/auth/tokens (Create API Token)
    let token_payload = json!({
        "name": "Integration-Test-Token",
        "scopes": ["torrents:read", "torrents:write"],
        "expires_days": 30
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/auth/tokens", Some(&ctx.admin_token), Some(token_payload)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let created_tok = response_json(resp).await;
    let api_token_str = created_tok["raw_token"].as_str().unwrap();
    assert!(api_token_str.starts_with("cnd_"));

    // 7. GET /api/auth/tokens (List API Tokens)
    let resp = send_request(&ctx.router, Method::GET, "/api/auth/tokens", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let tok_list = response_json(resp).await;
    assert!(!tok_list.as_array().unwrap().is_empty());

    // 8. Authenticate with the generated API Token
    let resp = send_request(&ctx.router, Method::GET, "/api/torrents", Some(api_token_str), None).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 9. DELETE /api/auth/tokens/{id} (Revoke Token)
    let tok_id = created_tok["token_record"]["id"].as_str().unwrap();
    let resp = send_request(&ctx.router, Method::DELETE, &format!("/api/auth/tokens/{}", tok_id), Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 10. Verify Revoked API Token is now rejected
    let resp = send_request(&ctx.router, Method::GET, "/api/torrents", Some(api_token_str), None).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 11. POST /api/auth/2fa/setup (2FA Secret Generation)
    let resp = send_request(&ctx.router, Method::POST, "/api/auth/2fa/setup", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let two_fa_json = response_json(resp).await;
    assert!(two_fa_json["secret"].is_string());
    assert!(two_fa_json["otpauth_url"].as_str().unwrap().contains("otpauth://totp/"));

    // 12. POST /api/auth/logout
    let resp = send_request(&ctx.router, Method::POST, "/api/auth/logout", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_torrent_and_circuit_breaker_timeline_endpoints() {
    let ctx = setup_test_context().await;

    // 1. GET /api/torrents (List all torrents with breaker tagging)
    let resp = send_request(&ctx.router, Method::GET, "/api/torrents", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let list_json = response_json(resp).await;
    let torrents = list_json.as_array().unwrap();
    assert_eq!(torrents.len(), 1);
    assert_eq!(torrents[0]["compound_id"], "test_node:42");
    assert_eq!(torrents[0]["is_circuit_broken"], true);
    assert!(torrents[0]["circuit_breaker_reason"].as_str().unwrap().contains("Swarm Pressure Relieved"));

    // 2. GET /api/torrents/stats (Cluster stats)
    let resp = send_request(&ctx.router, Method::GET, "/api/torrents/stats", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let stats_json = response_json(resp).await;
    assert_eq!(stats_json["total_torrents"], 1);

    // 3. GET /api/torrents/{compound_id} (Detailed timeline check)
    let resp = send_request(&ctx.router, Method::GET, "/api/torrents/test_node:42", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let detail_json = response_json(resp).await;
    assert_eq!(detail_json["compound_id"], "test_node:42");
    assert_eq!(detail_json["is_circuit_broken"], true);
    
    // Verify timeline includes Step 5 Circuit Breaker event
    let timeline = detail_json["timeline"].as_array().unwrap();
    let breaker_event = timeline.iter().find(|e| e["stage"] == "circuit_breaker");
    assert!(breaker_event.is_some(), "Expected circuit breaker event in timeline");
    assert_eq!(breaker_event.unwrap()["status"], "warning");
    assert!(breaker_event.unwrap()["title"].as_str().unwrap().contains("Paused by Swarm Pressure Relief"));

    // 4. POST /api/torrents/bulk (Bulk actions)
    let bulk_payload = json!({
        "action": "stop",
        "compound_ids": ["test_node:42"]
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/torrents/bulk", Some(&ctx.admin_token), Some(bulk_payload)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_sync_classification_and_events_endpoints() {
    let ctx = setup_test_context().await;

    // 1. GET /api/sync/routes (Route table)
    let resp = send_request(&ctx.router, Method::GET, "/api/sync/routes", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let routes_json = response_json(resp).await;
    assert!(routes_json["media_types"].is_array());

    // 2. POST /api/sync/classify (Inbound file classification)
    let classify_payload = json!({
        "name": "House.of.the.Dragon.S02E01.2160p.MAX.WEB-DL.DDP5.1.Atmos.H.265-FLUX",
        "hash": "11223344556677889900aabbccddeeff11223344",
        "node": "test_node",
        "tracker": "http://tracker.example.com/announce"
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/sync/classify", Some(&ctx.admin_token), Some(classify_payload)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let class_res = response_json(resp).await;
    assert_eq!(class_res["media_type"], "tv");
    assert_eq!(class_res["queue"], "tvUHD");

    // 3. POST /api/sync/notify-download (Staging completion callback)
    let notify_payload = json!({
        "name": "House.of.the.Dragon.S02E01.2160p.MAX.WEB-DL.DDP5.1.Atmos.H.265-FLUX",
        "hash": "11223344556677889900aabbccddeeff11223344",
        "node": "test_node",
        "queue": "/media/queue/tvUHDqueue"
    });
    let unauth_resp = send_request(&ctx.router, Method::POST, "/api/sync/notify-download", None, Some(notify_payload.clone())).await;
    assert_eq!(unauth_resp.status(), StatusCode::UNAUTHORIZED);

    let resp = send_request(&ctx.router, Method::POST, "/api/sync/notify-download", Some(&ctx.admin_token), Some(notify_payload)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. GET /api/sync/hook-script (Generates bash staging script)
    let resp = send_request(&ctx.router, Method::GET, "/api/sync/hook-script", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 5. GET /api/events (Audit logs query)
    let resp = send_request(&ctx.router, Method::GET, "/api/events?limit=50", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let events_json = response_json(resp).await;
    assert!(events_json.is_array());
}

#[tokio::test]
async fn test_arr_inbound_and_pipeline_endpoints() {
    let ctx = setup_test_context().await;

    // 1. POST /api/sonarr/inbound (Sonarr Grab Event)
    let grab_id = "test-grab-sonarr-101";
    let sonarr_payload = json!({
        "eventType": "Grab",
        "series": {
            "id": 88,
            "title": "Severance",
            "tvdbId": 371980,
            "genres": ["Drama", "Sci-Fi", "Mystery"],
            "year": 2022,
            "overview": "Mark leads a team of office workers whose memories have been surgically divided.",
            "images": [
                {
                    "coverType": "poster",
                    "url": "https://artworks.thetvdb.com/banners/v4/series/371980/posters/severance.jpg"
                }
            ]
        },
        "episodes": [
            {
                "id": 1001,
                "episodeNumber": 1,
                "seasonNumber": 2,
                "title": "Hello Ms. Cobel"
            }
        ],
        "release": {
            "releaseTitle": "Severance.S02E01.1080p.ATVP.WEB-DL.DDP5.1.Atmos.H.264",
            "indexer": "BTN",
            "size": 2500000000i64,
            "quality": "WEB-DL 1080p"
        },
        "downloadClient": "Transmission-Idyll",
        "downloadId": grab_id
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/sonarr/inbound", None, Some(sonarr_payload)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. GET /api/arr/grabs (List grabs)
    let resp = send_request(&ctx.router, Method::GET, "/api/arr/grabs", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let grabs_json = response_json(resp).await;
    assert!(!grabs_json.as_array().unwrap().is_empty());
    let found = grabs_json.as_array().unwrap().iter().find(|g| g["id"] == grab_id);
    assert!(found.is_some());
    assert_eq!(found.unwrap()["title"], "Severance");

    // 3. GET /api/arr/pipeline (List pipeline items)
    let resp = send_request(&ctx.router, Method::GET, "/api/arr/pipeline", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. GET /api/arr/stats
    let resp = send_request(&ctx.router, Method::GET, "/api/arr/stats", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_system_health_and_metrics_endpoints() {
    let ctx = setup_test_context().await;

    // 1. GET /api/health (System liveness)
    let resp = send_request(&ctx.router, Method::GET, "/api/health", None, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let health_json = response_json(resp).await;
    assert_eq!(health_json["status"], "ok");

    // 2. GET /metrics (Prometheus exposition format; needs a token by default)
    let resp = send_request(&ctx.router, Method::GET, "/metrics", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 3. GET /api/system/stats
    let resp = send_request(&ctx.router, Method::GET, "/api/system/stats", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let stats_json = response_json(resp).await;
    assert!(stats_json["cpu_count"].is_number());
    assert_eq!(stats_json["version"], env!("CARGO_PKG_VERSION"));

    // 4. GET /api/system/health (Cluster & Tracker Breaker Health)
    let resp = send_request(&ctx.router, Method::GET, "/api/system/health", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let sys_health = response_json(resp).await;
    assert!(sys_health["nodes"].is_array());
    assert!(sys_health["trackers"].is_array());

    // Verify tracker health exposes active circuit breaker
    let trackers = sys_health["trackers"].as_array().unwrap();
    let broken_tracker = trackers.iter().find(|t| t["host"] == "landof.tv:80");
    assert!(broken_tracker.is_some());
    assert_eq!(broken_tracker.unwrap()["is_circuit_broken"], true);
    assert_eq!(broken_tracker.unwrap()["paused_torrents"], 1);
    assert_eq!(broken_tracker.unwrap()["active_probe_name"], "Canary.Probe.Release");

    // 5. GET /api/system/crash-log requires auth (leaks internal file paths + backtraces)
    let unauth_resp = send_request(&ctx.router, Method::GET, "/api/system/crash-log", None, None).await;
    assert_eq!(unauth_resp.status(), StatusCode::UNAUTHORIZED);
    let resp = send_request(&ctx.router, Method::GET, "/api/system/crash-log", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_bazarr_overseerr_jellyfin_webhook_secret_enforcement() {
    let ctx = setup_test_context().await;

    // No secret configured yet: unauthenticated requests are accepted (matches the
    // existing Sonarr/Radarr/Lidarr/Ombi fail-open-when-unconfigured convention).
    let resp = send_request(&ctx.router, Method::POST, "/api/bazarr/inbound", None, Some(json!({"eventType": "Download", "seriesTitle": "Test Show"}))).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Configure secrets for all three, then confirm each rejects a request with no/wrong
    // secret and accepts one with the right one.
    let mut config = ctx.config_mgr.get().await;
    config.bazarr.webhook_secret = Some("bazarr-secret".to_string());
    config.overseerr.webhook_secret = Some("overseerr-secret".to_string());
    config.jellyfin.webhook_secret = Some("jellyfin-secret".to_string());
    ctx.config_mgr.update(config).await.unwrap();

    let cases: [(&str, &str, Value); 3] = [
        ("/api/bazarr/inbound", "bazarr-secret", json!({"eventType": "Download", "seriesTitle": "Test Show"})),
        ("/api/overseerr/inbound", "overseerr-secret", json!({"notification_type": "MEDIA_PENDING", "subject": "Test Movie"})),
        ("/api/jellyfin/inbound", "jellyfin-secret", json!({"NotificationType": "PlaybackStart", "Name": "Test Item"})),
    ];

    for (path, secret, payload) in cases {
        // No secret header at all.
        let resp = send_request(&ctx.router, Method::POST, path, None, Some(payload.clone())).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{path} should reject a request with no webhook secret");

        // Wrong secret.
        let req = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Webhook-Secret", "wrong-secret")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let resp = ctx.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{path} should reject the wrong webhook secret");

        // Correct secret.
        let req = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Webhook-Secret", secret)
            .body(Body::from(payload.to_string()))
            .unwrap();
        let resp = ctx.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{path} should accept the correct webhook secret");
    }
}

#[tokio::test]
async fn test_settings_and_backup_endpoints() {
    let ctx = setup_test_context().await;

    // 1. GET /api/settings (Admin access)
    let resp = send_request(&ctx.router, Method::GET, "/api/settings", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let cfg_json = response_json(resp).await;
    assert!(cfg_json["system"].is_object());

    // 2. POST /api/settings/backup (Plaintext & Encrypted exports)
    let backup_req = json!({
        "passphrase": "secure_backup_password_123"
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/settings/backup", Some(&ctx.admin_token), Some(backup_req)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bundle_json = response_json(resp).await;
    assert_eq!(bundle_json["is_encrypted"], true);
    assert!(bundle_json["vault_envelope"].is_object());

    // 3. POST /api/settings/restore (Restore backup)
    let restore_req = json!({
        "bundle": bundle_json,
        "passphrase": "secure_backup_password_123"
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/settings/restore", Some(&ctx.admin_token), Some(restore_req)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. POST /api/pipeline/test-regex (Integration endpoint)
    let regex_req = json!({
        "pattern": r"(?i)\b(2160p|uhd|4k)\b",
        "test_string": "House.of.the.Dragon.S02E01.2160p.MAX.WEB-DL"
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/pipeline/test-regex", Some(&ctx.admin_token), Some(regex_req)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let regex_res = response_json(resp).await;
    assert_eq!(regex_res["matched"], true);
    assert_eq!(regex_res["pattern_valid"], true);
}

#[tokio::test]
async fn test_circuit_breaker_persistence_and_restore_across_restart() {
    let ctx = setup_test_context().await;

    let breaker = ActiveCircuitBreaker {
        tracker_host: "landof.tv:80".to_string(),
        canary_compound_id: "node1:100".to_string(),
        canary_name: "Bones.S07.720p.BluRay.x264".to_string(),
        paused_torrents: vec!["node1:101".to_string(), "node1:102".to_string(), "node1:103".to_string()],
        failing_error: "Tracker HTTP response 530 (Unknown Error)".to_string(),
        tripped_at: Utc::now().timestamp(),
        state: "recovering".to_string(),
        recovery_started_at: Some(Utc::now().timestamp()),
        consecutive_successes: 3,
        backoff_secs: 60,
    };

    // 1. Save circuit breaker to database
    ctx.db.save_circuit_breaker(&breaker).expect("Failed to save breaker");

    // 2. Query list from DB
    let breakers = ctx.db.list_circuit_breakers().expect("Failed to list breakers");
    assert_eq!(breakers.len(), 1);
    assert_eq!(breakers[0].tracker_host, "landof.tv:80");
    assert_eq!(breakers[0].canary_compound_id, "node1:100");
    assert_eq!(breakers[0].paused_torrents.len(), 3);
    assert_eq!(breakers[0].state, "recovering");
    assert_eq!(breakers[0].consecutive_successes, 3);
    assert_eq!(breakers[0].backoff_secs, 60);
    assert!(breakers[0].recovery_started_at.is_some());

    // 3. Simulate daemon restart with fresh FetcherPool
    let fresh_pool = Arc::new(FetcherPool::new());
    for b in ctx.db.list_circuit_breakers().unwrap() {
        fresh_pool.set_active_breaker(b.tracker_host.clone(), b);
    }
    let restored = fresh_pool.get_active_breakers();
    assert!(restored.contains_key("landof.tv:80"));
    assert_eq!(restored["landof.tv:80"].canary_name, "Bones.S07.720p.BluRay.x264");

    // 4. Test recovery / removal
    ctx.db.remove_circuit_breaker("landof.tv:80").expect("Failed to remove breaker");
    let after_removal = ctx.db.list_circuit_breakers().expect("Failed to list after removal");
    assert_eq!(after_removal.len(), 0);
}

#[tokio::test]
async fn test_power_features_endpoints() {
    let ctx = setup_test_context().await;

    // 1. POST /api/torrents/queue-move (Bulk Queue Movement)
    let move_req = json!({
        "compound_ids": ["test_node:42"],
        "direction": "top"
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/torrents/queue-move", Some(&ctx.admin_token), Some(move_req)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let move_res = response_json(resp).await;
    assert_eq!(move_res["direction"], "top");

    // 2. POST /api/torrents/batch-replace-trackers (Tracker Search & Replace)
    let replace_req = json!({
        "old_url": "http://landof.tv:80/announce",
        "new_url": "https://landof.tv:443/announce"
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/torrents/batch-replace-trackers", Some(&ctx.admin_token), Some(replace_req)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let replace_res = response_json(resp).await;
    assert_eq!(replace_res["status"], "success");
    assert_eq!(replace_res["old_url"], "http://landof.tv:80/announce");
    assert_eq!(replace_res["new_url"], "https://landof.tv:443/announce");

    // 3. POST /api/nodes/turtle-mode (Global Turtle Mode Toggle)
    let turtle_req = json!({
        "enabled": true
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/nodes/turtle-mode", Some(&ctx.admin_token), Some(turtle_req)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let turtle_res = response_json(resp).await;
    assert_eq!(turtle_res["alt_speed_enabled"], true);

    // 4. GET /api/torrents/test_node:42 (Verify detailed power fields)
    let resp = send_request(&ctx.router, Method::GET, "/api/torrents/test_node:42", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let details = response_json(resp).await;
    assert_eq!(details["queue_position"], 1);
    assert_eq!(details["sequential_download"], false);
}

#[tokio::test]
async fn test_mobile_qr_pairing_lifecycle() {
    let ctx = setup_test_context().await;

    // 1. POST /api/auth/mobile/pair-token (Requires auth)
    let resp = send_request(&ctx.router, Method::POST, "/api/auth/mobile/pair-token", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let pair_token_res = response_json(resp).await;
    let pair_code = pair_token_res["pair_code"].as_str().unwrap();
    assert!(pair_code.starts_with("conduit_pair_"));
    assert_eq!(pair_token_res["expires_in_secs"], 180);
    assert!(pair_token_res["qr_payload"].as_str().unwrap().contains(pair_code));

    // 2. POST /api/auth/mobile/pair (Public endpoint called by mobile device)
    let mobile_req = json!({
        "pair_code": pair_code,
        "device_name": "Test User's iPhone",
        "device_id": "test-device-uuid-123"
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/auth/mobile/pair", None, Some(mobile_req)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let pair_res = response_json(resp).await;
    let mobile_jwt = pair_res["token"].as_str().unwrap();
    assert_eq!(pair_res["user"]["username"], "admin");
    assert_eq!(pair_res["ws_endpoint"], "/api/ws");

    // 3. Verify mobile JWT can authenticate against protected endpoints
    let resp = send_request(&ctx.router, Method::GET, "/api/auth/me", Some(mobile_jwt), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let me = response_json(resp).await;
    assert_eq!(me["username"], "admin");

    // 4. Verify pair code cannot be re-used (single-use)
    let replay_req = json!({
        "pair_code": pair_code,
        "device_name": "Attacker Device"
    });
    let resp = send_request(&ctx.router, Method::POST, "/api/auth/mobile/pair", None, Some(replay_req)).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}


/// Every route is reachable only with credentials, apart from the ones that are public on
/// purpose: sign-in and first-run setup, the health probe, inbound webhooks (which check their
/// own shared secret), the WebSocket (which needs a single-use ticket) and the Swagger UI.
/// Routes are read from the router source, so a route added later is covered without editing
/// this test.
#[tokio::test]
async fn every_route_requires_credentials_unless_it_is_public_on_purpose() {
    let ctx = setup_test_context().await;

    const PUBLIC: &[&str] = &[
        "/api/auth/setup-status",
        "/api/auth/setup",
        "/api/auth/login",
        "/api/auth/logout",
        "/api/health",
        "/api/ws",
        // Redeems a single-use, three-minute pairing code shown as a QR code by a signed-in user.
        "/api/auth/mobile/pair",
    ];
    let is_webhook = |p: &str| p.contains("inbound") || p.starts_with("/webhook/") || p.starts_with("/api/webhook");

    let src = include_str!("../src/api/mod.rs");
    let mut open: Vec<String> = Vec::new();
    let mut checked = 0;
    let mut rest = src;
    while let Some(i) = rest.find(".route(") {
        rest = &rest[i + ".route(".len()..];
        let Some(q1) = rest.find('"') else { break };
        let Some(q2) = rest[q1 + 1..].find('"') else { break };
        let path = &rest[q1 + 1..q1 + 1 + q2];
        // The handler expression runs to the end of the line.
        let line = rest[q1 + 1 + q2..].lines().next().unwrap_or("");
        if PUBLIC.contains(&path) || is_webhook(path) {
            continue;
        }
        let concrete = path.replace(['{', '}'], "");
        let mut methods = Vec::new();
        for (needle, method) in [("get(", Method::GET), ("post(", Method::POST), ("put(", Method::PUT), ("delete(", Method::DELETE), ("patch(", Method::PATCH)] {
            if line.contains(needle) {
                methods.push(method);
            }
        }
        for method in methods {
            let resp = send_request(&ctx.router, method.clone(), &concrete, None, None).await;
            checked += 1;
            if !matches!(resp.status(), StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                open.push(format!("{method} {path} -> {}", resp.status()));
            }
        }
    }
    assert!(checked >= 70, "the route scan found only {checked} routes; the parser is out of date");
    assert!(open.is_empty(), "routes reachable without credentials:\n  {}", open.join("\n  "));
}

#[tokio::test]
async fn metrics_need_a_token_unless_the_operator_makes_them_public() {
    let ctx = setup_test_context().await;

    let anon = send_request(&ctx.router, Method::GET, "/metrics", None, None).await;
    assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(anon.headers().get(header::WWW_AUTHENTICATE).unwrap(), "Bearer");

    let authed = send_request(&ctx.router, Method::GET, "/metrics", Some(&ctx.admin_token), None).await;
    assert_eq!(authed.status(), StatusCode::OK);
    let body = to_bytes(authed.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("conduit_app_info"));

    // Opting in makes the endpoint anonymous again, for a scraper that cannot send a token.
    let mut config = ctx.config_mgr.get().await;
    config.system.metrics_public = true;
    ctx.config_mgr.update(config).await.unwrap();
    let public = send_request(&ctx.router, Method::GET, "/metrics", None, None).await;
    assert_eq!(public.status(), StatusCode::OK);
}

#[tokio::test]
async fn the_formerly_open_endpoints_work_with_a_token() {
    let ctx = setup_test_context().await;
    for uri in ["/api/system/stats", "/api/system/health", "/api/system/engines", "/api/plex/scrobbles", "/api/ombi/requests"] {
        let resp = send_request(&ctx.router, Method::GET, uri, Some(&ctx.admin_token), None).await;
        assert_eq!(resp.status(), StatusCode::OK, "{uri}");
    }
}

fn node_config(name: &str, client_type: conduit::config::RetrieverClientType) -> FetcherNodeConfig {
    FetcherNodeConfig {
        client_type,
        name: name.to_string(),
        // Nothing listens here: an operation that reaches the network fails, one the backend
        // refuses up front does not.
        host: "127.0.0.1".to_string(),
        port: 1,
        rpc_path: "/".to_string(),
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
        media_dir_overrides: HashMap::new(),
    }
}

#[tokio::test]
async fn what_a_backend_cannot_do_is_a_501_with_the_reason_and_the_ui_can_see_it_coming() {
    use conduit::config::RetrieverClientType as Kind;
    let ctx = setup_test_context().await;

    let mut config = ctx.config_mgr.get().await;
    config.nodes.insert("qbit".into(), node_config("qbit", Kind::QBittorrent));
    config.nodes.insert("deluge".into(), node_config("deluge", Kind::Deluge));
    ctx.pool.sync_nodes_from_config(&config);

    // The capability map tells a client which buttons to offer, per node.
    let resp = send_request(&ctx.router, Method::GET, "/api/nodes/capabilities", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let caps = response_json(resp).await;
    assert_eq!(caps["test_node"]["test_port"], true, "Transmission really tests its port");
    assert_eq!(caps["qbit"]["test_port"], false);
    assert_eq!(caps["qbit"]["blocklist_update"], false);
    assert_eq!(caps["deluge"]["turtle_mode"], false);
    assert_eq!(caps["deluge"]["queue_move"], true);

    // Asking anyway is a 501 that says why, never a fabricated success.
    let cases = [
        (Method::POST, "/api/nodes/qbit/test-port", "qBittorrent has no port-check API"),
        (Method::POST, "/api/nodes/deluge/test-port", "Deluge has no port-check API"),
        (Method::POST, "/api/nodes/qbit/blocklist-update", "qBittorrent has no built-in blocklist"),
        (Method::POST, "/api/nodes/deluge/turtle-mode", "Deluge has no alternate-speed toggle"),
    ];
    for (method, uri, reason) in cases {
        let body = (uri.ends_with("turtle-mode")).then(|| json!({"enabled": true}));
        let resp = send_request(&ctx.router, method, uri, Some(&ctx.admin_token), body).await;
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED, "{uri}");
        let v = response_json(resp).await;
        assert!(v["error"].as_str().unwrap().contains(reason), "{uri}: {v}");
        assert_eq!(v["unsupported"], true);
    }

    // A real failure (the daemon is unreachable) is still a 500, not mistaken for "unsupported".
    let resp = send_request(&ctx.router, Method::POST, "/api/nodes/test_node/test-port", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn universal_search_finds_active_torrents_and_requires_a_token() {
    let ctx = setup_test_context().await;

    let anon = send_request(&ctx.router, Method::GET, "/api/search?q=Test.Release", None, None).await;
    assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);

    let resp = send_request(&ctx.router, Method::GET, "/api/search?q=Test.Release", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    let torrents = body["torrents"].as_array().unwrap();
    assert!(
        torrents.iter().any(|t| t["name"] == "Test.Release.2026.1080p.WEB-DL"),
        "the seeded active torrent should be found: {body}"
    );
    // History sections are present (even if empty) rather than missing keys.
    assert!(body["history"]["grabs"].is_array());
    assert!(body["sonarr"].is_array());
    assert!(body["radarr"].is_array());

    // An empty/blank query is answered, not an error, and finds nothing.
    let resp = send_request(&ctx.router, Method::GET, "/api/search?q=", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    assert_eq!(body["torrents"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn purge_history_now_requires_a_configured_retention_period() {
    let ctx = setup_test_context().await;

    // Not configured yet: a clear 400, not a purge of nothing silently reported as success.
    let resp = send_request(&ctx.router, Method::POST, "/api/settings/purge-history", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let mut config = ctx.config_mgr.get().await;
    config.system.history_retention_days = Some(30);
    ctx.config_mgr.update(config).await.unwrap();

    let resp = send_request(&ctx.router, Method::POST, "/api/settings/purge-history", Some(&ctx.admin_token), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    assert_eq!(body["event_logs"], 0, "nothing is old enough to purge yet: {body}");

    // Not an admin route by accident.
    let anon = send_request(&ctx.router, Method::POST, "/api/settings/purge-history", None, None).await;
    assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);
}
