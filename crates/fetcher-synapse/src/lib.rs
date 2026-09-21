pub mod alerts;
pub mod client;
pub mod error;
pub mod live_cache;
pub mod proto;

pub use client::SynapseClient as SdkClient;
pub use error::{Result as SdkResult, SynapseClientError};
pub use live_cache::SynapseLiveCache;

use async_trait::async_trait;
use base64::Engine;
use fetcher_core::{
    FetcherDriverFactory, FetcherNodeConfig, NativeCircuitBreakerStatus, NodeCapabilities,
    RetrieverClientType, Torrent, TorrentClientTrait, TorrentFile, TorrentPeer, TrackerStat,
};
use proto::v2::{
    TorrentDetailEvent, TorrentState, TorrentSummary, UpdateSessionSettingsRequest,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

const RPC_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct SynapseClient {
    config: FetcherNodeConfig,
    client: SdkClient,
    cache: SynapseLiveCache,
    alerts: alerts::AlertFeed,
}

impl SynapseClient {
    pub fn new(config: FetcherNodeConfig) -> Self {
        if config.use_ssl {
            tracing::warn!(
                "Synapse node '{}' has use_ssl set, but synapse.v2's gRPC server has no TLS \
                 support yet — connecting over plaintext HTTP/2 anyway.",
                config.name
            );
        }

        let uri = format!("http://{}:{}", config.host, config.port);
        let mut client = SdkClient::connect_lazy(&uri).unwrap_or_else(|e| {
            tracing::error!(
                "Synapse node '{}' has an invalid host/port ({}: {}); this node will fail to \
                 connect until its config is fixed: {e}",
                config.name,
                uri,
                e
            );
            SdkClient::connect_lazy("http://127.0.0.1:1").expect("fallback endpoint is valid")
        });

        if let Some(token) = config.password.as_deref().filter(|p| !p.is_empty()) {
            client = client.with_auth_token(token);
        }

        let cache = SynapseLiveCache::spawn(client.clone());
        let alerts = alerts::AlertFeed::spawn(client.clone(), config.name.clone());

        Self {
            config,
            client,
            cache,
            alerts,
        }
    }

    /// Derives a stable, deterministic i64 id from a torrent's hex hash.
    /// 15 hex chars = 60 bits, comfortably inside i64's positive range.
    fn hash_to_id(hash: &str) -> i64 {
        let prefix: String = hash.chars().take(15).collect();
        i64::from_str_radix(&prefix, 16).unwrap_or(0)
    }

    /// Instant, non-network read of the live cache — errors if the background stream isn't
    /// currently connected.
    fn snapshot(&self) -> anyhow::Result<Vec<TorrentSummary>> {
        if !self.cache.is_connected() {
            return Err(anyhow::anyhow!(
                "Synapse node '{}' is not connected (SubscribeTorrents stream down)",
                self.config.name
            ));
        }
        Ok(self.cache.list_torrents())
    }

    fn summary_to_torrent(&self, s: TorrentSummary) -> Torrent {
        let id = Self::hash_to_id(&s.hash);
        let state = TorrentState::try_from(s.state).unwrap_or(TorrentState::StateStopped);

        let (status, error, error_string) = match state {
            TorrentState::StateStopped => (0, 0, String::new()),
            TorrentState::StateChecking => (2, 0, String::new()),
            TorrentState::StateQueued => (3, 0, String::new()),
            TorrentState::StateDownloading => (4, 0, String::new()),
            TorrentState::StateSeeding => (6, 0, String::new()),
            TorrentState::StateError => (0, 3, s.error_message.clone().unwrap_or_default()),
        };

        let download_dir = if s.download_dir.is_empty() {
            self.config.host.clone()
        } else {
            s.download_dir
        };
        let progress = s.progress as f64;

        Torrent {
            id,
            name: s.name,
            hash_string: s.hash,
            status,
            rate_upload: s.rate_upload as i64,
            rate_download: s.rate_download as i64,
            uploaded_ever: 0,
            downloaded_ever: (s.total_size as f64 * progress) as i64,
            upload_ratio: s.ratio as f64,
            total_size: s.total_size as i64,
            size_when_done: s.total_size as i64,
            left_until_done: (s.total_size as f64 * (1.0 - progress)) as i64,
            percent_done: progress,
            eta: s.eta_seconds as i64,
            eta_idle: None,
            error,
            error_string,
            peers_connected: s.peers_connected as i64,
            peers_sending_to_us: s.peers_sending as i64,
            peers_getting_from_us: 0,
            added_date: s.added_at,
            done_date: 0,
            download_dir,
            tracker_stats: Vec::new(),
            files: None,
            peers: None,
            comment: None,
            creator: None,
            date_created: None,
            piece_count: if s.piece_count > 0 { Some(s.piece_count as i64) } else { None },
            piece_size: if s.piece_size > 0 { Some(s.piece_size as i64) } else { None },
            is_private: None,
            magnet_link: None,
            corrupt_ever: None,
            seconds_downloading: None,
            seconds_seeding: None,
            activity_date: None,
            queue_position: s.queue_position as i64,
            sequential_download: false,
            pieces: None,
            availability: None,
        }
    }

    /// Resolves a requested batch of `i64` ids back to hex info-hashes.
    fn resolve_hashes(&self, ids: &[i64]) -> anyhow::Result<Vec<String>> {
        let summaries = self.snapshot()?;
        Ok(summaries
            .into_iter()
            .filter(|s| ids.contains(&Self::hash_to_id(&s.hash)))
            .map(|s| s.hash)
            .collect())
    }
}

#[async_trait]
impl TorrentClientTrait for SynapseClient {
    fn node_name(&self) -> &str {
        &self.config.name
    }

    fn config(&self) -> &FetcherNodeConfig {
        &self.config
    }

    fn client_type(&self) -> RetrieverClientType {
        RetrieverClientType::Synapse
    }

    fn subscribe_alerts(&self) -> Option<tokio::sync::broadcast::Receiver<fetcher_core::NodeAlert>> {
        Some(self.alerts.sender.subscribe())
    }

    async fn get_capabilities(&self) -> anyhow::Result<NodeCapabilities> {
        match self.client.get_capabilities().await {
            Ok(resp) => Ok(capabilities_from_features(&resp.features)),
            // An older Synapse build that predates this RPC entirely — that's not a
            // connection failure, it just has no optional features.
            Err(SynapseClientError::Rpc { code: tonic::Code::Unimplemented, .. }) => {
                Ok(NodeCapabilities::baseline(RetrieverClientType::Synapse))
            }
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    }

    async fn list_circuit_breakers(&self) -> anyhow::Result<Vec<NativeCircuitBreakerStatus>> {
        let breakers = self.client.list_circuit_breakers().await.map_err(|e| anyhow::anyhow!(e))?;
        Ok(breakers
            .into_iter()
            .map(|b| NativeCircuitBreakerStatus {
                host: b.host,
                state: circuit_breaker_state_label(b.state),
                consecutive_successes: b.consecutive_successes,
                consecutive_failures: b.consecutive_failures,
                backoff_remaining_ms: b.backoff_remaining_ms,
                recovery_progress_pct: b.recovery_progress_pct,
            })
            .collect())
    }

    async fn force_trip_circuit_breaker(&self, host: &str) -> anyhow::Result<()> {
        self.client.force_trip_circuit_breaker(host).await.map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }

    async fn force_reset_circuit_breaker(&self, host: &str) -> anyhow::Result<()> {
        self.client.force_reset_circuit_breaker(host).await.map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }

    async fn get_torrents(&self, _ids: Option<Vec<i64>>) -> anyhow::Result<Vec<Torrent>> {
        let summaries = self.snapshot()?;
        Ok(summaries.into_iter().map(|s| self.summary_to_torrent(s)).collect())
    }

    async fn get_torrent_details(&self, id: i64) -> anyhow::Result<Torrent> {
        let summaries = self.snapshot()?;
        let hash = summaries
            .into_iter()
            .find(|s| Self::hash_to_id(&s.hash) == id)
            .map(|s| s.hash)
            .ok_or_else(|| anyhow::anyhow!("Torrent id {} not found on node {}", id, self.config.name))?;
        self.get_torrent_details_by_hash(&hash).await
    }

    async fn get_torrent_details_by_hash(&self, hash: &str) -> anyhow::Result<Torrent> {
        let summary = self
            .cache
            .get_torrent(hash)
            .ok_or_else(|| anyhow::anyhow!("Torrent {} not found on node {}", hash, self.config.name))?;
        let mut torrent = self.summary_to_torrent(summary);

        let stream_res = tokio::time::timeout(
            RPC_TIMEOUT,
            self.client.subscribe_torrent_detail(hash, 0),
        )
        .await;

        let mut stream = match stream_res {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                tracing::debug!("Synapse torrent detail RPC for {} failed: {}", hash, e);
                return Ok(torrent);
            }
            Err(_) => {
                tracing::debug!("Synapse torrent detail RPC for {} timed out", hash);
                return Ok(torrent);
            }
        };

        let event: TorrentDetailEvent = match stream.message().await {
            Ok(Some(ev)) => ev,
            Ok(None) => {
                tracing::debug!("Synapse detail stream for {} closed with no event", hash);
                return Ok(torrent);
            }
            Err(e) => {
                tracing::debug!("Synapse torrent detail stream error for {}: {}", hash, e);
                return Ok(torrent);
            }
        };

        torrent.peers = Some(
            event
                .active_peers
                .into_iter()
                .map(|p| {
                    let port = p
                        .address
                        .rsplit_once(':')
                        .and_then(|(_, port)| port.parse().ok())
                        .unwrap_or(0);
                    TorrentPeer {
                        address: p.address,
                        client_name: p.client_name,
                        client_is_choked: false,
                        client_is_interested: false,
                        flagstr: p.flags,
                        is_downloading_from: p.rate_to_client > 0,
                        is_encrypted: p.is_encrypted,
                        is_incoming: false,
                        is_uploading_to: p.rate_to_peer > 0,
                        is_utp: p.is_utp,
                        peer_is_choked: false,
                        peer_is_interested: false,
                        port,
                        progress: p.progress as f64,
                        rate_to_client: p.rate_to_client as i64,
                        rate_to_peer: p.rate_to_peer as i64,
                        country_code: p.country_code,
                        as_name: p.as_name,
                    }
                })
                .collect(),
        );

        torrent.tracker_stats = event
            .trackers
            .into_iter()
            .map(|t| {
                let host = t
                    .url
                    .split("://")
                    .nth(1)
                    .and_then(|s| s.split('/').next())
                    .unwrap_or(&t.url)
                    .to_string();
                TrackerStat {
                    announce: t.url,
                    host,
                    seeder_count: t.seeders as i64,
                    leecher_count: t.leechers as i64,
                    download_count: 0,
                    last_announce_succeeded: t.failure_reason.is_none(),
                    last_announce_result: t.failure_reason.unwrap_or(t.status),
                    is_backup: false,
                }
            })
            .collect();

        torrent.files = Some(
            event
                .files
                .into_iter()
                .map(|f| TorrentFile {
                    name: f.path,
                    bytes_completed: f.bytes_completed as i64,
                    length: f.size_bytes as i64,
                })
                .collect(),
        );

        if event.piece_count > 0 {
            torrent.piece_count = Some(event.piece_count as i64);
        }
        if event.piece_size > 0 {
            torrent.piece_size = Some(event.piece_size as i64);
        }
        if !event.availability.is_empty() {
            torrent.availability = Some(event.availability.into_iter().map(|a| a as i64).collect());
        }
        if !event.piece_bitfield.is_empty() {
            torrent.pieces =
                Some(base64::engine::general_purpose::STANDARD.encode(&event.piece_bitfield));
        }

        Ok(torrent)
    }

    async fn add_torrent(
        &self,
        magnet_or_url: Option<&str>,
        metainfo_b64: Option<&str>,
        download_dir: Option<&str>,
        paused: bool,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = if let Some(b64) = metainfo_b64 {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| anyhow::anyhow!("Failed to decode base64 metainfo: {}", e))?;
            self.client
                .add_torrent_bytes(bytes, download_dir.map(String::from), paused)
                .await?
        } else if let Some(m) = magnet_or_url.filter(|m| m.starts_with("magnet:")) {
            self.client
                .add_magnet(m, download_dir.map(String::from), paused)
                .await?
        } else if let Some(url) = magnet_or_url {
            self.client
                .add_torrent_url(url, download_dir.map(String::from), paused)
                .await?
        } else {
            return Err(anyhow::anyhow!(
                "add_torrent requires either a magnet URI, a torrent file URL, or base64 metainfo"
            ));
        };

        if !resp.success {
            return Err(anyhow::anyhow!(resp
                .error
                .unwrap_or_else(|| "Synapse rejected the torrent with no error message".to_string())));
        }

        Ok(json!({
            "torrent-added": {
                "id": Self::hash_to_id(&resp.hash),
                "name": resp.name,
                "hashString": resp.hash,
                "downloadDir": download_dir.unwrap_or_default(),
            }
        }))
    }

    async fn start_torrents(&self, ids: &[i64], _now: bool) -> anyhow::Result<()> {
        let hashes = self.resolve_hashes(ids)?;
        for hash in &hashes {
            let resp = self.client.resume_torrent(hash).await?;
            if !resp.success {
                anyhow::bail!("ResumeTorrent failed for {hash}: {}", resp.error.unwrap_or_default());
            }
        }
        Ok(())
    }

    async fn stop_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let hashes = self.resolve_hashes(ids)?;
        for hash in &hashes {
            let resp = self.client.pause_torrent(hash).await?;
            if !resp.success {
                anyhow::bail!("PauseTorrent failed for {hash}: {}", resp.error.unwrap_or_default());
            }
        }
        Ok(())
    }

    async fn verify_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let hashes = self.resolve_hashes(ids)?;
        for hash in &hashes {
            let resp = self.client.recheck_torrent(hash).await?;
            if !resp.success {
                anyhow::bail!("RecheckTorrent failed for {hash}: {}", resp.error.unwrap_or_default());
            }
        }
        Ok(())
    }

    async fn reannounce_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let hashes = self.resolve_hashes(ids)?;
        let resp = self
            .client
            .reannounce_torrents(&hashes)
            .await
            .map_err(|e| rpc_error(e, "re-announcing"))?;
        command_ok(resp, "ReannounceTorrents")
    }

    async fn remove_torrents(&self, ids: &[i64], delete_local_data: bool) -> anyhow::Result<()> {
        let hashes = self.resolve_hashes(ids)?;
        for hash in &hashes {
            let resp = self.client.remove_torrent(hash, delete_local_data).await?;
            if !resp.success {
                anyhow::bail!("RemoveTorrent failed for {hash}: {}", resp.error.unwrap_or_default());
            }
        }
        Ok(())
    }

    async fn set_location(&self, ids: &[i64], location: &str, move_data: bool) -> anyhow::Result<()> {
        let hashes = self.resolve_hashes(ids)?;
        for hash in &hashes {
            let resp = self.client.set_location(hash, location, move_data).await?;
            if !resp.success {
                anyhow::bail!("SetLocation failed for {hash}: {}", resp.error.unwrap_or_default());
            }
        }
        Ok(())
    }

    // Synapse's gRPC surface (its canonical `synapse-proto/proto/synapse.proto`)
    // has no RPC for queue position, sequential download, path renaming, or tracker
    // replacement -- there is nothing to call for any of these four, on any Synapse
    // daemon version. Returning a clear "unsupported" error is honest about that; the
    // previous `Ok(())`/fabricated-success-JSON behavior looked identical, from the
    // caller's side, to the operation having actually happened.
    async fn queue_move(&self, ids: &[i64], direction: &str) -> anyhow::Result<()> {
        let hashes = self.resolve_hashes(ids)?;
        let resp = self
            .client
            .move_in_queue(&hashes, direction)
            .await
            .map_err(|e| rpc_error(e, "queue reordering"))?;
        command_ok(resp, "MoveInQueue")
    }

    async fn set_sequential_download(&self, ids: &[i64], enabled: bool) -> anyhow::Result<()> {
        let hashes = self.resolve_hashes(ids)?;
        let resp = self
            .client
            .set_sequential_download(&hashes, enabled)
            .await
            .map_err(|e| rpc_error(e, "sequential download"))?;
        command_ok(resp, "SetSequentialDownload")
    }

    async fn rename_path(&self, _id: i64, _path: &str, _new_name: &str) -> anyhow::Result<serde_json::Value> {
        Err(fetcher_core::unsupported("Synapse cannot rename a torrent's files or folders"))
    }

    async fn set_turtle_mode(&self, enabled: bool) -> anyhow::Result<()> {
        self.client.set_turtle_mode(enabled).await?;
        Ok(())
    }

    /// Re-reads Synapse's IP filter (its configured ranges and blocklist file). Synapse does
    /// not download a list itself, so this picks up a file something else keeps fresh.
    async fn update_blocklist(&self) -> anyhow::Result<i64> {
        let rules = self
            .client
            .reload_ip_filter()
            .await
            .map_err(|e| rpc_error(e, "IP filter reload"))?;
        Ok(rules as i64)
    }

    async fn get_free_space(&self, _path: &str) -> anyhow::Result<i64> {
        let stats = self.client.get_session_stats().await?;
        Ok(stats.free_disk_space_bytes as i64)
    }

    async fn get_session(&self) -> anyhow::Result<serde_json::Value> {
        match self.client.get_session_settings().await {
            Ok(s) => Ok(session_json(&s)),
            Err(_) => {
                let stats = self.client.get_session_stats().await?;
                Ok(json!({
                    "version": "Synapse 2.0 (Rust)",
                    "free-space": stats.free_disk_space_bytes,
                    "torrent-count": stats.torrent_count,
                }))
            }
        }
    }

    async fn set_session(&self, settings: serde_json::Value) -> anyhow::Result<()> {
        let update = session_update(&settings);
        let resp = self.client.update_session_settings(update).await?;
        if !resp.success {
            anyhow::bail!(resp
                .error
                .unwrap_or_else(|| "Synapse rejected the settings".to_string()));
        }
        Ok(())
    }

    async fn get_session_stats(&self) -> anyhow::Result<serde_json::Value> {
        let stats = self.client.get_session_stats().await?;
        Ok(json!({
            "activeTorrentCount": stats.active_downloading + stats.active_seeding,
            "downloadSpeed": stats.rate_download,
            "uploadSpeed": stats.rate_upload,
            "pausedTorrentCount": stats.torrent_count.saturating_sub(stats.active_downloading + stats.active_seeding),
            "torrentCount": stats.torrent_count,
        }))
    }

    async fn test_port(&self) -> anyhow::Result<bool> {
        Err(fetcher_core::unsupported("Synapse cannot test whether its listen port is reachable"))
    }

    /// `tracker_list` is the caller's already-computed, post-replacement announce list, one
    /// URL per line (blank lines separate tiers, which Synapse does not distinguish).
    async fn replace_trackers(&self, id: i64, tracker_list: &str, _old_url: &str, _new_url: &str) -> anyhow::Result<()> {
        let hashes = self.resolve_hashes(&[id])?;
        let hash = hashes.first().ok_or_else(|| anyhow::anyhow!("Torrent not found on node {}", self.config.name))?;
        let trackers: Vec<String> = tracker_list
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        let resp = self
            .client
            .replace_trackers(hash, trackers)
            .await
            .map_err(|e| rpc_error(e, "tracker replacement"))?;
        command_ok(resp, "ReplaceTrackers")
    }
}

/// Maps the feature names a Synapse daemon lists to what Conduit can offer on that node.
fn capabilities_from_features(features: &[String]) -> NodeCapabilities {
    let has = |name: &str| features.iter().any(|f| f == name);
    NodeCapabilities {
        native_tracker_circuit_breaker: has("tracker_circuit_breaker_v1"),
        queue_move: has("queue_move_v1"),
        sequential_download: has("sequential_download_v1"),
        reannounce: has("reannounce_v1"),
        replace_trackers: has("replace_trackers_v1"),
        blocklist_update: has("ip_filter_reload_v1"),
        ..NodeCapabilities::baseline(RetrieverClientType::Synapse)
    }
}

/// Turns a failed call into the error the API should show: an older Synapse that predates the
/// RPC is "unsupported" (a 501 with a clear message), anything else is a real failure.
fn rpc_error(err: SynapseClientError, what: &str) -> anyhow::Error {
    match err {
        SynapseClientError::Rpc { code: tonic::Code::Unimplemented, .. } => fetcher_core::unsupported(format!(
            "This Synapse version does not support {what}; upgrade the daemon"
        )),
        other => anyhow::anyhow!(other),
    }
}

/// A `CommandResponse` that reports failure becomes an error carrying the daemon's reason.
fn command_ok(resp: proto::v2::CommandResponse, rpc: &str) -> anyhow::Result<()> {
    if resp.success {
        Ok(())
    } else {
        anyhow::bail!("{rpc} failed: {}", resp.error.unwrap_or_default())
    }
}

fn circuit_breaker_state_label(state: i32) -> String {
    use proto::v2::CircuitBreakerState;
    match CircuitBreakerState::try_from(state).unwrap_or(CircuitBreakerState::CbHealthy) {
        CircuitBreakerState::CbHealthy => "healthy",
        CircuitBreakerState::CbTripped => "tripped",
        CircuitBreakerState::CbHalfOpenCanary => "half_open_canary",
        CircuitBreakerState::CbRecovering => "recovering",
    }
    .to_string()
}

pub struct SynapseDriverFactory;

impl FetcherDriverFactory for SynapseDriverFactory {
    fn client_type(&self) -> RetrieverClientType {
        RetrieverClientType::Synapse
    }

    fn create_client(&self, config: FetcherNodeConfig) -> Arc<dyn TorrentClientTrait> {
        Arc::new(SynapseClient::new(config))
    }
}

/// Synapse's session settings in the Transmission-shaped keys Conduit's UI uses, plus
/// `synapse-*` keys for the settings only Synapse has.
fn session_json(s: &proto::v2::SessionSettingsResponse) -> serde_json::Value {
    json!({
        "version": "Synapse 2.0 (Rust)",
        "download-dir": s.download_dir,
        "alt-speed-enabled": s.alt_speed_enabled,
        "alt-speed-down": s.alt_speed_down_bytes / 1024,
        "alt-speed-up": s.alt_speed_up_bytes / 1024,
        "speed-limit-down": s.download_limit_bytes / 1024,
        "speed-limit-down-enabled": s.download_limit_enabled,
        "speed-limit-up": s.upload_limit_bytes / 1024,
        "speed-limit-up-enabled": s.upload_limit_enabled,
        "download-queue-size": s.download_queue_size,
        "seed-queue-size": s.seed_queue_size,
        "peer-limit-global": s.max_global_peers,
        "peer-limit-per-torrent": s.max_peers_per_torrent,
        "dht-enabled": s.dht_enabled,
        "pex-enabled": s.pex_enabled,
        "lpd-enabled": s.lsd_enabled,
        "utp-enabled": s.enable_utp,
        "synapse-encryption": s.encryption,
        "synapse-dht-read-only": s.dht_read_only,
        "synapse-zeroconf": s.zeroconf_enabled,
        "synapse-announce-ip": s.announce_ip,
    })
}

/// The inverse of [`session_json`]: only keys present (and of the right type) are changed.
fn session_update(settings: &serde_json::Value) -> UpdateSessionSettingsRequest {
    let flag = |k: &str| settings.get(k).and_then(|v| v.as_bool());
    let count = |k: &str| {
        settings
            .get(k)
            .and_then(|v| v.as_u64())
            .and_then(|n| u32::try_from(n).ok())
    };
    let kib = |k: &str| {
        settings
            .get(k)
            .and_then(|v| v.as_u64())
            .map(|n| n.saturating_mul(1024))
    };
    UpdateSessionSettingsRequest {
        alt_speed_enabled: flag("alt-speed-enabled"),
        alt_speed_down_bytes: kib("alt-speed-down"),
        alt_speed_up_bytes: kib("alt-speed-up"),
        download_limit_bytes: kib("speed-limit-down"),
        download_limit_enabled: flag("speed-limit-down-enabled"),
        upload_limit_bytes: kib("speed-limit-up"),
        upload_limit_enabled: flag("speed-limit-up-enabled"),
        max_global_peers: count("peer-limit-global"),
        max_peers_per_torrent: count("peer-limit-per-torrent"),
        dht_enabled: flag("dht-enabled"),
        pex_enabled: flag("pex-enabled"),
        lsd_enabled: flag("lpd-enabled"),
        enable_utp: flag("utp-enabled"),
        dht_read_only: flag("synapse-dht-read-only"),
        zeroconf_enabled: flag("synapse-zeroconf"),
        announce_ip: settings
            .get("synapse-announce-ip")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        download_dir: settings
            .get("download-dir")
            .and_then(|v| v.as_str())
            .filter(|d| !d.trim().is_empty())
            .map(str::to_string),
        ..Default::default()
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_the_transmission_shaped_keys() {
        let s = proto::v2::SessionSettingsResponse {
            download_dir: "/data".into(),
            download_limit_enabled: true,
            download_limit_bytes: 5 * 1024,
            max_global_peers: 400,
            max_peers_per_torrent: 60,
            dht_enabled: true,
            lsd_enabled: true,
            dht_read_only: true,
            zeroconf_enabled: true,
            announce_ip: "203.0.113.5".into(),
            ..Default::default()
        };
        let j = session_json(&s);
        assert_eq!(j["speed-limit-down"], 5);
        assert_eq!(j["peer-limit-global"], 400);
        assert_eq!(j["lpd-enabled"], true);
        assert_eq!(j["synapse-announce-ip"], "203.0.113.5");
        // The UI sends back what it read; the update carries the same values, in bytes.
        let u = session_update(&j);
        assert_eq!(u.download_limit_bytes, Some(5 * 1024));
        assert_eq!(u.max_global_peers, Some(400));
        assert_eq!(u.max_peers_per_torrent, Some(60));
        assert_eq!(u.lsd_enabled, Some(true));
        assert_eq!(u.dht_read_only, Some(true));
        assert_eq!(u.zeroconf_enabled, Some(true));
        assert_eq!(u.announce_ip.as_deref(), Some("203.0.113.5"));
        assert_eq!(u.download_dir.as_deref(), Some("/data"));
    }

    #[test]
    fn only_the_keys_present_change_and_bad_values_are_ignored() {
        let u = session_update(
            &json!({ "dht-enabled": false, "peer-limit-global": "many", "speed-limit-up": -3, "download-dir": "  " }),
        );
        assert_eq!(u.dht_enabled, Some(false));
        assert_eq!(u.max_global_peers, None);
        assert_eq!(u.upload_limit_bytes, None);
        assert_eq!(u.download_dir, None);
        assert_eq!(u.pex_enabled, None);
        // An empty announce_ip is meaningful: it clears the setting.
        assert_eq!(
            session_update(&json!({"synapse-announce-ip": ""}))
                .announce_ip
                .as_deref(),
            Some("")
        );
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    fn feats(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn an_older_daemon_advertises_only_what_it_lists() {
        let old = capabilities_from_features(&feats(&["tracker_circuit_breaker_v1"]));
        assert!(old.native_tracker_circuit_breaker);
        assert!(!old.queue_move && !old.sequential_download && !old.reannounce);
        assert!(!old.replace_trackers && !old.blocklist_update);
    }

    #[test]
    fn a_current_daemon_gets_every_operation_it_lists_and_never_rename_or_port_test() {
        let caps = capabilities_from_features(&feats(&[
            "tracker_circuit_breaker_v1",
            "queue_move_v1",
            "sequential_download_v1",
            "reannounce_v1",
            "replace_trackers_v1",
            "ip_filter_reload_v1",
        ]));
        assert!(caps.queue_move && caps.sequential_download && caps.reannounce);
        assert!(caps.replace_trackers && caps.blocklist_update && caps.turtle_mode);
        assert!(!caps.rename_path, "Synapse cannot rename files");
        assert!(!caps.test_port, "Synapse cannot test its own port");
    }

    #[test]
    fn a_daemon_that_predates_an_rpc_is_unsupported_not_broken() {
        let old = SynapseClientError::Rpc {
            code: tonic::Code::Unimplemented,
            message: "unknown service method".into(),
        };
        let err = rpc_error(old, "queue reordering");
        assert!(err.downcast_ref::<fetcher_core::Unsupported>().is_some());
        assert!(err.to_string().contains("queue reordering"));

        let down = SynapseClientError::Rpc { code: tonic::Code::Unavailable, message: "down".into() };
        assert!(rpc_error(down, "x").downcast_ref::<fetcher_core::Unsupported>().is_none());
    }

    #[test]
    fn a_refusal_from_the_daemon_carries_its_reason() {
        let refused = proto::v2::CommandResponse { success: false, error: Some("no such torrent".into()) };
        let err = command_ok(refused, "MoveInQueue").unwrap_err();
        assert!(err.to_string().contains("no such torrent"));
        assert!(command_ok(proto::v2::CommandResponse { success: true, error: None }, "x").is_ok());
    }
}
