// src/api/metrics_routes.rs
use crate::auth::{AppState, OptionalAuth, RequireAuth};
use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use serde_json::json;
use sysinfo::System;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct HostSystemStats {
    /// Conduit daemon version
    pub version: String,
    /// Host CPU cores count
    pub cpu_count: usize,
    /// Total system RAM in bytes
    pub total_memory_bytes: u64,
    /// Used system RAM in bytes
    pub used_memory_bytes: u64,
    /// System uptime in seconds
    pub uptime_secs: u64,
}

#[utoipa::path(
    get,
    path = "/metrics",
    tag = "Metrics",
    summary = "Prometheus metrics exposition",
    description = "Prometheus text exposition format endpoint exposing aggregate speeds, torrent counts, and storage metrics.",
    responses(
        (status = 200, description = "Prometheus metrics text", content_type = "text/plain"),
        (status = 401, description = "Authentication required (unless system.metrics_public is set)")
    )
)]
pub async fn prometheus_metrics(State(state): State<AppState>, OptionalAuth(auth): OptionalAuth) -> axum::response::Response {
    // The exposition lists node names and counts: it needs a credential (a bearer or API token
    // works for Prometheus) unless `system.metrics_public` says otherwise.
    if auth.is_none() && !state.config.get().await.system.metrics_public {
        return (
            StatusCode::UNAUTHORIZED,
            [(axum::http::header::WWW_AUTHENTICATE, "Bearer")],
            "metrics require authentication (send a bearer token, or set system.metrics_public)\n",
        )
            .into_response();
    }
    let stats = state.fetcher_pool.get_aggregate_stats();
    let health = state.fetcher_pool.get_system_health();
    let config = state.config.get().await;

    let mut output = String::new();

    // 1. Application Info
    output.push_str("# HELP conduit_app_info Application build and version info\n");
    output.push_str("# TYPE conduit_app_info gauge\n");
    output.push_str(&format!("conduit_app_info{{version=\"{}\"}} 1\n", env!("CARGO_PKG_VERSION")));

    // 2. Aggregate Cluster Metrics
    output.push_str("# HELP conduit_torrents_total Total torrent count across all nodes\n");
    output.push_str("# TYPE conduit_torrents_total gauge\n");
    output.push_str(&format!("conduit_torrents_total {}\n", stats.total_torrents));

    output.push_str("# HELP conduit_download_bytes_per_sec Aggregated download speed\n");
    output.push_str("# TYPE conduit_download_bytes_per_sec gauge\n");
    output.push_str(&format!("conduit_download_bytes_per_sec {}\n", stats.total_download_speed));

    output.push_str("# HELP conduit_upload_bytes_per_sec Aggregated upload speed\n");
    output.push_str("# TYPE conduit_upload_bytes_per_sec gauge\n");
    output.push_str(&format!("conduit_upload_bytes_per_sec {}\n", stats.total_upload_speed));

    output.push_str("# HELP conduit_nodes_connected Connected node count\n");
    output.push_str("# TYPE conduit_nodes_connected gauge\n");
    output.push_str(&format!("conduit_nodes_connected {}\n", stats.connected_nodes));

    output.push_str("# HELP conduit_nodes_total Total configured node count\n");
    output.push_str("# TYPE conduit_nodes_total gauge\n");
    output.push_str(&format!("conduit_nodes_total {}\n", stats.total_nodes));

    output.push_str("# HELP conduit_storage_total_bytes Total storage managed by torrents\n");
    output.push_str("# TYPE conduit_storage_total_bytes gauge\n");
    output.push_str(&format!("conduit_storage_total_bytes {}\n", stats.total_size_bytes));

    // 3. Per-Node Telemetry
    output.push_str("# HELP conduit_node_connected Connection status of fetcher node (1 = online, 0 = offline)\n");
    output.push_str("# TYPE conduit_node_connected gauge\n");
    for node in &health.nodes {
        let is_conn = if node.connected { 1 } else { 0 };
        output.push_str(&format!("conduit_node_connected{{node=\"{}\"}} {}\n", node.name, is_conn));
    }

    output.push_str("# HELP conduit_node_torrents Number of torrents on fetcher node\n");
    output.push_str("# TYPE conduit_node_torrents gauge\n");
    for node in &health.nodes {
        output.push_str(&format!("conduit_node_torrents{{node=\"{}\"}} {}\n", node.name, node.total_torrents));
    }

    output.push_str("# HELP conduit_node_latency_ms RPC response latency in milliseconds\n");
    output.push_str("# TYPE conduit_node_latency_ms gauge\n");
    for node in &health.nodes {
        output.push_str(&format!("conduit_node_latency_ms{{node=\"{}\"}} {}\n", node.name, node.latency_ms));
    }

    // 4. External Services Telemetry
    output.push_str("# HELP conduit_service_enabled Configuration status of external service (1 = enabled, 0 = disabled)\n");
    output.push_str("# TYPE conduit_service_enabled gauge\n");
    output.push_str(&format!("conduit_service_enabled{{service=\"sonarr\"}} {}\n", if config.sonarr.enabled { 1 } else { 0 }));
    output.push_str(&format!("conduit_service_enabled{{service=\"radarr\"}} {}\n", if config.radarr.enabled { 1 } else { 0 }));
    output.push_str(&format!("conduit_service_enabled{{service=\"lidarr\"}} {}\n", if config.lidarr.enabled { 1 } else { 0 }));
    output.push_str(&format!("conduit_service_enabled{{service=\"plex\"}} {}\n", if config.plex.enabled { 1 } else { 0 }));
    output.push_str(&format!("conduit_service_enabled{{service=\"trakt\"}} {}\n", if config.trakt.enabled { 1 } else { 0 }));

    // 5. Circuit Breaker Swarm Metrics
    output.push_str("# HELP conduit_tracker_circuit_broken Status of tracker circuit breaker (1 = tripped/paused, 0 = normal)\n");
    output.push_str("# TYPE conduit_tracker_circuit_broken gauge\n");
    for tr in &health.trackers {
        let broken = if tr.is_circuit_broken { 1 } else { 0 };
        output.push_str(&format!("conduit_tracker_circuit_broken{{tracker=\"{}\"}} {}\n", tr.host, broken));
    }

    output.push_str("# HELP conduit_tracker_paused_torrents Number of paused torrents on tracker\n");
    output.push_str("# TYPE conduit_tracker_paused_torrents gauge\n");
    for tr in &health.trackers {
        output.push_str(&format!("conduit_tracker_paused_torrents{{tracker=\"{}\"}} {}\n", tr.host, tr.paused_torrents));
    }

    // 6. Host Hardware Metrics
    let mut sys = System::new_all();
    sys.refresh_all();
    output.push_str("# HELP conduit_host_cpu_cores Number of CPU cores on host\n");
    output.push_str("# TYPE conduit_host_cpu_cores gauge\n");
    output.push_str(&format!("conduit_host_cpu_cores {}\n", sys.cpus().len()));

    output.push_str("# HELP conduit_host_memory_total_bytes Total physical RAM in bytes\n");
    output.push_str("# TYPE conduit_host_memory_total_bytes gauge\n");
    output.push_str(&format!("conduit_host_memory_total_bytes {}\n", sys.total_memory()));

    output.push_str("# HELP conduit_host_memory_used_bytes Used physical RAM in bytes\n");
    output.push_str("# TYPE conduit_host_memory_used_bytes gauge\n");
    output.push_str(&format!("conduit_host_memory_used_bytes {}\n", sys.used_memory()));

    output.push_str("# HELP conduit_host_uptime_seconds System uptime in seconds\n");
    output.push_str("# TYPE conduit_host_uptime_seconds gauge\n");
    output.push_str(&format!("conduit_host_uptime_seconds {}\n", System::uptime()));

    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        output,
    )
        .into_response()
}

#[utoipa::path(
    get,
    path = "/api/system/stats",
    tag = "Metrics",
    summary = "Get host hardware telemetry",
    description = "Returns host CPU, memory utilization, and system uptime.",
    responses(
        (status = 200, description = "Host hardware telemetry", body = HostSystemStats)
    )
)]
pub async fn get_system_stats(_auth: RequireAuth) -> Result<Json<HostSystemStats>, StatusCode> {
    let mut sys = System::new_all();
    sys.refresh_all();

    Ok(Json(HostSystemStats {
        version: env!("CARGO_PKG_VERSION").to_string(),
        cpu_count: sys.cpus().len(),
        total_memory_bytes: sys.total_memory(),
        used_memory_bytes: sys.used_memory(),
        uptime_secs: System::uptime(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/system/health",
    tag = "Metrics",
    summary = "Get cluster & tracker swarm health panel telemetry",
    description = "Returns real-time node connectivity, response latency, active circuit-broken trackers, canary probes, and swarm error responses.",
    responses(
        (status = 200, description = "Cluster and tracker health telemetry", body = crate::fetcher::SystemHealthOverview)
    )
)]
pub async fn get_system_health(_auth: RequireAuth, State(state): State<AppState>) -> Result<Json<crate::fetcher::SystemHealthOverview>, StatusCode> {
    let overview = state.fetcher_pool.get_system_health();
    Ok(Json(overview))
}

/// The node(s) whose native breaker is authoritative for this tracker host, or `None` if
/// no node touching it is in passive mode (i.e. Conduit's own active breaker owns it) —
/// mirrors the "all nodes touching this host are passive" rule the health engine and
/// dashboard use, so an override always lands wherever the displayed status says it will.
fn passive_owner_nodes(pool: &crate::fetcher::FetcherPool, host: &str) -> Option<Vec<String>> {
    let torrents = pool.get_torrents(None, None, None);
    let mut nodes: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut touched = false;
    for t in &torrents {
        if t.tracker_stats.iter().any(|ts| ts.host == host) {
            touched = true;
            if !pool.is_passive_mode(&t.node) {
                return None;
            }
            nodes.insert(t.node.clone());
        }
    }
    if touched && !nodes.is_empty() {
        Some(nodes.into_iter().collect())
    } else {
        None
    }
}

#[utoipa::path(
    post,
    path = "/api/system/circuit-breakers/{host}/trip",
    tag = "Metrics",
    summary = "Force-trip a tracker's circuit breaker",
    description = "Forces the given tracker host's circuit breaker into a tripped state. Routed to the owning node's native breaker in passive mode (see /api/system/health's breaker_mode field), or applied directly to Conduit's own external breaker in active mode.",
    params(
        ("host" = String, Path, description = "Tracker hostname")
    ),
    responses(
        (status = 200, description = "Circuit breaker force-tripped"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn force_trip_circuit_breaker(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Path(host): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if let Some(owning_nodes) = passive_owner_nodes(&state.fetcher_pool, &host) {
        for node in &owning_nodes {
            if let Some(client) = state.fetcher_pool.get_client(node) {
                let _ = client.force_trip_circuit_breaker(&host).await;
            }
        }
        return Ok(Json(json!({
            "message": format!("Circuit breaker for '{}' force-tripped via {}", host, owning_nodes.join(", "))
        })));
    }

    // Active mode: pause every torrent currently announcing to this tracker and record a
    // synthetic breaker. No real canary is designated (this is a manual override, not a
    // failure-detected trip), so it holds for one backoff window and then auto-clears on
    // the next tick since there's nothing left to actively re-check.
    let torrents = state.fetcher_pool.get_torrents(None, None, None);
    let mut paused = Vec::new();
    for t in &torrents {
        if t.tracker_stats.iter().any(|ts| ts.host == host) && t.raw_status != 0 {
            if let Some(client) = state.fetcher_pool.get_client(&t.node) {
                if client.stop_torrents(&[t.id]).await.is_ok() {
                    paused.push(format!("{}:{}", t.node, t.id));
                }
            }
        }
    }
    let breaker = crate::fetcher::ActiveCircuitBreaker {
        tracker_host: host.clone(),
        canary_compound_id: String::new(),
        canary_name: String::new(),
        paused_torrents: paused,
        failing_error: "Manually force-tripped".to_string(),
        tripped_at: chrono::Utc::now().timestamp(),
        state: "tripped".to_string(),
        recovery_started_at: None,
        consecutive_successes: 0,
        backoff_secs: 0,
    };
    let _ = state.db.save_circuit_breaker(&breaker);
    state.fetcher_pool.set_active_breaker(host.clone(), breaker);
    Ok(Json(json!({"message": format!("Circuit breaker for '{}' force-tripped", host)})))
}

#[utoipa::path(
    post,
    path = "/api/system/circuit-breakers/{host}/reset",
    tag = "Metrics",
    summary = "Force-reset a tracker's circuit breaker",
    description = "Clears the given tracker host's circuit breaker state entirely and resumes any torrents it had paused. Routed to the owning node's native breaker in passive mode, or applied directly to Conduit's own external breaker in active mode.",
    params(
        ("host" = String, Path, description = "Tracker hostname")
    ),
    responses(
        (status = 200, description = "Circuit breaker force-reset"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn force_reset_circuit_breaker(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Path(host): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if let Some(owning_nodes) = passive_owner_nodes(&state.fetcher_pool, &host) {
        for node in &owning_nodes {
            if let Some(client) = state.fetcher_pool.get_client(node) {
                let _ = client.force_reset_circuit_breaker(&host).await;
            }
        }
        return Ok(Json(json!({
            "message": format!("Circuit breaker for '{}' force-reset via {}", host, owning_nodes.join(", "))
        })));
    }

    if let Some(breaker) = state.fetcher_pool.get_active_breakers().get(&host).cloned() {
        for cid in &breaker.paused_torrents {
            if let Some((node, id_str)) = cid.split_once(':') {
                if let Ok(id) = id_str.parse::<i64>() {
                    if let Some(client) = state.fetcher_pool.get_client(node) {
                        let _ = client.start_torrents(&[id], false).await;
                    }
                }
            }
        }
    }
    let _ = state.db.remove_circuit_breaker(&host);
    state.fetcher_pool.remove_active_breaker(&host);
    Ok(Json(json!({"message": format!("Circuit breaker for '{}' force-reset", host)})))
}

#[utoipa::path(
    get,
    path = "/api/system/engines",
    tag = "Metrics",
    summary = "Get background engine / task scheduler health ('Platform Health')",
    description = "Returns the state, last heartbeat, tick count, and restart count for every background engine (pollers, sync loops, the space manager, etc.) — visibility into Conduit's own internal task scheduler, not the Arr stack or fetcher nodes.",
    responses(
        (status = 200, description = "Background engine health snapshot", body = [crate::engines::registry::EngineStatus])
    )
)]
pub async fn get_engine_health(_auth: RequireAuth, State(state): State<AppState>) -> Result<Json<Vec<crate::engines::registry::EngineStatus>>, StatusCode> {
    Ok(Json(crate::engines::registry::snapshot(&state.engine_registry)))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CrashLogResponse {
    pub has_crashes: bool,
    pub log_content: String,
    pub crash_file_path: String,
}

#[utoipa::path(
    get,
    path = "/api/system/crash-log",
    tag = "Metrics",
    summary = "Retrieve fatal crash and panic diagnostics log",
    description = "Returns captured panic stack traces and timestamps to aid post-mortem crash diagnostics.",
    responses(
        (status = 200, description = "Crash log output", body = CrashLogResponse),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn get_crash_log(_auth: RequireAuth, State(state): State<AppState>) -> Result<Json<CrashLogResponse>, StatusCode> {
    let cfg = state.config.get().await;
    let crash_file = std::path::Path::new(&cfg.system.data_dir).join("crash.log");
    let crash_file_path = crash_file.to_string_lossy().to_string();

    if crash_file.exists() {
        match tokio::fs::read_to_string(&crash_file).await {
            Ok(content) => Ok(Json(CrashLogResponse {
                has_crashes: !content.trim().is_empty(),
                log_content: content,
                crash_file_path,
            })),
            Err(_) => Ok(Json(CrashLogResponse {
                has_crashes: false,
                log_content: "Failed to read crash log file".to_string(),
                crash_file_path,
            })),
        }
    } else {
        Ok(Json(CrashLogResponse {
            has_crashes: false,
            log_content: "No crash logs recorded. System running cleanly.".to_string(),
            crash_file_path,
        }))
    }
}

