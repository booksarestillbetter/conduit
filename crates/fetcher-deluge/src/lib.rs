use async_trait::async_trait;
use fetcher_core::{
    FetcherDriverFactory, FetcherNodeConfig, RetrieverClientType, Torrent, TorrentClientTrait,
    TorrentFile, TrackerStat,
};
use parking_lot::RwLock;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct DelugeClient {
    config: FetcherNodeConfig,
    http: reqwest::Client,
    cookie_session: Arc<RwLock<Option<String>>>,
    rpc_url: String,
    msg_id: Arc<AtomicI64>,
}

impl DelugeClient {
    pub fn new(config: FetcherNodeConfig) -> Self {
        let scheme = if config.use_ssl { "https" } else { "http" };
        let rpc_url = format!("{}://{}:{}/json", scheme, config.host, config.port);

        let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(15));
        if !config.verify_tls {
            builder = builder.danger_accept_invalid_certs(true);
        }
        let http = builder.build().unwrap_or_default();

        Self {
            config,
            http,
            cookie_session: Arc::new(RwLock::new(None)),
            rpc_url,
            msg_id: Arc::new(AtomicI64::new(1)),
        }
    }

    async fn ensure_authenticated(&self) -> anyhow::Result<()> {
        if self.cookie_session.read().is_some() {
            return Ok(());
        }

        let password = self.config.password.as_deref().unwrap_or("deluge");
        let id = self.msg_id.fetch_add(1, Ordering::Relaxed);
        let payload = json!({
            "method": "auth.login",
            "params": [password],
            "id": id,
        });

        let resp = self.http.post(&self.rpc_url).json(&payload).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Deluge auth request failed (HTTP {})", resp.status()));
        }

        for cookie in resp.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(cookie_str) = cookie.to_str() {
                if let Some(cookie_val) = cookie_str.split(';').next() {
                    *self.cookie_session.write() = Some(cookie_val.to_string());
                    debug!("Authenticated with Deluge node {}", self.config.name);
                    return Ok(());
                }
            }
        }

        // If no Set-Cookie returned, verify JSON body result
        let body: Value = resp.json().await.unwrap_or_default();
        if body.get("result").and_then(|r| r.as_bool()).unwrap_or(false) {
            *self.cookie_session.write() = Some("auth_ok".to_string());
            return Ok(());
        }

        Err(anyhow::anyhow!("Deluge authentication rejected: invalid password"))
    }

    async fn send_rpc(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.ensure_authenticated().await?;

        for attempt in 0..2 {
            let id = self.msg_id.fetch_add(1, Ordering::Relaxed);
            let payload = json!({
                "method": method,
                "params": params,
                "id": id,
            });

            let mut req = self.http.post(&self.rpc_url).json(&payload);
            if let Some(ref cookie) = *self.cookie_session.read() {
                if cookie != "auth_ok" {
                    req = req.header(reqwest::header::COOKIE, cookie);
                }
            }

            let resp = req.send().await?;
            if !resp.status().is_success() {
                return Err(anyhow::anyhow!("Deluge HTTP error (HTTP {})", resp.status()));
            }

            let body: Value = resp.json().await?;
            if let Some(err) = body.get("error") {
                if !err.is_null() {
                    let err_msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("Deluge RPC error");
                    if err_msg.to_lowercase().contains("not authenticated") && attempt == 0 {
                        *self.cookie_session.write() = None;
                        self.ensure_authenticated().await?;
                        continue;
                    }
                    return Err(anyhow::anyhow!("Deluge RPC error: {}", err_msg));
                }
            }

            return Ok(body.get("result").cloned().unwrap_or(Value::Null));
        }

        Err(anyhow::anyhow!("Failed Deluge RPC call after re-authentication"))
    }

    fn map_deluge_to_torrent(&self, hash: &str, obj: &Value, numeric_id: i64) -> Torrent {
        let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let state = obj.get("state").and_then(|v| v.as_str()).unwrap_or("");
        let progress = obj.get("progress").and_then(|v| v.as_f64()).unwrap_or(0.0) / 100.0;
        let upspeed = obj.get("upload_payload_rate").and_then(|v| v.as_i64()).unwrap_or(0);
        let dlspeed = obj.get("download_payload_rate").and_then(|v| v.as_i64()).unwrap_or(0);
        let uploaded = obj.get("total_uploaded").and_then(|v| v.as_i64()).unwrap_or(0);
        let downloaded = obj.get("total_done").and_then(|v| v.as_i64()).unwrap_or(0);
        let total_size = obj.get("total_size").and_then(|v| v.as_i64()).unwrap_or(0);
        let ratio = obj.get("ratio").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let eta = obj.get("eta").and_then(|v| v.as_i64()).unwrap_or(0);
        let save_path = obj.get("save_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let time_added = obj.get("time_added").and_then(|v| v.as_i64()).unwrap_or(0);
        let num_seeds = obj.get("num_seeds").and_then(|v| v.as_i64()).unwrap_or(0);
        let num_peers = obj.get("num_peers").and_then(|v| v.as_i64()).unwrap_or(0);

        // "files" ({index, path, size, offset} per Deluge's Torrent.get_files) and
        // "file_progress" (a flat list of 0.0-1.0 fractions, indexed identically to
        // "files") are only present when explicitly requested -- see
        // `get_torrent_details_by_hash`, the only caller that asks for them.
        let files = obj.get("files").and_then(|v| v.as_array()).map(|files_arr| {
            let progress_arr = obj.get("file_progress").and_then(|v| v.as_array());
            files_arr
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let length = f.get("size").and_then(|v| v.as_i64()).unwrap_or(0);
                    let frac = progress_arr
                        .and_then(|p| p.get(i))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    TorrentFile {
                        name: f.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        bytes_completed: (length as f64 * frac) as i64,
                        length,
                    }
                })
                .collect::<Vec<_>>()
        });

        let (status_code, error_str) = match state {
            "Downloading" => (4, None),
            "Seeding" => (6, None),
            "Paused" => (0, None),
            "Checking" => (2, None),
            "Queued" => (3, None),
            "Error" => (16, Some("Deluge reported error state".to_string())),
            _ => (0, None),
        };

        let mut tracker_stats = Vec::new();
        if let Some(trackers_arr) = obj.get("trackers").and_then(|t| t.as_array()) {
            for tr in trackers_arr {
                if let Some(url) = tr.get("url").and_then(|u| u.as_str()) {
                    tracker_stats.push(TrackerStat {
                        announce: url.to_string(),
                        host: url.replace("http://", "").replace("https://", "").split('/').next().unwrap_or(url).to_string(),
                        seeder_count: num_seeds,
                        leecher_count: num_peers,
                        download_count: 0,
                        last_announce_succeeded: true,
                        last_announce_result: "Success".to_string(),
                        is_backup: false,
                    });
                }
            }
        }

        Torrent {
            id: numeric_id,
            name,
            hash_string: hash.to_string(),
            status: status_code,
            rate_upload: upspeed,
            rate_download: dlspeed,
            uploaded_ever: uploaded,
            downloaded_ever: downloaded,
            upload_ratio: ratio,
            total_size,
            size_when_done: total_size,
            left_until_done: (total_size as f64 * (1.0 - progress)).max(0.0) as i64,
            percent_done: progress,
            eta,
            eta_idle: None,
            error: if error_str.is_some() { 1 } else { 0 },
            error_string: error_str.unwrap_or_default(),
            peers_connected: num_seeds + num_peers,
            peers_sending_to_us: num_peers,
            peers_getting_from_us: num_seeds,
            added_date: time_added,
            done_date: 0,
            download_dir: save_path,
            tracker_stats,
            queue_position: 0,
            files,
            peers: None,
            comment: None,
            creator: None,
            date_created: None,
            piece_count: None,
            piece_size: None,
            is_private: None,
            magnet_link: None,
            corrupt_ever: None,
            seconds_downloading: None,
            seconds_seeding: None,
            activity_date: None,
            sequential_download: false,
            pieces: None,
            availability: None,
        }
    }
}

#[async_trait]
impl TorrentClientTrait for DelugeClient {
    fn node_name(&self) -> &str {
        &self.config.name
    }

    fn config(&self) -> &FetcherNodeConfig {
        &self.config
    }

    fn client_type(&self) -> RetrieverClientType {
        RetrieverClientType::Deluge
    }

    async fn get_torrents(&self, _ids: Option<Vec<i64>>) -> anyhow::Result<Vec<Torrent>> {
        let fields = vec![
            "name", "hash", "state", "progress", "upload_payload_rate", "download_payload_rate",
            "total_uploaded", "total_done", "total_size", "tracker_status", "trackers",
            "save_path", "time_added", "num_seeds", "num_peers", "total_seeds", "total_peers",
            "eta", "ratio",
        ];

        let result = self.send_rpc("core.get_torrents_status", json!([{}, fields])).await?;
        let map: HashMap<String, Value> = serde_json::from_value(result).unwrap_or_default();

        let mut torrents = Vec::with_capacity(map.len());
        for (idx, (hash, obj)) in map.into_iter().enumerate() {
            torrents.push(self.map_deluge_to_torrent(&hash, &obj, (idx + 1) as i64));
        }

        Ok(torrents)
    }

    async fn get_torrent_details(&self, id: i64) -> anyhow::Result<Torrent> {
        let torrents = self.get_torrents(None).await?;
        torrents
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| anyhow::anyhow!("Torrent id {} not found on node {}", id, self.config.name))
    }

    async fn get_torrent_details_by_hash(&self, hash: &str) -> anyhow::Result<Torrent> {
        let fields = vec![
            "name", "hash", "state", "progress", "upload_payload_rate", "download_payload_rate",
            "total_uploaded", "total_done", "total_size", "tracker_status", "trackers",
            "save_path", "time_added", "num_seeds", "num_peers", "files", "file_progress", "peers", "ratio", "eta",
        ];

        let result = self.send_rpc("core.get_torrent_status", json!([hash, fields])).await?;
        Ok(self.map_deluge_to_torrent(hash, &result, 1))
    }

    async fn add_torrent(
        &self,
        magnet_or_url: Option<&str>,
        metainfo_b64: Option<&str>,
        download_dir: Option<&str>,
        _paused: bool,
    ) -> anyhow::Result<Value> {
        let mut opts = json!({});
        if let Some(dir) = download_dir {
            opts["download_location"] = json!(dir);
        }

        if let Some(url) = magnet_or_url {
            let res = self.send_rpc("core.add_torrent_magnet", json!([url, opts])).await?;
            return Ok(json!({ "result": "success", "hash": res }));
        }

        if let Some(b64) = metainfo_b64 {
            let res = self.send_rpc("core.add_torrent_file", json!(["torrent.torrent", b64, opts])).await?;
            return Ok(json!({ "result": "success", "hash": res }));
        }

        Err(anyhow::anyhow!("No torrent URL or metainfo payload provided"))
    }

    async fn start_torrents(&self, ids: &[i64], _now: bool) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        self.send_rpc("core.resume_torrents", json!([hashes])).await?;
        Ok(())
    }

    async fn stop_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        self.send_rpc("core.pause_torrents", json!([hashes])).await?;
        Ok(())
    }

    async fn verify_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Err(anyhow::anyhow!("None of the requested torrents were found on this node"));
        }
        // One RPC per hash, and deliberately not `?` on the first error — a failure on one
        // torrent used to silently abort the loop (via `let _ =`, swallowed) and leave every
        // later torrent in the batch unrecheck, while the caller was told the whole batch
        // succeeded. Attempt every torrent regardless of earlier failures, then report if any
        // of them didn't actually take.
        let mut failed = Vec::new();
        for hash in &hashes {
            if let Err(e) = self.send_rpc("core.force_recheck", json!([[hash]])).await {
                failed.push(format!("{}: {}", hash, e));
            }
        }
        if !failed.is_empty() {
            return Err(anyhow::anyhow!("Recheck failed for {}/{} torrent(s): {}", failed.len(), hashes.len(), failed.join("; ")));
        }
        Ok(())
    }

    async fn reannounce_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Err(anyhow::anyhow!("None of the requested torrents were found on this node"));
        }
        let mut failed = Vec::new();
        for hash in &hashes {
            if let Err(e) = self.send_rpc("core.force_reannounce", json!([[hash]])).await {
                failed.push(format!("{}: {}", hash, e));
            }
        }
        if !failed.is_empty() {
            return Err(anyhow::anyhow!("Reannounce failed for {}/{} torrent(s): {}", failed.len(), hashes.len(), failed.join("; ")));
        }
        Ok(())
    }

    async fn remove_torrents(&self, ids: &[i64], delete_local_data: bool) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Err(anyhow::anyhow!("None of the requested torrents were found on this node"));
        }
        // Previously used `?` inside this loop: one removal failing (e.g. a transient RPC
        // error) returned immediately, abandoning every remaining torrent in the batch
        // un-removed while also reporting the whole node's batch as failed even though earlier
        // hashes in the loop had already been removed successfully. Remove every resolved hash
        // regardless of earlier failures, then report which ones didn't take.
        let mut failed = Vec::new();
        for hash in &hashes {
            if let Err(e) = self.send_rpc("core.remove_torrent", json!([hash, delete_local_data])).await {
                failed.push(format!("{}: {}", hash, e));
            }
        }
        if !failed.is_empty() {
            return Err(anyhow::anyhow!("Remove failed for {}/{} torrent(s): {}", failed.len(), hashes.len(), failed.join("; ")));
        }
        Ok(())
    }

    async fn set_location(&self, ids: &[i64], location: &str, _move_data: bool) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        self.send_rpc("core.move_storage", json!([hashes, location])).await?;
        Ok(())
    }

    async fn queue_move(&self, ids: &[i64], direction: &str) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        let method = match direction.to_lowercase().as_str() {
            "top" => "core.queue_top",
            "up" => "core.queue_up",
            "down" => "core.queue_down",
            "bottom" => "core.queue_bottom",
            other => return Err(anyhow::anyhow!("Invalid queue direction {}", other)),
        };
        // A single batched call (Deluge natively accepts the whole hash list) rather than
        // one RPC per torrent -- besides being slower, moving a multi-selection one torrent
        // at a time doesn't preserve their relative order the way a real batch move does,
        // and silently swallowing a per-call failure (the previous `let _ =` loop) hid any
        // torrent that failed to move from the caller entirely.
        self.send_rpc(method, json!([hashes])).await?;
        Ok(())
    }

    /// `sequential_download` is a per-torrent option under Deluge's generic
    /// `core.set_torrent_options(torrent_ids, options)` RPC (see `torrent.TorrentOptions`).
    async fn set_sequential_download(&self, ids: &[i64], enabled: bool) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        self.send_rpc("core.set_torrent_options", json!([hashes, {"sequential_download": enabled}])).await?;
        Ok(())
    }

    /// Deluge splits file/folder renaming into two distinct RPCs: `core.rename_files`
    /// (by file index) and `core.rename_folder` (by current/new folder path string).
    /// `path` here is always a single file's current in-torrent relative path (the only
    /// caller is the per-file rename control in the torrent details UI, which lists
    /// individual files) -- so this resolves `path` to its file index via the same
    /// `files` listing `get_torrent_details_by_hash` now populates, then renames by index.
    async fn rename_path(&self, id: i64, path: &str, new_name: &str) -> anyhow::Result<Value> {
        let torrent = self.get_torrent_details(id).await?;
        let details = self.get_torrent_details_by_hash(&torrent.hash_string).await?;
        let files = details.files.unwrap_or_default();
        let index = files
            .iter()
            .position(|f| f.name == path)
            .ok_or_else(|| anyhow::anyhow!("No file matching path '{}' found in torrent", path))?;
        self.send_rpc("core.rename_files", json!([torrent.hash_string, [[index, new_name]]])).await?;
        Ok(json!({ "result": "success" }))
    }

    /// Unlike Transmission/qBittorrent, Deluge's core RPC has no built-in alternate-speed
    /// ("turtle mode") toggle -- only raw `max_download_speed`/`max_upload_speed` config
    /// values, with no remembered secondary "alt" limit pair to swap to/from. Implementing
    /// an equivalent would mean inventing new per-node "turtle speed" config that doesn't
    /// exist anywhere in Conduit today; returning a clear error here is honest about that
    /// gap instead of silently doing nothing while a UI toggle claims success.
    async fn set_turtle_mode(&self, _enabled: bool) -> anyhow::Result<()> {
        anyhow::bail!("Deluge has no native turtle-mode / alternate-speed toggle; use per-node bandwidth limits instead")
    }

    async fn update_blocklist(&self) -> anyhow::Result<i64> {
        anyhow::bail!("Deluge has no built-in blocklist RPC; use the Blocklist plugin's own settings instead")
    }

    async fn get_free_space(&self, path: &str) -> anyhow::Result<i64> {
        let res = self.send_rpc("core.get_free_space", json!([path])).await?;
        Ok(res.as_i64().unwrap_or(0))
    }

    async fn get_session(&self) -> anyhow::Result<Value> {
        self.send_rpc("core.get_config", json!([])).await
    }

    async fn set_session(&self, settings: Value) -> anyhow::Result<()> {
        self.send_rpc("core.set_config", json!([settings])).await?;
        Ok(())
    }

    async fn get_session_stats(&self) -> anyhow::Result<Value> {
        self.send_rpc("core.get_session_state", json!([])).await
    }

    async fn test_port(&self) -> anyhow::Result<bool> {
        Ok(true)
    }

    /// `tracker_list` is the caller's already-computed, post-replacement announce list --
    /// each line its own tier (see the caller in `src/api/torrent_routes.rs`). Deluge's
    /// `core.set_torrent_trackers` wants `[{"url", "tier"}]` rather than a flat string, so
    /// this just re-splits it back into that shape, assigning each line its own tier index
    /// (consistent with how the caller built it).
    async fn replace_trackers(&self, id: i64, tracker_list: &str, _old_url: &str, _new_url: &str) -> anyhow::Result<()> {
        let torrent = self.get_torrent_details(id).await?;
        let trackers: Vec<Value> = tracker_list
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .enumerate()
            .map(|(tier, url)| json!({"url": url, "tier": tier}))
            .collect();
        self.send_rpc("core.set_torrent_trackers", json!([torrent.hash_string, trackers])).await?;
        Ok(())
    }
}

pub struct DelugeDriverFactory;

impl FetcherDriverFactory for DelugeDriverFactory {
    fn client_type(&self) -> RetrieverClientType {
        RetrieverClientType::Deluge
    }

    fn create_client(&self, config: FetcherNodeConfig) -> Arc<dyn TorrentClientTrait> {
        Arc::new(DelugeClient::new(config))
    }
}

