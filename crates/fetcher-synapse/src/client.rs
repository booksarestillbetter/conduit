//! High-level async gRPC client for Synapse 2.0.

use std::time::Duration;

use tonic::transport::{Channel, Endpoint};
use tonic::Request;

use crate::error::{Result, SynapseClientError};
use crate::proto::v2::add_torrent_request::Source;
use crate::proto::v2::synapse_control_client::SynapseControlClient;
use crate::proto::v2::*;

/// High-level client for the Synapse 2.0 BitTorrent Daemon control plane.
#[derive(Clone)]
pub struct SynapseClient {
    endpoint: String,
    auth_token: Option<String>,
    inner: SynapseControlClient<Channel>,
}

impl SynapseClient {
    /// Connects to a Synapse daemon gRPC endpoint (e.g. `http://127.0.0.1:50051`).
    pub async fn connect(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint_str = endpoint.into();
        let channel = Endpoint::from_shared(endpoint_str.clone())
            .map_err(|e| SynapseClientError::InvalidEndpoint(e.to_string()))?
            .timeout(Duration::from_secs(30))
            .connect()
            .await?;

        let inner = SynapseControlClient::new(channel);
        Ok(Self {
            endpoint: endpoint_str,
            auth_token: None,
            inner,
        })
    }

    /// Lazily connects to a Synapse daemon gRPC endpoint without blocking on initial I/O.
    pub fn connect_lazy(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint_str = endpoint.into();
        let channel = Endpoint::from_shared(endpoint_str.clone())
            .map_err(|e| SynapseClientError::InvalidEndpoint(e.to_string()))?
            .timeout(Duration::from_secs(30))
            .connect_lazy();

        let inner = SynapseControlClient::new(channel);
        Ok(Self {
            endpoint: endpoint_str,
            auth_token: None,
            inner,
        })
    }

    /// Constructs a SynapseClient from an existing Tonic Channel.
    pub fn from_channel(endpoint: impl Into<String>, channel: Channel) -> Self {
        let endpoint_str = endpoint.into();
        let inner = SynapseControlClient::new(channel);
        Self {
            endpoint: endpoint_str,
            auth_token: None,
            inner,
        }
    }

    /// Sets the bearer authentication token for subsequent requests.
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Returns the target endpoint URI.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Returns the active auth token, if configured.
    pub fn auth_token(&self) -> Option<&str> {
        self.auth_token.as_deref()
    }

    /// Attaches bearer authentication metadata if an auth token is configured.
    fn request<T>(&self, message: T) -> Request<T> {
        let mut req = Request::new(message);
        if let Some(ref token) = self.auth_token {
            let header_val = format!("Bearer {token}");
            if let Ok(val) = header_val.parse() {
                req.metadata_mut().insert("authorization", val);
            }
        }
        req
    }

    // --- Swarm Management ---

    /// Adds a torrent using a Magnet URI.
    pub async fn add_magnet(
        &self,
        magnet_uri: &str,
        download_dir: Option<String>,
        start_paused: bool,
    ) -> Result<AddTorrentResponse> {
        let req = self.request(AddTorrentRequest {
            source: Some(Source::MagnetUri(magnet_uri.to_string())),
            download_dir,
            start_paused: Some(start_paused),
        });
        let resp = self.inner.clone().add_torrent(req).await?;
        Ok(resp.into_inner())
    }

    /// Adds a torrent from raw `.torrent` file bytes.
    pub async fn add_torrent_bytes(
        &self,
        torrent_bytes: Vec<u8>,
        download_dir: Option<String>,
        start_paused: bool,
    ) -> Result<AddTorrentResponse> {
        let req = self.request(AddTorrentRequest {
            source: Some(Source::TorrentBytes(torrent_bytes)),
            download_dir,
            start_paused: Some(start_paused),
        });
        let resp = self.inner.clone().add_torrent(req).await?;
        Ok(resp.into_inner())
    }

    /// Adds a torrent from a remote HTTP or HTTPS `.torrent` URL.
    pub async fn add_torrent_url(
        &self,
        url: &str,
        download_dir: Option<String>,
        start_paused: bool,
    ) -> Result<AddTorrentResponse> {
        let req = self.request(AddTorrentRequest {
            source: Some(Source::TorrentUrl(url.to_string())),
            download_dir,
            start_paused: Some(start_paused),
        });
        let resp = self.inner.clone().add_torrent(req).await?;
        Ok(resp.into_inner())
    }

    /// Removes a torrent from the daemon, optionally deleting its files from disk.
    pub async fn remove_torrent(&self, hash: &str, delete_data: bool) -> Result<CommandResponse> {
        let req = self.request(RemoveTorrentRequest {
            hash: hash.to_string(),
            delete_data,
        });
        let resp = self.inner.clone().remove_torrent(req).await?;
        Ok(resp.into_inner())
    }

    /// Pauses (demotes to cold state) an active torrent swarm.
    pub async fn pause_torrent(&self, hash: &str) -> Result<CommandResponse> {
        let req = self.request(TorrentHashRequest {
            hash: hash.to_string(),
        });
        let resp = self.inner.clone().pause_torrent(req).await?;
        Ok(resp.into_inner())
    }

    /// Resumes (promotes to hot state) a paused torrent swarm.
    pub async fn resume_torrent(&self, hash: &str) -> Result<CommandResponse> {
        let req = self.request(TorrentHashRequest {
            hash: hash.to_string(),
        });
        let resp = self.inner.clone().resume_torrent(req).await?;
        Ok(resp.into_inner())
    }

    /// Triggers an asynchronous hash check of all downloaded pieces on disk.
    pub async fn recheck_torrent(&self, hash: &str) -> Result<CommandResponse> {
        let req = self.request(TorrentHashRequest {
            hash: hash.to_string(),
        });
        let resp = self.inner.clone().recheck_torrent(req).await?;
        Ok(resp.into_inner())
    }

    /// Sets the download priority for a specific file index in a multi-file torrent.
    /// Priority levels: 0=skip, 1=low, 4=normal, 7=high.
    pub async fn set_file_priority(
        &self,
        hash: &str,
        file_index: u32,
        priority: u32,
    ) -> Result<CommandResponse> {
        let req = self.request(FilePriorityRequest {
            hash: hash.to_string(),
            file_index,
            priority,
        });
        let resp = self.inner.clone().set_file_priority(req).await?;
        Ok(resp.into_inner())
    }

    /// Relocates a torrent's download directory on disk.
    pub async fn set_location(
        &self,
        hash: &str,
        new_download_dir: &str,
        move_existing_files: bool,
    ) -> Result<CommandResponse> {
        let req = self.request(SetLocationRequest {
            hash: hash.to_string(),
            new_download_dir: new_download_dir.to_string(),
            move_existing_files,
        });
        let resp = self.inner.clone().set_location(req).await?;
        Ok(resp.into_inner())
    }

    /// Sets global or per-torrent download/upload bandwidth limits.
    pub async fn set_rate_limits(
        &self,
        global_download: Option<u64>,
        global_upload: Option<u64>,
        per_torrent_hash: Option<String>,
        torrent_download: Option<u64>,
        torrent_upload: Option<u64>,
    ) -> Result<CommandResponse> {
        let req = self.request(RateLimitsRequest {
            global_download_limit: global_download,
            global_upload_limit: global_upload,
            per_torrent_hash,
            torrent_download_limit: torrent_download,
            torrent_upload_limit: torrent_upload,
        });
        let resp = self.inner.clone().set_rate_limits(req).await?;
        Ok(resp.into_inner())
    }

    /// Queries global session statistics (rates, bytes, swarm counts).
    pub async fn get_session_stats(&self) -> Result<SessionStatsUpdate> {
        let req = self.request(SessionStatsRequest {});
        let resp = self.inner.clone().get_session_stats(req).await?;
        Ok(resp.into_inner())
    }

    // --- Session Settings (Transmission Parity) ---

    /// Queries complete dynamic session settings, queue policies, and Turtle Mode status.
    pub async fn get_session_settings(&self) -> Result<SessionSettingsResponse> {
        let req = self.request(SessionSettingsRequest {});
        let resp = self.inner.clone().get_session_settings(req).await?;
        Ok(resp.into_inner())
    }

    /// Updates dynamic session settings in flight. Returns warnings for any static parameters.
    pub async fn update_session_settings(
        &self,
        update: UpdateSessionSettingsRequest,
    ) -> Result<UpdateSessionSettingsResponse> {
        let req = self.request(update);
        let resp = self.inner.clone().update_session_settings(req).await?;
        Ok(resp.into_inner())
    }

    /// Convenient helper to toggle Turtle Mode (alternative speeds) on the fly.
    pub async fn set_turtle_mode(&self, enabled: bool) -> Result<Vec<String>> {
        let resp = self
            .update_session_settings(UpdateSessionSettingsRequest {
                alt_speed_enabled: Some(enabled),
                ..Default::default()
            })
            .await?;
        Ok(resp.warnings)
    }

    /// Convenient helper to set global rate limits on the fly.
    pub async fn set_bandwidth_limits(
        &self,
        download_limit_bytes: Option<u64>,
        upload_limit_bytes: Option<u64>,
    ) -> Result<Vec<String>> {
        let update = UpdateSessionSettingsRequest {
            download_limit_enabled: download_limit_bytes.map(|b| b > 0),
            download_limit_bytes,
            upload_limit_enabled: upload_limit_bytes.map(|b| b > 0),
            upload_limit_bytes,
            ..Default::default()
        };
        let resp = self.update_session_settings(update).await?;
        Ok(resp.warnings)
    }

    /// Convenient helper to adjust queue concurrency limits in flight.
    pub async fn set_queue_concurrency(
        &self,
        download_queue_size: Option<u32>,
        seed_queue_size: Option<u32>,
        max_active_torrents: Option<u32>,
    ) -> Result<Vec<String>> {
        let update = UpdateSessionSettingsRequest {
            download_queue_size,
            seed_queue_size,
            max_active_torrents,
            ..Default::default()
        };
        let resp = self.update_session_settings(update).await?;
        Ok(resp.warnings)
    }

    // --- Streaming Telemetry ---

    /// Subscribes to the live sparse delta coalesced torrent stream.
    pub async fn subscribe_torrents(
        &self,
        chunk_size: u32,
        flush_window_ms: u32,
    ) -> Result<tonic::Streaming<TorrentListEvent>> {
        let req = self.request(SubscribeTorrentsRequest {
            chunk_size,
            flush_window_ms,
        });
        let resp = self.inner.clone().subscribe_torrents(req).await?;
        Ok(resp.into_inner())
    }

    /// Subscribes to live detailed inspection of a single torrent swarm.
    pub async fn subscribe_torrent_detail(
        &self,
        hash: &str,
        refresh_interval_ms: u32,
    ) -> Result<tonic::Streaming<TorrentDetailEvent>> {
        let req = self.request(TorrentDetailRequest {
            hash: hash.to_string(),
            refresh_interval_ms,
        });
        let resp = self.inner.clone().subscribe_torrent_detail(req).await?;
        Ok(resp.into_inner())
    }

    /// Subscribes to the daemon's alert stream (torrent finished, peer banned, tracker announced,
    /// ...), optionally only for one torrent (40-character hex info hash).
    pub async fn subscribe_alerts(
        &self,
        info_hash: Option<&str>,
    ) -> Result<tonic::Streaming<AlertEvent>> {
        let req = self.request(SubscribeAlertsRequest {
            info_hash: info_hash.map(str::to_string),
        });
        let resp = self.inner.clone().subscribe_alerts(req).await?;
        Ok(resp.into_inner())
    }

    /// Subscribes to periodic global session stats telemetry.
    pub async fn subscribe_session_stats(&self) -> Result<tonic::Streaming<SessionStatsUpdate>> {
        let req = self.request(SessionStatsRequest {});
        let resp = self.inner.clone().subscribe_session_stats(req).await?;
        Ok(resp.into_inner())
    }

    // --- Queue, picking, trackers, blocklist ---

    /// Moves torrents within the download queue. `direction` is `top`, `up`, `down` or `bottom`.
    pub async fn move_in_queue(&self, hashes: &[String], direction: &str) -> Result<CommandResponse> {
        let direction = match direction.to_lowercase().as_str() {
            "top" => move_in_queue_request::Direction::Top,
            "up" => move_in_queue_request::Direction::Up,
            "down" => move_in_queue_request::Direction::Down,
            "bottom" => move_in_queue_request::Direction::Bottom,
            other => {
                return Err(SynapseClientError::Rpc {
                    code: tonic::Code::InvalidArgument,
                    message: format!("invalid queue move direction '{other}'"),
                })
            }
        };
        let req = self.request(MoveInQueueRequest {
            hashes: hashes.to_vec(),
            direction: direction as i32,
        });
        Ok(self.inner.clone().move_in_queue(req).await?.into_inner())
    }

    /// Turns in-order piece picking on or off for the given torrents.
    pub async fn set_sequential_download(&self, hashes: &[String], enabled: bool) -> Result<CommandResponse> {
        let req = self.request(SequentialDownloadRequest {
            hashes: hashes.to_vec(),
            enabled,
        });
        Ok(self.inner.clone().set_sequential_download(req).await?.into_inner())
    }

    /// Announces to the given torrents' trackers now.
    pub async fn reannounce_torrents(&self, hashes: &[String]) -> Result<CommandResponse> {
        let req = self.request(TorrentHashesRequest {
            hashes: hashes.to_vec(),
        });
        Ok(self.inner.clone().reannounce_torrents(req).await?.into_inner())
    }

    /// Replaces a torrent's trackers. An empty list restores the ones in its metainfo.
    pub async fn replace_trackers(&self, hash: &str, trackers: Vec<String>) -> Result<CommandResponse> {
        let req = self.request(ReplaceTrackersRequest {
            hash: hash.to_string(),
            trackers,
        });
        Ok(self.inner.clone().replace_trackers(req).await?.into_inner())
    }

    /// Re-reads the daemon's IP filter from its configured sources; returns the rule count.
    pub async fn reload_ip_filter(&self) -> Result<u32> {
        let req = self.request(Empty {});
        Ok(self.inner.clone().reload_ip_filter(req).await?.into_inner().rules)
    }

    // --- Capability Negotiation & Circuit Breaker Control ---

    /// Returns this daemon's version and supported feature names. A daemon predating this
    /// RPC returns a `SynapseClientError::Rpc` with `code: tonic::Code::Unimplemented` —
    /// callers should treat that as "no optional features", not a connection failure.
    pub async fn get_capabilities(&self) -> Result<CapabilitiesResponse> {
        let req = self.request(Empty {});
        let resp = self.inner.clone().get_capabilities(req).await?;
        Ok(resp.into_inner())
    }

    /// Lists the live circuit breaker status for every tracker host this daemon is
    /// currently tracking.
    pub async fn list_circuit_breakers(&self) -> Result<Vec<CircuitBreakerStatus>> {
        let req = self.request(Empty {});
        let resp = self.inner.clone().list_circuit_breakers(req).await?;
        Ok(resp.into_inner().breakers)
    }

    /// Forces a tracker host's circuit breaker into `Tripped`.
    pub async fn force_trip_circuit_breaker(&self, host: &str) -> Result<CommandResponse> {
        let req = self.request(CircuitBreakerActionRequest {
            host: host.to_string(),
            action: circuit_breaker_action_request::Action::Trip as i32,
        });
        let resp = self.inner.clone().force_circuit_breaker_action(req).await?;
        Ok(resp.into_inner())
    }

    /// Clears a tracker host's circuit breaker state entirely.
    pub async fn force_reset_circuit_breaker(&self, host: &str) -> Result<CommandResponse> {
        let req = self.request(CircuitBreakerActionRequest {
            host: host.to_string(),
            action: circuit_breaker_action_request::Action::Reset as i32,
        });
        let resp = self.inner.clone().force_circuit_breaker_action(req).await?;
        Ok(resp.into_inner())
    }
}
