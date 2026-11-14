// src/api/stats_routes.rs
use crate::auth::{AppState, RequireRead};
use crate::db::Database;
use crate::stats::{self, PeerClientSnapshot, StatsSnapshot};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use utoipa::IntoParams;

#[derive(Debug, Deserialize, IntoParams)]
pub struct StatsHistoryQueryParams {
    /// How far back to look, in hours. Clamped to [1, 720] (30 days -- see
    /// `Database::insert_stats_snapshot`'s prune window). Defaults to 24.
    pub hours: Option<u32>,
}

#[utoipa::path(
    get,
    path = "/api/stats/current",
    tag = "Stats",
    summary = "Get live cluster stats totals & breakdowns",
    description = "Computes a fresh stats snapshot (totals, plus breakdowns by fetcher node, tracker, and media type) from whatever the poller already has cached -- no extra network calls. Distinct from the remote-peer-client breakdown, which is sampled periodically rather than computed live (see /api/stats/peer-clients/history).",
    responses(
        (status = 200, description = "Live stats snapshot", body = StatsSnapshot),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn get_current_stats(_auth: RequireRead, State(state): State<AppState>) -> Json<StatsSnapshot> {
    Json(stats::compute_stats_snapshot(&state.fetcher_pool, &state.db))
}

#[utoipa::path(
    get,
    path = "/api/stats/history",
    tag = "Stats",
    summary = "Get historical stats snapshots",
    description = "Returns periodic stats snapshots (one every ~5 minutes, see engines::stats_history) for up to the last 30 days, backing the Stats page's trend charts.",
    params(StatsHistoryQueryParams),
    responses(
        (status = 200, description = "Stats history, oldest first", body = [StatsSnapshot]),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn get_stats_history(
    _auth: RequireRead,
    State(db): State<Database>,
    Query(params): Query<StatsHistoryQueryParams>,
) -> Result<Json<Vec<StatsSnapshot>>, StatusCode> {
    let hours = params.hours.unwrap_or(24).clamp(1, 720) as i64;
    let since_ms = chrono::Utc::now().timestamp_millis() - hours * 3600 * 1000;
    let history = db.list_stats_history_async(since_ms).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(history))
}

#[utoipa::path(
    get,
    path = "/api/stats/peer-clients/history",
    tag = "Stats",
    summary = "Get historical remote peer-client breakdown samples",
    description = "Returns periodic peer-client-name breakdown samples (one every ~15 minutes, see engines::stats_history and stats::sample_peer_clients). There's no live/on-demand variant of this endpoint -- computing it fresh means fetching full peer lists per torrent, which is deliberately rate-limited to this background sampler. The frontend treats the last element as \"current\".",
    params(StatsHistoryQueryParams),
    responses(
        (status = 200, description = "Peer-client history, oldest first", body = [PeerClientSnapshot]),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn get_peer_client_history(
    _auth: RequireRead,
    State(db): State<Database>,
    Query(params): Query<StatsHistoryQueryParams>,
) -> Result<Json<Vec<PeerClientSnapshot>>, StatusCode> {
    let hours = params.hours.unwrap_or(24).clamp(1, 720) as i64;
    let since_ms = chrono::Utc::now().timestamp_millis() - hours * 3600 * 1000;
    let history = db.list_peer_client_history_async(since_ms).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(history))
}
