// src/fetcher/pool.rs
use super::models::{
    ActiveCircuitBreaker, AggregateStats, BandwidthPoint, BulkActionType, BulkItemResult,
    BulkTorrentActionPayload, NodeHealthStatus, NodeStats, SystemHealthOverview, Torrent,
    TorrentBandwidthContributor, TorrentStatus, TrackerHealthStatus, UnifiedTorrent,
};
use crate::config::AppConfig;
use crate::downloader::traits::TorrentClientTrait;
use fetcher_core::{NativeCircuitBreakerStatus, NodeCapabilities, RetrieverClientType};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// How long a detected `NodeCapabilities` snapshot stays valid before re-probing — long
/// enough to avoid an extra RPC on every poll tick, short enough that an in-place daemon
/// upgrade/downgrade is picked up without restarting Conduit.
const CAPABILITIES_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CachedNodeData {
    pub connected: bool,
    pub last_updated: Instant,
    pub latency_ms: u64,
    pub free_space_bytes: i64,
    pub torrents: Vec<UnifiedTorrent>,
    pub stats: NodeStats,
}

#[derive(Debug, Clone)]
pub struct NodeBackoff {
    pub last_poll: Instant,
    pub consecutive_errors: u32,
    pub backoff_duration: Duration,
    pub last_latency: Duration,
}

pub struct FetcherPool {
    clients: RwLock<HashMap<String, Arc<dyn TorrentClientTrait>>>,
    cache: RwLock<HashMap<String, CachedNodeData>>,
    in_flight: Mutex<HashSet<String>>,
    backoffs: RwLock<HashMap<String, NodeBackoff>>,
    active_breakers: RwLock<HashMap<String, ActiveCircuitBreaker>>,
    bandwidth_history: RwLock<VecDeque<BandwidthPoint>>,
    /// In-memory only, deliberately not persisted to `FetcherNodeConfig` (see that struct's
    /// `PartialEq`-based client-rebuild check) — re-detected on every poll cycle, keyed by
    /// node name, value is `(capabilities, detected_at)`.
    capabilities: RwLock<HashMap<String, (NodeCapabilities, Instant)>>,
    /// Live native circuit breaker status for passive-mode nodes, keyed by node name.
    /// Refreshed on every successful poll of that node (not TTL-throttled like
    /// `capabilities` — this is the actual live status data the UI displays).
    native_breakers: RwLock<HashMap<String, Vec<NativeCircuitBreakerStatus>>>,
}

impl Default for FetcherPool {
    fn default() -> Self {
        Self::new()
    }
}

impl FetcherPool {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            cache: RwLock::new(HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            backoffs: RwLock::new(HashMap::new()),
            active_breakers: RwLock::new(HashMap::new()),
            bandwidth_history: RwLock::new(VecDeque::with_capacity(300)),
            capabilities: RwLock::new(HashMap::new()),
            native_breakers: RwLock::new(HashMap::new()),
        }
    }

    pub fn sync_nodes_from_config(&self, config: &AppConfig) {
        let mut clients_write = self.clients.write();
        let mut cache_write = self.cache.write();

        // Remove disabled or deleted nodes
        clients_write.retain(|name, _| {
            config.nodes.get(name).map(|n| n.enabled).unwrap_or(false)
        });
        cache_write.retain(|name, _| {
            config.nodes.get(name).map(|n| n.enabled).unwrap_or(false)
        });

        // Add or update nodes only when config actually changed or client missing
        for (name, node_cfg) in &config.nodes {
            if node_cfg.enabled {
                let needs_new_client = match clients_write.get(name) {
                    Some(existing_client) => existing_client.config() != node_cfg,
                    None => true,
                };

                if needs_new_client {
                    clients_write.insert(name.clone(), crate::downloader::create_client(node_cfg.clone()));
                }

                // Ensure an initial entry in cache exists so nodes appear immediately on dashboard
                if !cache_write.contains_key(name) {
                    cache_write.insert(
                        name.clone(),
                        CachedNodeData {
                            connected: false,
                            last_updated: std::time::Instant::now(),
                            latency_ms: 0,
                            free_space_bytes: 0,
                            torrents: Vec::new(),
                            stats: NodeStats {
                                node: name.clone(),
                                client_type: node_cfg.client_type,
                                connected: false,
                                latency_ms: 0,
                                free_space_bytes: 0,
                                free_space_gb: 0.0,
                                total_torrents: 0,
                                active_torrents: 0,
                                paused_torrents: 0,
                                error_torrents: 0,
                                rate_upload: 0,
                                rate_download: 0,
                                ratio_0: 0,
                                ratio_lt1: 0,
                                ratio_gt1: 0,
                                ratio_gt2: 0,
                                alt_speed_enabled: false,
                                blocklist_size: 0,
                                blocklist_enabled: false,
                            },
                        },
                    );
                }
            } else {
                clients_write.remove(name);
                cache_write.remove(name);
            }
        }
    }

    pub fn should_poll_node(&self, node_name: &str, _base_interval_secs: u64) -> bool {
        // 1. Never stack requests: if a request is already in-flight for this node, skip
        if self.in_flight.lock().contains(node_name) {
            return false;
        }

        let backoffs = self.backoffs.read();
        if let Some(bo) = backoffs.get(node_name) {
            let elapsed = bo.last_poll.elapsed();

            // 2. Check if error backoff or normal interval has passed
            if elapsed < bo.backoff_duration {
                return false;
            }

            // 3. Adaptive latency pacing: if daemon response time is slow,
            // never query faster than its actual response latency + 500ms headroom
            let min_adaptive_interval = bo.last_latency + Duration::from_millis(500);
            if elapsed < min_adaptive_interval {
                return false;
            }
        }

        true
    }

    pub fn get_client(&self, node_name: &str) -> Option<Arc<dyn TorrentClientTrait>> {
        self.clients.read().get(node_name).cloned()
    }

    pub fn list_clients(&self) -> Vec<Arc<dyn TorrentClientTrait>> {
        self.clients.read().values().cloned().collect()
    }

    pub fn get_all_clients(&self) -> Vec<Arc<dyn TorrentClientTrait>> {
        self.list_clients()
    }

    /// Re-detects a node's capabilities if the cached snapshot is missing or stale, and —
    /// for nodes already known to be in passive mode — refreshes their live native breaker
    /// status on every call. Cheap no-op for backends that don't override these trait
    /// methods (the default impls return instantly with no I/O), but still gated on client
    /// type to skip the calls — and the lock writes — entirely for backends that can never
    /// have this.
    async fn maybe_probe_capabilities(&self, node_name: &str, client: &Arc<dyn TorrentClientTrait>) {
        if client.client_type() != RetrieverClientType::Synapse {
            return;
        }
        let needs_probe = match self.capabilities.read().get(node_name) {
            Some((_, detected_at)) => detected_at.elapsed() >= CAPABILITIES_TTL,
            None => true,
        };
        if needs_probe {
            if let Ok(caps) = client.get_capabilities().await {
                self.capabilities.write().insert(node_name.to_string(), (caps, Instant::now()));
            }
        }

        if self.is_passive_mode(node_name) {
            if let Ok(breakers) = client.list_circuit_breakers().await {
                self.native_breakers.write().insert(node_name.to_string(), breakers);
            }
        } else {
            self.native_breakers.write().remove(node_name);
        }
    }

    /// The most recently detected capabilities for a node, or an all-`false` default if
    /// it hasn't been probed yet (e.g. right after startup, before the first successful
    /// poll).
    pub fn get_node_capabilities(&self, node_name: &str) -> NodeCapabilities {
        self.capabilities
            .read()
            .get(node_name)
            .map(|(caps, _)| *caps)
            .unwrap_or_default()
    }

    /// True if this node's own daemon should be trusted to manage its tracker circuit
    /// breaker itself (Conduit reads/displays/overrides, but doesn't independently pause
    /// or resume torrents on it).
    pub fn is_passive_mode(&self, node_name: &str) -> bool {
        self.get_node_capabilities(node_name).native_tracker_circuit_breaker
    }

    /// The most recently fetched native breaker status list for a passive-mode node.
    /// Empty for active-mode nodes or nodes that haven't been probed yet.
    pub fn get_native_breakers(&self, node_name: &str) -> Vec<NativeCircuitBreakerStatus> {
        self.native_breakers.read().get(node_name).cloned().unwrap_or_default()
    }

    /// All passive-mode nodes' native breaker statuses, flattened to `(node_name, status)`
    /// pairs.
    pub fn get_all_native_breakers(&self) -> Vec<(String, NativeCircuitBreakerStatus)> {
        self.native_breakers
            .read()
            .iter()
            .flat_map(|(node, list)| list.iter().map(move |s| (node.clone(), s.clone())))
            .collect()
    }

    pub async fn poll_node(&self, client: Arc<dyn TorrentClientTrait>) -> anyhow::Result<()> {
        let node_name = client.node_name().to_string();

        // 1. Acquire in-flight lock to guarantee at most 1 active request per node
        {
            let mut inflight = self.in_flight.lock();
            if !inflight.insert(node_name.clone()) {
                debug!("Poll already in-flight for node {}, skipping to prevent request piling", node_name);
                return Ok(());
            }
        }

        // RAII In-Flight Guard ensuring node is cleared even on error or panic
        struct InFlightGuard<'a> {
            pool: &'a FetcherPool,
            node: String,
        }
        impl<'a> Drop for InFlightGuard<'a> {
            fn drop(&mut self) {
                self.pool.in_flight.lock().remove(&self.node);
            }
        }
        let _guard = InFlightGuard {
            pool: self,
            node: node_name.clone(),
        };

        let start = Instant::now();
        let torrents_res = client.get_torrents(None).await;
        let latency = start.elapsed();
        let latency_ms = latency.as_millis() as u64;

        match torrents_res {
            Ok(torrents) => {
                // Record success & reset backoff
                {
                    let mut bo_write = self.backoffs.write();
                    bo_write.insert(node_name.clone(), NodeBackoff {
                        last_poll: Instant::now(),
                        consecutive_errors: 0,
                        backoff_duration: Duration::from_secs(1),
                        last_latency: latency,
                    });
                }

                self.maybe_probe_capabilities(&node_name, &client).await;

                let free_space_path = torrents
                    .first()
                    .map(|t| t.download_dir.clone())
                    .filter(|d| !d.is_empty())
                    .unwrap_or_else(|| "/".to_string());
                let free_space = client.get_free_space(&free_space_path).await.unwrap_or(0);
                let free_space_gb = (free_space as f64) / (1024.0 * 1024.0 * 1024.0);

                let mut unified: Vec<UnifiedTorrent> = Vec::with_capacity(torrents.len());
                let mut rate_up = 0i64;
                let mut rate_down = 0i64;
                let mut active_count = 0usize;
                let mut paused_count = 0usize;
                let mut error_count = 0usize;
                let mut ratio_0 = 0usize;
                let mut ratio_lt1 = 0usize;
                let mut ratio_gt1 = 0usize;
                let mut ratio_gt2 = 0usize;

                for t in torrents {
                    rate_up += t.rate_upload;
                    rate_down += t.rate_download;

                    let u = UnifiedTorrent::from_torrent(&node_name, t);
                    match u.status {
                        TorrentStatus::Downloading | TorrentStatus::Seeding => active_count += 1,
                        TorrentStatus::Stopped => paused_count += 1,
                        TorrentStatus::Error => error_count += 1,
                        _ => {}
                    }

                    if u.upload_ratio == 0.0 {
                        ratio_0 += 1;
                    } else if u.upload_ratio < 1.0 {
                        ratio_lt1 += 1;
                    } else if u.upload_ratio >= 1.0 && u.upload_ratio < 2.0 {
                        ratio_gt1 += 1;
                    } else if u.upload_ratio >= 2.0 {
                        ratio_gt2 += 1;
                    }

                    unified.push(u);
                }

                let total_torrents = unified.len();
                let stats = NodeStats {
                    node: node_name.clone(),
                    client_type: client.client_type(),
                    connected: true,
                    latency_ms,
                    free_space_bytes: free_space,
                    free_space_gb,
                    total_torrents,
                    active_torrents: active_count,
                    paused_torrents: paused_count,
                    error_torrents: error_count,
                    rate_upload: rate_up,
                    rate_download: rate_down,
                    ratio_0,
                    ratio_lt1,
                    ratio_gt1,
                    ratio_gt2,
                    alt_speed_enabled: false,
                    blocklist_size: 0,
                    blocklist_enabled: false,
                };

                let cached = CachedNodeData {
                    connected: true,
                    last_updated: Instant::now(),
                    latency_ms,
                    free_space_bytes: free_space,
                    torrents: unified,
                    stats,
                };

                self.cache.write().insert(node_name, cached);
                Ok(())
            }
            Err(e) => {
                // Record failure & apply exponential backoff (2s -> 4s -> 8s -> 16s -> max 30s)
                {
                    let mut bo_write = self.backoffs.write();
                    let entry = bo_write.entry(node_name.clone()).or_insert_with(|| NodeBackoff {
                        last_poll: Instant::now(),
                        consecutive_errors: 0,
                        backoff_duration: Duration::from_secs(2),
                        last_latency: latency,
                    });
                    entry.consecutive_errors += 1;
                    let multiplier = 2u64.saturating_pow(entry.consecutive_errors.min(5));
                    entry.backoff_duration = Duration::from_secs((2 * multiplier).min(30));
                    entry.last_poll = Instant::now();
                    entry.last_latency = latency;
                }

                warn!("Failed to poll fetcher node {}: {}", node_name, e);
                let mut cache_write = self.cache.write();
                if let Some(cached) = cache_write.get_mut(&node_name) {
                    cached.connected = false;
                    cached.stats.connected = false;
                } else {
                    cache_write.insert(
                        node_name.clone(),
                        CachedNodeData {
                            connected: false,
                            last_updated: Instant::now(),
                            latency_ms: 0,
                            free_space_bytes: 0,
                            torrents: Vec::new(),
                            stats: NodeStats {
                                node: node_name,
                                client_type: client.client_type(),
                                connected: false,
                                latency_ms: 0,
                                free_space_bytes: 0,
                                free_space_gb: 0.0,
                                total_torrents: 0,
                                active_torrents: 0,
                                paused_torrents: 0,
                                error_torrents: 0,
                                rate_upload: 0,
                                rate_download: 0,
                                ratio_0: 0,
                                ratio_lt1: 0,
                                ratio_gt1: 0,
                                ratio_gt2: 0,
                                alt_speed_enabled: false,
                                blocklist_size: 0,
                                blocklist_enabled: false,
                            },
                        },
                    );
                }
                Err(e)
            }
        }
    }

    pub fn update_node_cache(&self, node_name: &str, torrents: Vec<Torrent>, free_space_bytes: i64, latency_ms: u64) {
        let mut unified: Vec<UnifiedTorrent> = Vec::with_capacity(torrents.len());
        let mut rate_up = 0i64;
        let mut rate_download = 0i64;
        let mut active_count = 0usize;
        let mut paused_count = 0usize;
        let mut error_count = 0usize;
        let mut ratio_0 = 0usize;
        let mut ratio_lt1 = 0usize;
        let mut ratio_gt1 = 0usize;
        let mut ratio_gt2 = 0usize;

        for t in torrents {
            rate_up += t.rate_upload;
            rate_download += t.rate_download;

            let u = UnifiedTorrent::from_torrent(node_name, t);
            match u.status {
                TorrentStatus::Downloading | TorrentStatus::Seeding => active_count += 1,
                TorrentStatus::Stopped => paused_count += 1,
                TorrentStatus::Error => error_count += 1,
                _ => {}
            }

            if u.upload_ratio == 0.0 {
                ratio_0 += 1;
            } else if u.upload_ratio < 1.0 {
                ratio_lt1 += 1;
            } else if u.upload_ratio >= 1.0 && u.upload_ratio < 2.0 {
                ratio_gt1 += 1;
            } else {
                ratio_gt2 += 1;
            }

            unified.push(u);
        }

        let total_count = unified.len();
        let free_space_gb = (free_space_bytes as f64) / (1024.0 * 1024.0 * 1024.0);

        let mut cache_write = self.cache.write();
        cache_write.insert(
            node_name.to_string(),
            CachedNodeData {
                connected: true,
                last_updated: Instant::now(),
                latency_ms,
                free_space_bytes,
                torrents: unified,
                stats: NodeStats {
                    node: node_name.to_string(),
                    client_type: self.get_client(node_name).map(|c| c.client_type()).unwrap_or_default(),
                    connected: true,
                    latency_ms,
                    free_space_bytes,
                    free_space_gb,
                    total_torrents: total_count,
                    active_torrents: active_count,
                    paused_torrents: paused_count,
                    error_torrents: error_count,
                    rate_upload: rate_up,
                    rate_download,
                    ratio_0,
                    ratio_lt1,
                    ratio_gt1,
                    ratio_gt2,
                    alt_speed_enabled: false,
                    blocklist_size: 0,
                    blocklist_enabled: false,
                },
            },
        );
    }

    pub fn get_torrents(&self, node_filter: Option<&str>, search: Option<&str>, status_filter: Option<&str>) -> Vec<UnifiedTorrent> {
        let cache = self.cache.read();
        let mut results = Vec::new();

        let mut node_names: Vec<String> = cache.keys().cloned().collect();
        node_names.sort();

        for node_name in node_names {
            if let Some(target_node) = node_filter {
                if target_node != "all" && target_node != node_name {
                    continue;
                }
            }

            if let Some(data) = cache.get(&node_name) {
                for t in &data.torrents {
                    if let Some(search_term) = search {
                        let st = search_term.to_lowercase();
                        let name_match = t.name.to_lowercase().contains(&st);
                        let tracker_match = t.tracker_stats.iter().any(|ts| ts.host.to_lowercase().contains(&st));
                        let hash_match = t.hash_string.to_lowercase().contains(&st);
                        if !name_match && !tracker_match && !hash_match {
                            continue;
                        }
                    }

                    if let Some(status_str) = status_filter {
                        let matches_status = match status_str.to_lowercase().as_str() {
                            "downloading" => t.status == TorrentStatus::Downloading,
                            "seeding" => t.status == TorrentStatus::Seeding,
                            "stopped" | "paused" => t.status == TorrentStatus::Stopped,
                            "error" => t.status == TorrentStatus::Error,
                            "checking" => t.status == TorrentStatus::Checking || t.status == TorrentStatus::CheckWait,
                            "idle" => t.status == TorrentStatus::Idle,
                            "active" => t.rate_download > 0 || t.rate_upload > 0,
                            _ => true,
                        };
                        if !matches_status {
                            continue;
                        }
                    }

                    results.push(t.clone());
                }
            }
        }

        // Deterministic default ordering:
        // 1. added_date descending (newest additions first)
        // 2. Tie-breaker: name (case-insensitive)
        // 3. Tie-breaker: compound_id
        results.sort_by(|a, b| {
            b.added_date.cmp(&a.added_date)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.compound_id.cmp(&b.compound_id))
        });

        // Populate active circuit breaker metadata
        let breakers = self.active_breakers.read().clone();
        if !breakers.is_empty() {
            for t in &mut results {
                for ts in &t.tracker_stats {
                    let host = ts.host.trim();
                    if let Some(breaker) = breakers.get(host) {
                        if t.compound_id == breaker.canary_compound_id {
                            t.is_canary_probe = true;
                            t.circuit_breaker_reason = Some(format!(
                                "Active Canary Probe: Monitoring '{}' ({})",
                                breaker.tracker_host, breaker.failing_error
                            ));
                        } else if breaker.paused_torrents.contains(&t.compound_id) || t.raw_status == 0 {
                            t.is_circuit_broken = true;
                            t.circuit_breaker_reason = Some(format!(
                                "Swarm Pressure Relieved: Tracker '{}' unreachable ({})",
                                breaker.tracker_host, breaker.failing_error
                            ));
                        }
                        break;
                    }
                }
            }
        }

        results
    }

    pub fn get_torrent_by_compound_id(&self, compound_id: &str) -> Option<UnifiedTorrent> {
        let (node, id_str) = compound_id.split_once(':')?;
        let id: i64 = id_str.parse().ok()?;
        let cache = self.cache.read();
        let node_data = cache.get(node)?;
        let mut t = node_data.torrents.iter().find(|t| t.id == id).cloned()?;

        let breakers = self.active_breakers.read();
        for ts in &t.tracker_stats {
            let host = ts.host.trim();
            if let Some(breaker) = breakers.get(host) {
                if t.compound_id == breaker.canary_compound_id {
                    t.is_canary_probe = true;
                    t.circuit_breaker_reason = Some(format!(
                        "Active Canary Probe: Monitoring '{}' ({})",
                        breaker.tracker_host, breaker.failing_error
                    ));
                } else if breaker.paused_torrents.contains(&t.compound_id) || t.raw_status == 0 {
                    t.is_circuit_broken = true;
                    t.circuit_breaker_reason = Some(format!(
                        "Swarm Pressure Relieved: Tracker '{}' unreachable ({})",
                        breaker.tracker_host, breaker.failing_error
                    ));
                }
                break;
            }
        }

        Some(t)
    }

    /// Takes a 1-second sample of aggregated cluster bandwidth and keeps a rolling 5-minute (300-point) history,
    /// tracking top active swarm contributors for each second so users can inspect bandwidth spikes on hover.
    pub fn record_bandwidth_snapshot(&self) {
        let (down, up, top_torrents) = {
            let cache = self.cache.read();
            let mut total_down = 0i64;
            let mut total_up = 0i64;
            let mut active_torrents = Vec::new();

            for data in cache.values() {
                if data.connected {
                    total_down += data.stats.rate_download;
                    total_up += data.stats.rate_upload;
                    for t in &data.torrents {
                        if t.rate_download > 0 || t.rate_upload > 0 {
                            active_torrents.push(TorrentBandwidthContributor {
                                id: t.compound_id.clone(),
                                name: t.name.clone(),
                                node: t.node.clone(),
                                download_speed: t.rate_download,
                                upload_speed: t.rate_upload,
                            });
                        }
                    }
                }
            }

            // Sort active contributors by combined speed descending
            active_torrents.sort_by(|a, b| {
                let total_b = b.download_speed + b.upload_speed;
                let total_a = a.download_speed + a.upload_speed;
                total_b.cmp(&total_a)
            });

            // Keep top 10 contributors to keep snapshot payload compact
            active_torrents.truncate(10);

            (total_down, total_up, active_torrents)
        };

        let now = chrono::Utc::now().timestamp_millis();
        let point = BandwidthPoint {
            time: now,
            download_speed: down,
            upload_speed: up,
            top_torrents,
        };

        let mut hist = self.bandwidth_history.write();
        hist.push_back(point);
        while hist.len() > 300 {
            hist.pop_front();
        }
    }

    /// Averages the most recent `last_n` 1-second samples from the rolling 5-minute buffer —
    /// used once a minute to persist a smoothed point into the downsampled 72h SQLite history
    /// (see `Database::insert_bandwidth_point`), rather than persisting a single noisy instant.
    pub fn get_bandwidth_average(&self, last_n: usize) -> (i64, i64) {
        let hist = self.bandwidth_history.read();
        let count = hist.len().min(last_n);
        if count == 0 {
            return (0, 0);
        }
        let (sum_down, sum_up) = hist
            .iter()
            .rev()
            .take(count)
            .fold((0i64, 0i64), |(d, u), p| (d + p.download_speed, u + p.upload_speed));
        (sum_down / count as i64, sum_up / count as i64)
    }

    pub fn get_aggregate_stats(&self) -> AggregateStats {
        let cache = self.cache.read();
        let mut total_torrents = 0;
        let mut total_down = 0i64;
        let mut total_up = 0i64;
        let mut total_size = 0i64;
        let mut connected_count = 0;
        let mut node_stats_vec = Vec::new();

        for data in cache.values() {
            if data.connected {
                connected_count += 1;
            }
            total_torrents += data.stats.total_torrents;
            total_down += data.stats.rate_download;
            total_up += data.stats.rate_upload;
            for t in &data.torrents {
                total_size += t.total_size;
            }
            node_stats_vec.push(data.stats.clone());
        }

        let history: Vec<BandwidthPoint> = self.bandwidth_history.read().iter().cloned().collect();

        AggregateStats {
            total_nodes: cache.len(),
            connected_nodes: connected_count,
            total_torrents,
            total_download_speed: total_down,
            total_upload_speed: total_up,
            total_size_bytes: total_size,
            nodes: node_stats_vec,
            bandwidth_history: history,
        }
    }

    /// Executes a bulk action and reports one result per input compound ID (not per node) —
    /// torrents sharing a node share one underlying RPC call and so share that call's outcome,
    /// but every ID the caller sent gets an explicit entry, including malformed ones that used
    /// to be silently dropped.
    pub async fn execute_bulk_action(&self, payload: &BulkTorrentActionPayload) -> anyhow::Result<HashMap<String, BulkItemResult>> {
        // Group by node, keeping the original (as-sent) compound_id alongside the parsed torrent id.
        let mut grouped: HashMap<String, Vec<(String, i64)>> = HashMap::new();
        let mut results: HashMap<String, BulkItemResult> = HashMap::new();

        for cid in &payload.compound_ids {
            let parsed = cid
                .split_once(':')
                .and_then(|(node, id_str)| id_str.parse::<i64>().ok().map(|id| (node.to_string(), id)));

            match parsed {
                Some((node, id)) => grouped.entry(node).or_default().push((cid.clone(), id)),
                None => {
                    results.insert(cid.clone(), BulkItemResult {
                        status: "error".to_string(),
                        message: "Invalid compound ID — expected \"node:id\"".to_string(),
                    });
                }
            }
        }

        for (node, items) in grouped {
            let ids: Vec<i64> = items.iter().map(|(_, id)| *id).collect();

            let (status, message) = if let Some(client) = self.get_client(&node) {
                let res = match payload.action {
                    BulkActionType::Start => client.start_torrents(&ids, false).await,
                    BulkActionType::StartNow => client.start_torrents(&ids, true).await,
                    BulkActionType::Stop => client.stop_torrents(&ids).await,
                    BulkActionType::Verify => client.verify_torrents(&ids).await,
                    BulkActionType::Reannounce => client.reannounce_torrents(&ids).await,
                    BulkActionType::Delete => client.remove_torrents(&ids, payload.delete_local_data).await,
                    BulkActionType::SetLocation => {
                        if let Some(ref loc) = payload.target_directory {
                            client.set_location(&ids, loc, true).await
                        } else {
                            Err(anyhow::anyhow!("Missing target directory for SetLocation"))
                        }
                    }
                };

                match res {
                    Ok(_) => ("success".to_string(), format!("{} torrent(s) affected on {}", ids.len(), node)),
                    Err(e) => ("error".to_string(), e.to_string()),
                }
            } else {
                ("error".to_string(), "Node client not found or offline".to_string())
            };

            for (orig_cid, _id) in items {
                results.insert(orig_cid, BulkItemResult { status: status.clone(), message: message.clone() });
            }
        }

        Ok(results)
    }

    pub fn get_active_breakers(&self) -> HashMap<String, ActiveCircuitBreaker> {
        self.active_breakers.read().clone()
    }

    pub fn set_active_breaker(&self, host: String, breaker: ActiveCircuitBreaker) {
        self.active_breakers.write().insert(host, breaker);
    }

    pub fn remove_active_breaker(&self, host: &str) {
        self.active_breakers.write().remove(host);
    }

    pub fn get_system_health(&self) -> SystemHealthOverview {
        let cache = self.cache.read();
        let breakers = self.active_breakers.read().clone();

        let mut node_health = Vec::new();
        let mut all_torrents: Vec<UnifiedTorrent> = Vec::new();

        let mut total_torrents = 0;
        let mut connected_nodes = 0;
        let total_nodes = cache.len();

        for (name, data) in cache.iter() {
            if data.connected {
                connected_nodes += 1;
            }
            total_torrents += data.torrents.len();
            all_torrents.extend(data.torrents.clone());

            let status = if !data.connected {
                "unreachable".to_string()
            } else if data.stats.error_torrents > 0 || data.latency_ms > 2000 {
                "degraded".to_string()
            } else {
                "healthy".to_string()
            };

            node_health.push(NodeHealthStatus {
                name: name.clone(),
                connected: data.connected,
                latency_ms: data.latency_ms,
                status,
                last_error: if data.connected {
                    None
                } else {
                    Some("RPC connection failed or unreachable".to_string())
                },
                total_torrents: data.stats.total_torrents,
                active_torrents: data.stats.active_torrents,
                paused_torrents: data.stats.paused_torrents,
                error_torrents: data.stats.error_torrents,
                free_space_gb: data.stats.free_space_gb,
            });
        }

        // Sort nodes alphabetically
        node_health.sort_by(|a, b| a.name.cmp(&b.name));

        // Group torrents by tracker host
        let mut tracker_map: HashMap<String, Vec<UnifiedTorrent>> = HashMap::new();
        for t in &all_torrents {
            for ts in &t.tracker_stats {
                let host = ts.host.trim().to_string();
                if !host.is_empty() && host != "DHT" && host != "PEX" && host != "LPD" {
                    tracker_map.entry(host).or_default().push(t.clone());
                }
            }
        }

        let mut tracker_health = Vec::new();
        let mut circuit_broken_count = 0;

        for (host, t_list) in &tracker_map {
        let mut affected_nodes: HashSet<String> = HashSet::new();
            for t in t_list {
                affected_nodes.insert(t.node.clone());
            }
            let mut nodes_vec: Vec<String> = affected_nodes.into_iter().collect();
            nodes_vec.sort();

            // Passive mode: every node touching this tracker host manages its own native
            // breaker, so Conduit mirrors that status instead of computing its own. A host
            // shared with at least one active-mode node stays under Conduit's management
            // below (so that node's torrents don't go unmanaged).
            let all_passive = !nodes_vec.is_empty() && nodes_vec.iter().all(|n| self.is_passive_mode(n));
            let native_status = if all_passive {
                nodes_vec.iter().find_map(|n| {
                    self.native_breakers
                        .read()
                        .get(n)
                        .and_then(|list| list.iter().find(|s| s.host == *host).cloned())
                })
            } else {
                None
            };

            if let Some(native) = native_status {
                let is_broken = native.state != "healthy";
                if is_broken {
                    circuit_broken_count += 1;
                }
                // Map the native 4-state vocabulary onto the existing status/health_tier
                // vocabulary the dashboard already understands; the precise native state
                // lives in the new `cb_state` field alongside it.
                let mapped_status = match native.state.as_str() {
                    "healthy" => "healthy",
                    "recovering" => "warning",
                    _ => "circuit_broken", // tripped, half_open_canary
                };
                tracker_health.push(TrackerHealthStatus {
                    host: host.clone(),
                    status: mapped_status.to_string(),
                    total_torrents: t_list.len(),
                    online_torrents: t_list.len(),
                    error_torrents: if is_broken { t_list.len() } else { 0 },
                    error_ratio: if is_broken { 1.0 } else { 0.0 },
                    health_tier: mapped_status.to_string(),
                    paused_torrents: 0,
                    active_probe_id: None,
                    active_probe_name: None,
                    last_announce_result: format!("Managed natively by {}", nodes_vec.join(", ")),
                    last_announce_succeeded: !is_broken,
                    affected_nodes: nodes_vec,
                    is_circuit_broken: is_broken,
                    breaker_mode: "passive".to_string(),
                    cb_state: Some(native.state),
                    recovery_progress_pct: native.recovery_progress_pct,
                    consecutive_successes: Some(native.consecutive_successes),
                });
            } else if all_passive {
                // Passive-mode node(s) that haven't reported a status for this host yet
                // (not currently tracked by its breaker, i.e. implicitly healthy).
                tracker_health.push(TrackerHealthStatus {
                    host: host.clone(),
                    status: "healthy".to_string(),
                    total_torrents: t_list.len(),
                    online_torrents: t_list.len(),
                    error_torrents: 0,
                    error_ratio: 0.0,
                    health_tier: "healthy".to_string(),
                    paused_torrents: 0,
                    active_probe_id: None,
                    active_probe_name: None,
                    last_announce_result: format!("Managed natively by {}", nodes_vec.join(", ")),
                    last_announce_succeeded: true,
                    affected_nodes: nodes_vec,
                    is_circuit_broken: false,
                    breaker_mode: "passive".to_string(),
                    cb_state: Some("healthy".to_string()),
                    recovery_progress_pct: None,
                    consecutive_successes: None,
                });
            } else if let Some(breaker) = breakers.get(host) {
                circuit_broken_count += 1;

                // Find active canary probe announce result
                let mut announce_res = breaker.failing_error.clone();
                let mut announce_succ = false;

                if let Some(probe) = t_list.iter().find(|t| t.compound_id == breaker.canary_compound_id) {
                    for ts in &probe.tracker_stats {
                        if ts.host == *host {
                            if !ts.last_announce_result.is_empty() {
                                announce_res = ts.last_announce_result.clone();
                            }
                            announce_succ = ts.last_announce_succeeded;
                            break;
                        }
                    }
                }

                tracker_health.push(TrackerHealthStatus {
                    host: host.clone(),
                    status: "circuit_broken".to_string(),
                    total_torrents: t_list.len(),
                    online_torrents: 1,
                    error_torrents: breaker.paused_torrents.len(),
                    error_ratio: 1.0,
                    health_tier: "circuit_broken".to_string(),
                    paused_torrents: breaker.paused_torrents.len(),
                    active_probe_id: Some(breaker.canary_compound_id.clone()),
                    active_probe_name: Some(breaker.canary_name.clone()),
                    last_announce_result: announce_res,
                    last_announce_succeeded: announce_succ,
                    affected_nodes: nodes_vec,
                    is_circuit_broken: true,
                    breaker_mode: "active".to_string(),
                    cb_state: Some(if breaker.state.is_empty() { "tripped".to_string() } else { breaker.state.clone() }),
                    recovery_progress_pct: None,
                    consecutive_successes: Some(breaker.consecutive_successes),
                });
            } else {
                let total = t_list.len();
                let mut online_count = 0usize;
                let mut error_count = 0usize;
                let mut failing_error: Option<String> = None;

                for t in t_list {
                    let mut matched = false;
                    for ts in &t.tracker_stats {
                        if ts.host == *host {
                            matched = true;
                            let res_lower = ts.last_announce_result.to_lowercase();
                            if ts.last_announce_succeeded
                                || res_lower.contains("success")
                                || res_lower == "ok"
                                || res_lower.starts_with("ok ")
                                || res_lower.ends_with(" ok")
                            {
                                online_count += 1;
                            } else if !ts.last_announce_result.trim().is_empty() {
                                error_count += 1;
                                if failing_error.is_none() {
                                    failing_error = Some(ts.last_announce_result.clone());
                                }
                            } else {
                                // Default unannounced / idle
                                online_count += 1;
                            }
                            break;
                        }
                    }
                    if !matched {
                        online_count += 1;
                    }
                }

                let error_ratio = if total > 0 { error_count as f64 / total as f64 } else { 0.0 };
                let (status, health_tier, res_str, succ) = if error_count == 0 {
                    ("healthy".to_string(), "healthy".to_string(), "Success / OK".to_string(), true)
                } else if error_ratio < 0.15 {
                    let err = failing_error.unwrap_or_else(|| "Minor announce errors".to_string());
                    ("nominal".to_string(), "nominal".to_string(), err, false)
                } else if error_ratio < 0.50 {
                    let err = failing_error.unwrap_or_else(|| "Degraded announce responses".to_string());
                    ("warning".to_string(), "warning".to_string(), err, false)
                } else {
                    let err = failing_error.unwrap_or_else(|| "Tracker error".to_string());
                    ("critical".to_string(), "critical".to_string(), err, false)
                };

                tracker_health.push(TrackerHealthStatus {
                    host: host.clone(),
                    status,
                    total_torrents: total,
                    online_torrents: online_count,
                    error_torrents: error_count,
                    error_ratio,
                    health_tier,
                    paused_torrents: 0,
                    active_probe_id: None,
                    active_probe_name: None,
                    last_announce_result: res_str,
                    last_announce_succeeded: succ,
                    affected_nodes: nodes_vec,
                    is_circuit_broken: false,
                    breaker_mode: "active".to_string(),
                    cb_state: None,
                    recovery_progress_pct: None,
                    consecutive_successes: None,
                });
            }
        }

        // Sort trackers: circuit broken first, critical, warning, nominal, healthy
        tracker_health.sort_by(|a, b| {
            let score = |s: &str| match s {
                "circuit_broken" => 0,
                "critical" => 1,
                "warning" => 2,
                "nominal" => 3,
                "healthy" => 4,
                _ => 5,
            };
            score(&a.health_tier)
                .cmp(&score(&b.health_tier))
                .then(b.error_torrents.cmp(&a.error_torrents))
                .then(a.host.cmp(&b.host))
        });

        let overall_status = if connected_nodes < total_nodes {
            "degraded".to_string()
        } else if circuit_broken_count > 0 || tracker_health.iter().any(|t| t.status == "warning") {
            "warning".to_string()
        } else {
            "healthy".to_string()
        };

        let total_trackers = tracker_health.len();

        SystemHealthOverview {
            overall_status,
            total_nodes,
            connected_nodes,
            total_torrents,
            total_trackers,
            circuit_broken_trackers: circuit_broken_count,
            nodes: node_health,
            trackers: tracker_health,
        }
    }
}
