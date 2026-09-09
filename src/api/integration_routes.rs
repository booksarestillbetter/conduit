// src/api/integration_routes.rs
use crate::auth::{AppState, RequireAdmin};
use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlexTestRequest {
    pub url: String,
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IntegrationTestResponse {
    pub success: bool,
    pub message: String,
    pub server_name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlexRefreshRequest {
    pub url: String,
    pub token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TraktTestRequest {
    pub client_id: String,
    pub access_token: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TestNotificationRequest {
    /// The `NotificationTarget.id` to test — one config can now have multiple targets of the
    /// same channel type, so this replaces the old bare channel-type string.
    pub target_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TestRegexRequest {
    pub pattern: String,
    pub test_string: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TestRegexResponse {
    pub matched: bool,
    pub pattern_valid: bool,
    pub error: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/plex/test-connection",
    tag = "Settings",
    summary = "Test Plex Media Server connectivity",
    description = "Tests authentication and reachability against a remote Plex Media Server instance. Connectivity failures are reported as `success: false` in the 200 response body, not as an HTTP error.",
    request_body = PlexTestRequest,
    responses(
        (status = 200, description = "Plex test response (check `success`)", body = IntegrationTestResponse),
        (status = 401, description = "Unauthorized — admin session required"),
        (status = 500, description = "Failed to build the outbound HTTP client")
    )
)]
pub async fn test_plex_connection(
    _admin: RequireAdmin,
    Json(req): Json<PlexTestRequest>,
) -> Result<Json<IntegrationTestResponse>, StatusCode> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let identity_url = format!("{}/identity", req.url.trim_end_matches('/'));
    match client
        .get(&identity_url)
        .header("X-Plex-Token", &req.token)
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => {
            let body: serde_json::Value = res.json().await.unwrap_or_default();
            let server_name = body
                .get("MediaContainer")
                .and_then(|c| c.get("friendlyName"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let version = body
                .get("MediaContainer")
                .and_then(|c| c.get("version"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            Ok(Json(IntegrationTestResponse {
                success: true,
                message: "Successfully connected to Plex Media Server!".to_string(),
                server_name,
                version,
            }))
        }
        Ok(res) => Ok(Json(IntegrationTestResponse {
            success: false,
            message: format!("Plex server returned HTTP {}", res.status()),
            server_name: None,
            version: None,
        })),
        Err(e) => Ok(Json(IntegrationTestResponse {
            success: false,
            message: format!("Failed to reach Plex server: {}", e),
            server_name: None,
            version: None,
        })),
    }
}

#[utoipa::path(
    post,
    path = "/api/plex/refresh",
    tag = "Settings",
    summary = "Refresh Plex Library Sections",
    description = "Commands a Plex server to scan and refresh all library sections.",
    request_body = PlexRefreshRequest,
    responses(
        (status = 200, description = "Refresh command dispatched")
    )
)]
pub async fn refresh_plex_sections(
    _admin: RequireAdmin,
    Json(req): Json<PlexRefreshRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to create client"}))))?;

    let refresh_url = format!("{}/library/sections/all/refresh", req.url.trim_end_matches('/'));
    match client
        .get(&refresh_url)
        .header("X-Plex-Token", &req.token)
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => {
            Ok(Json(json!({"status": "ok", "message": "Plex library refresh triggered"})))
        }
        Ok(res) => Err((StatusCode::BAD_REQUEST, Json(json!({"error": format!("Plex returned HTTP {}", res.status())})))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Plex request failed: {}", e)})))),
    }
}

#[utoipa::path(
    post,
    path = "/api/trakt/test-connection",
    tag = "Settings",
    summary = "Test Trakt API credentials",
    description = "Validates Trakt API Client ID and OAuth access token. Connectivity/auth failures are reported as `success: false` in the 200 response body, not as an HTTP error.",
    request_body = TraktTestRequest,
    responses(
        (status = 200, description = "Trakt test response (check `success`)", body = IntegrationTestResponse),
        (status = 401, description = "Unauthorized — admin session required")
    )
)]
pub async fn test_trakt_connection(
    _admin: RequireAdmin,
    Json(req): Json<TraktTestRequest>,
) -> Result<Json<IntegrationTestResponse>, StatusCode> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut request = client
        .get("https://api.trakt.tv/users/settings")
        .header("trakt-api-version", "2")
        .header("trakt-api-key", &req.client_id)
        .header("Content-Type", "application/json");

    if let Some(ref token) = req.access_token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    match request.send().await {
        Ok(res) if res.status().is_success() => {
            let data: serde_json::Value = res.json().await.unwrap_or_default();
            let username = data.get("user").and_then(|u| u.get("username")).and_then(|v| v.as_str());
            Ok(Json(IntegrationTestResponse {
                success: true,
                message: format!("Successfully authenticated with Trakt as '{}'!", username.unwrap_or("user")),
                server_name: username.map(|s| s.to_string()),
                version: None,
            }))
        }
        Ok(res) => Ok(Json(IntegrationTestResponse {
            success: false,
            message: format!("Trakt API returned HTTP {}", res.status()),
            server_name: None,
            version: None,
        })),
        Err(e) => Ok(Json(IntegrationTestResponse {
            success: false,
            message: format!("Failed to reach Trakt API: {}", e),
            server_name: None,
            version: None,
        })),
    }
}

#[utoipa::path(
    post,
    path = "/api/notifications/test",
    tag = "Settings",
    summary = "Send Test Notification Ping",
    description = "Dispatches a rich markdown test notification to configured Mattermost, Discord, or Webhook channels.",
    request_body = TestNotificationRequest,
    responses(
        (status = 200, description = "Test notification dispatched")
    )
)]
pub async fn send_test_notification(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Json(req): Json<TestNotificationRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = state.config.get().await;
    let Some(target) = config.notifications.targets.iter().find(|t| t.id == req.target_id) else {
        return Err((StatusCode::NOT_FOUND, Json(json!({
            "status": "error",
            "message": format!("Unknown notification target '{}'", req.target_id)
        }))));
    };

    // Mattermost with a bot token gets the richer living-card round-trip test (create, then
    // edit in-place) since that's the one behavior worth specifically verifying works.
    if let crate::config::NotificationChannel::Mattermost(mm) = &target.channel {
        if mm.bot_token.as_deref().map(|t| !t.trim().is_empty()).unwrap_or(false) {
            let test_title = "🐕🐾 Conduit Living Card Test";
            let test_overview = "Testing Mattermost Bot Token permissions and in-place card updates.";
            let fields = vec![
                ("🤖 Mode", "Mattermost Bot Token REST API v4", true),
                ("⚙️ In-Place Edit", "Testing Live Edit...", true),
            ];
            let post_res = crate::notify::mattermost::dispatch_or_update_mattermost_card(
                mm, None, None, test_title, test_overview, None, fields, Some("#10B981"),
            ).await;

            match post_res {
                Ok(Some(post_id)) => {
                    let mm_clone = mm.clone();
                    let target_post_id = post_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
                        let updated_fields = vec![
                            ("🤖 Mode", "Mattermost Bot Token REST API v4", true),
                            ("⚙️ In-Place Edit", "✅ Live Edit Tested & Verified!", true),
                        ];
                        let _ = crate::notify::mattermost::dispatch_or_update_mattermost_card(
                            &mm_clone, Some(&target_post_id), None,
                            "🐕✨ Conduit Living Card (Updated In-Place)",
                            "Post was successfully updated in-place via Mattermost Bot Token!",
                            None, updated_fields, Some("#3B82F6"),
                        ).await;
                    });

                    return Ok(Json(json!({
                        "status": "ok",
                        "message": format!("Mattermost Bot card created (ID: {}) and in-place update scheduled!", post_id)
                    })));
                }
                Ok(None) => {}
                Err(e) => {
                    return Err((StatusCode::BAD_REQUEST, Json(json!({
                        "status": "error",
                        "message": format!("Mattermost Bot API error: {}", e)
                    }))));
                }
            }
        }
    }

    let test_msg = format!(
        "🐕 **Conduit Notification Test**\nTarget: `{}`\nTime: `{}`\nAll systems operational!",
        target.name,
        chrono::Utc::now().to_rfc3339()
    );

    let send_result: anyhow::Result<()> = match &target.channel {
        crate::config::NotificationChannel::Mattermost(mm) => {
            crate::notify::mattermost::send_mattermost_notification(mm, "Conduit Test Alert", &test_msg, Some("#0ea5e9")).await
        }
        crate::config::NotificationChannel::Discord(dc) => {
            crate::notify::discord::send_discord_notification(dc, "Conduit Test Alert", &test_msg, Some("#0ea5e9")).await
        }
        crate::config::NotificationChannel::Pushover(po) => {
            crate::notify::pushover::send_pushover_notification(po, "Conduit Test Alert", &test_msg).await
        }
        crate::config::NotificationChannel::Webhook(wh) => {
            crate::notify::webhook::send_generic_webhook(wh, "test", "Conduit Test Alert", &test_msg, None).await
        }
    };

    if let Err(e) = send_result {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "status": "error",
            "message": format!("Test notification to '{}' failed: {}", target.name, e)
        }))));
    }

    let _ = state.db.log_event(
        "notification",
        "info",
        &format!("Dispatched test notification to target '{}'", target.name),
        None,
    );

    Ok(Json(json!({
        "status": "ok",
        "message": format!("Test notification dispatched to {}", target.name)
    })))
}

#[utoipa::path(
    post,
    path = "/api/pipeline/test-regex",
    tag = "Settings",
    summary = "Test Pipeline Regex Pattern",
    description = "Tests whether a given tracker error string matches the specified regular expression.",
    request_body = TestRegexRequest,
    responses(
        (status = 200, description = "Regex evaluation result", body = TestRegexResponse)
    )
)]
pub async fn test_pipeline_regex(
    _admin: RequireAdmin,
    Json(req): Json<TestRegexRequest>,
) -> Result<Json<TestRegexResponse>, StatusCode> {
    match Regex::new(&req.pattern) {
        Ok(re) => {
            let matched = re.is_match(&req.test_string);
            Ok(Json(TestRegexResponse {
                matched,
                pattern_valid: true,
                error: None,
            }))
        }
        Err(e) => Ok(Json(TestRegexResponse {
            matched: false,
            pattern_valid: false,
            error: Some(e.to_string()),
        })),
    }
}
