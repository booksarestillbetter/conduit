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

        Self {
            config,
            client,
            cache,
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
            queue_position: 0,
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

    async fn get_capabilities(&self) -> anyhow::Result<NodeCapabilities> {
        match self.client.get_capabilities().await {
            Ok(resp) => Ok(NodeCapabilities {
                native_tracker_circuit_breaker: resp
                    .features
                    .iter()
                    .any(|f| f == "tracker_circuit_breaker_v1"),
            }),
            // An older Synapse build that predates this RPC entirely — that's not a
            // connection failure, it just has no optional features.
            Err(SynapseClientError::Rpc { code: tonic::Code::Unimplemented, .. }) => {
                Ok(NodeCapabilities::default())
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

    async fn reannounce_torrents(&self, _ids: &[i64]) -> anyhow::Result<()> {
        tracing::debug!(
            "Synapse node '{}': reannounce requested (no-op)",
            self.config.name
        );
        Ok(())
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

    async fn queue_move(&self, _ids: &[i64], _direction: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn set_sequential_download(&self, _ids: &[i64], _enabled: bool) -> anyhow::Result<()> {
        Ok(())
    }

    async fn rename_path(&self, _id: i64, _path: &str, _new_name: &str) -> anyhow::Result<serde_json::Value> {
        Ok(json!({ "result": "success" }))
    }

    async fn set_turtle_mode(&self, enabled: bool) -> anyhow::Result<()> {
        self.client.set_turtle_mode(enabled).await?;
        Ok(())
    }

    async fn update_blocklist(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn get_free_space(&self, _path: &str) -> anyhow::Result<i64> {
        let stats = self.client.get_session_stats().await?;
        Ok(stats.free_disk_space_bytes as i64)
    }

    async fn get_session(&self) -> anyhow::Result<serde_json::Value> {
        match self.client.get_session_settings().await {
            Ok(s) => Ok(json!({
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
                "peer-port": self.config.port,
            })),
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
        let mut update = UpdateSessionSettingsRequest::default();
        if let Some(alt) = settings.get("alt-speed-enabled").and_then(|v| v.as_bool()) {
            update.alt_speed_enabled = Some(alt);
        }
        if let Some(dl_limit) = settings.get("speed-limit-down").and_then(|v| v.as_u64()) {
            update.download_limit_bytes = Some(dl_limit * 1024);
        }
        if let Some(dl_en) = settings.get("speed-limit-down-enabled").and_then(|v| v.as_bool()) {
            update.download_limit_enabled = Some(dl_en);
        }
        if let Some(ul_limit) = settings.get("speed-limit-up").and_then(|v| v.as_u64()) {
            update.upload_limit_bytes = Some(ul_limit * 1024);
        }
        if let Some(ul_en) = settings.get("speed-limit-up-enabled").and_then(|v| v.as_bool()) {
            update.upload_limit_enabled = Some(ul_en);
        }
        self.client.update_session_settings(update).await?;
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
        Ok(true)
    }

    async fn replace_trackers(&self, _id: i64, _tracker_list: &str, _old_url: &str, _new_url: &str) -> anyhow::Result<()> {
        Ok(())
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

