// src/api/metrics_routes.rs
use crate::auth::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
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
        (status = 200, description = "Prometheus metrics text", content_type = "text/plain")
    )
)]
pub async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
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
pub async fn get_system_stats() -> Result<Json<HostSystemStats>, StatusCode> {
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
pub async fn get_system_health(State(state): State<AppState>) -> Result<Json<crate::fetcher::SystemHealthOverview>, StatusCode> {
    let overview = state.fetcher_pool.get_system_health();
    Ok(Json(overview))
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
pub async fn get_engine_health(State(state): State<AppState>) -> Result<Json<Vec<crate::engines::registry::EngineStatus>>, StatusCode> {
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
        (status = 200, description = "Crash log output", body = CrashLogResponse)
    )
)]
pub async fn get_crash_log(State(state): State<AppState>) -> Result<Json<CrashLogResponse>, StatusCode> {
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

