// src/api/node_routes.rs
use crate::auth::RequireAuth;
use crate::fetcher::{NodeStats, FetcherPool};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::json;
use std::sync::Arc;

#[utoipa::path(
    get,
    path = "/api/nodes",
    tag = "Nodes",
    summary = "List all fetcher nodes",
    description = "Returns the status, connection latency, and storage stats for all configured nodes.",
    responses(
        (status = 200, description = "List of node statuses", body = [NodeStats]),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn list_nodes(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
) -> Json<Vec<NodeStats>> {
    let stats = pool.get_aggregate_stats();
    Json(stats.nodes)
}

#[utoipa::path(
    get,
    path = "/api/nodes/{name}/session",
    tag = "Nodes",
    summary = "Get fetcher daemon session settings",
    description = "Queries the remote fetcher daemon (Transmission, qBittorrent, or Deluge RPC — not supported on Synapse) for session settings (speed limits, alt-speeds, peer limits, dirs).",
    params(
        ("name" = String, Path, description = "Configured node name (e.g. 'idyll')")
    ),
    responses(
        (status = 200, description = "Daemon session settings dictionary"),
        (status = 404, description = "Node not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn get_node_session(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client = pool.get_client(&name)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Node not found"}))))?;

    let session = client.get_session().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(session))
}

#[utoipa::path(
    put,
    path = "/api/nodes/{name}/session",
    tag = "Nodes",
    summary = "Update fetcher daemon session settings",
    description = "Applies configuration changes directly to the remote fetcher daemon (Transmission, qBittorrent, or Deluge — not supported on Synapse) via RPC.",
    params(
        ("name" = String, Path, description = "Configured node name")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Session updated successfully"),
        (status = 404, description = "Node not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn update_node_session(
    _auth: crate::auth::RequireAdmin,
    State(pool): State<Arc<FetcherPool>>,
    Path(name): Path<String>,
    Json(settings): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client = pool.get_client(&name)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Node not found"}))))?;

    client.set_session(settings).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({"message": "Node session updated"})))
}

#[utoipa::path(
    post,
    path = "/api/nodes/{name}/test-port",
    tag = "Nodes",
    summary = "Test incoming peer port",
    description = "Instructs the fetcher daemon to test if its incoming peer listening port is open to the internet.",
    params(
        ("name" = String, Path, description = "Configured node name")
    ),
    responses(
        (status = 200, description = "Port test result"),
        (status = 404, description = "Node not found"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn test_node_port(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client = pool.get_client(&name)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Node not found"}))))?;

    let is_open = client.test_port().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({"port_is_open": is_open})))
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct TurtleModePayload {
    pub enabled: bool,
}

#[utoipa::path(
    post,
    path = "/api/nodes/{name}/turtle-mode",
    tag = "Nodes",
    summary = "Toggle Turtle Mode (Alternative Speed Limits) for a node",
    params(
        ("name" = String, Path, description = "Node name")
    ),
    request_body = TurtleModePayload,
    responses(
        (status = 200, description = "Turtle mode updated"),
        (status = 404, description = "Node not found")
    )
)]
pub async fn set_turtle_mode(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Path(name): Path<String>,
    Json(payload): Json<TurtleModePayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client = pool.get_client(&name)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Node not found"}))))?;

    client.set_turtle_mode(payload.enabled).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({
        "node": name,
        "alt_speed_enabled": payload.enabled,
        "message": format!("Turtle mode {}", if payload.enabled { "enabled" } else { "disabled" })
    })))
}

#[utoipa::path(
    post,
    path = "/api/nodes/turtle-mode",
    tag = "Nodes",
    summary = "Toggle Turtle Mode for all connected nodes",
    request_body = TurtleModePayload,
    responses(
        (status = 200, description = "Turtle mode applied to all nodes")
    )
)]
pub async fn set_turtle_mode_all(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Json(payload): Json<TurtleModePayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut updated = Vec::new();
    for client in pool.get_all_clients() {
        if client.set_turtle_mode(payload.enabled).await.is_ok() {
            updated.push(client.node_name().to_string());
        }
    }

    Ok(Json(json!({
        "updated_nodes": updated,
        "alt_speed_enabled": payload.enabled
    })))
}

#[utoipa::path(
    post,
    path = "/api/nodes/{name}/blocklist-update",
    tag = "Nodes",
    summary = "Trigger daemon-side blocklist update",
    params(
        ("name" = String, Path, description = "Node name")
    ),
    responses(
        (status = 200, description = "Blocklist updated with rule count"),
        (status = 404, description = "Node not found")
    )
)]
pub async fn update_node_blocklist(
    _auth: RequireAuth,
    State(pool): State<Arc<FetcherPool>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client = pool.get_client(&name)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "Node not found"}))))?;

    let count = client.update_blocklist().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({
        "node": name,
        "blocklist_size": count,
        "message": format!("Blocklist updated ({} rules active)", count)
    })))
}
