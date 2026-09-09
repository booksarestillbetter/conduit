// src/api/plex_oauth_routes.rs
use crate::auth::RequireAdmin;
use crate::config::{ConfigManager, PlexNodeConfig};
use crate::plex_client;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct PlexPinResponse {
    pub pin_id: i64,
    pub code: String,
    /// Open this in a popup/new tab — it's Plex's own hosted sign-in page; the user logs in and
    /// approves there, then this endpoint's PIN can be polled via `/api/plex/oauth/poll/{pin_id}`.
    pub signin_url: String,
}

#[utoipa::path(
    post,
    path = "/api/plex/oauth/pin",
    tag = "Settings",
    summary = "Start Plex account OAuth linking",
    description = "Creates a plex.tv PIN identifying Conduit as its own registered device (via a stable X-Plex-Client-Identifier generated once at startup) and returns the sign-in URL to open in a popup — step 1 of linking a Plex account, replacing manual X-Plex-Token entry. This is the same PIN-based device-linking flow Sonarr/Radarr/Ombi use.",
    responses(
        (status = 200, description = "PIN created", body = PlexPinResponse),
        (status = 401, description = "Unauthorized — admin session required"),
        (status = 502, description = "Failed to reach plex.tv")
    )
)]
pub async fn create_oauth_pin(
    _admin: RequireAdmin,
    State(config_mgr): State<ConfigManager>,
) -> Result<Json<PlexPinResponse>, (StatusCode, Json<serde_json::Value>)> {
    let config = config_mgr.get().await;
    let client_id = config.plex.client_identifier.clone();

    let pin = plex_client::create_pin(&client_id)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": format!("Failed to create Plex PIN: {}", e)}))))?;

    let signin_url = plex_client::build_signin_url(&client_id, &pin.code, "");

    Ok(Json(PlexPinResponse { pin_id: pin.id, code: pin.code, signin_url }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlexPinPollResponse {
    pub authenticated: bool,
    /// Only present once `authenticated` is true — the Plex account token, which works directly
    /// as `X-Plex-Token` against any of the account's own servers with no further exchange.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_token: Option<String>,
    /// The account's owned Plex servers, ready to offer as one-click "Add" options — only
    /// present once `authenticated` is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<plex_client::PlexDiscoveredServer>>,
}

#[utoipa::path(
    get,
    path = "/api/plex/oauth/poll/{pin_id}",
    tag = "Settings",
    summary = "Poll a Plex OAuth PIN for approval",
    description = "Checks whether the user has approved the PIN at plex.tv yet (poll every ~2s from the frontend while the sign-in popup is open). Once approved, also discovers the account's owned Plex servers via plex.tv's resource listing so the frontend can offer them for one-click adding.",
    params(("pin_id" = i64, Path, description = "PIN id returned by POST /api/plex/oauth/pin")),
    responses(
        (status = 200, description = "Poll result — check `authenticated`", body = PlexPinPollResponse),
        (status = 401, description = "Unauthorized — admin session required"),
        (status = 502, description = "Failed to reach plex.tv")
    )
)]
pub async fn poll_oauth_pin(
    _admin: RequireAdmin,
    State(config_mgr): State<ConfigManager>,
    Path(pin_id): Path<i64>,
) -> Result<Json<PlexPinPollResponse>, (StatusCode, Json<serde_json::Value>)> {
    let config = config_mgr.get().await;
    let client_id = config.plex.client_identifier.clone();

    let pin = plex_client::poll_pin(&client_id, pin_id)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": format!("Failed to poll Plex PIN: {}", e)}))))?;

    let Some(token) = pin.auth_token.filter(|t| !t.is_empty()) else {
        return Ok(Json(PlexPinPollResponse { authenticated: false, account_token: None, servers: None }));
    };

    let servers = plex_client::discover_owned_servers(&client_id, &token)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": format!("Authenticated, but failed to discover Plex servers: {}", e)}))))?;

    Ok(Json(PlexPinPollResponse { authenticated: true, account_token: Some(token), servers: Some(servers) }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddPlexServerRequest {
    pub name: String,
    pub url: String,
    pub token: String,
}

#[utoipa::path(
    post,
    path = "/api/plex/oauth/add-server",
    tag = "Settings",
    summary = "Add an OAuth-discovered Plex server",
    description = "Persists a Plex server discovered via the OAuth flow as a new configured Plex node (PlexConfig.nodes), using the linked account token as that node's token_override. Also enables the Plex integration if it wasn't already.",
    request_body = AddPlexServerRequest,
    responses(
        (status = 200, description = "Server added and configuration saved"),
        (status = 401, description = "Unauthorized — admin session required"),
        (status = 500, description = "Failed to persist configuration")
    )
)]
pub async fn add_oauth_plex_server(
    _admin: RequireAdmin,
    State(config_mgr): State<ConfigManager>,
    Json(req): Json<AddPlexServerRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut config = config_mgr.get().await;
    config.plex.enabled = true;
    config.plex.nodes.push(PlexNodeConfig {
        name: req.name,
        url: req.url,
        token_override: Some(req.token),
        sync_watch_status: false,
        server_uuid: None,
    });
    config_mgr
        .update(config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    Ok(Json(json!({"message": "Plex server added"})))
}
