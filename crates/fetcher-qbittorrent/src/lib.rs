use async_trait::async_trait;
use fetcher_core::{
    FetcherDriverFactory, FetcherNodeConfig, RetrieverClientType, Torrent, TorrentClientTrait,
    TorrentFile, TorrentPeer, TrackerStat,
};
use parking_lot::RwLock;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct QBittorrentClient {
    config: FetcherNodeConfig,
    http: reqwest::Client,
    cookie_sid: Arc<RwLock<Option<String>>>,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct QbitTorrentItem {
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub total_size: i64,
    #[serde(default)]
    pub amount_left: i64,
    #[serde(default)]
    pub dlspeed: i64,
    #[serde(default)]
    pub upspeed: i64,
    #[serde(default)]
    pub downloaded: i64,
    #[serde(default)]
    pub uploaded: i64,
    #[serde(default)]
    pub ratio: f64,
    #[serde(default)]
    pub eta: i64,
    #[serde(default)]
    pub num_seeds: i64,
    #[serde(default)]
    pub num_leechs: i64,
    #[serde(default)]
    pub added_on: i64,
    #[serde(default)]
    pub completion_on: i64,
    #[serde(default)]
    pub save_path: String,
    #[serde(default)]
    pub tracker: String,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub seq_dl: Option<bool>,
}

impl QBittorrentClient {
    pub fn new(config: FetcherNodeConfig) -> Self {
        let scheme = if config.use_ssl { "https" } else { "http" };
        let base_url = format!("{}://{}:{}", scheme, config.host, config.port);

        let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(15));
        if !config.verify_tls {
            builder = builder.danger_accept_invalid_certs(true);
        }
        let http = builder.build().unwrap_or_default();

        Self {
            config,
            http,
            cookie_sid: Arc::new(RwLock::new(None)),
            base_url,
        }
    }

    async fn ensure_authenticated(&self) -> anyhow::Result<()> {
        if self.cookie_sid.read().is_some() {
            return Ok(());
        }

        let login_url = format!("{}/api/v2/auth/login", self.base_url);
        let mut params = HashMap::new();
        if let Some(ref u) = self.config.username {
            params.insert("username", u.as_str());
        }
        if let Some(ref p) = self.config.password {
            params.insert("password", p.as_str());
        }

        let resp = self.http.post(&login_url).form(&params).send().await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("qBittorrent login failed (HTTP {})", resp.status()));
        }

        let cookies: Vec<String> = resp.headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|c| c.to_str().ok().map(|s| s.to_string()))
            .collect();

        let text = resp.text().await.unwrap_or_default();
        if text.to_lowercase().contains("fails") {
            return Err(anyhow::anyhow!("qBittorrent invalid username/password"));
        }

        for cookie_str in cookies {
            if let Some(sid_part) = cookie_str.split(';').next() {
                if sid_part.starts_with("SID=") {
                    let sid = sid_part.trim_start_matches("SID=").to_string();
                    *self.cookie_sid.write() = Some(sid);
                    debug!("Authenticated with qBittorrent node {}", self.config.name);
                    return Ok(());
                }
            }
        }

        // If no Set-Cookie returned but login responded OK, assume open auth
        *self.cookie_sid.write() = Some("auth_ok".to_string());
        Ok(())
    }

    async fn send_api_get(&self, endpoint: &str, query: &[(&str, &str)]) -> anyhow::Result<reqwest::Response> {
        self.ensure_authenticated().await?;
        let url = format!("{}{}", self.base_url, endpoint);

        for attempt in 0..2 {
            let mut req = self.http.get(&url).query(query);
            if let Some(ref sid) = *self.cookie_sid.read() {
                if sid != "auth_ok" {
                    req = req.header(reqwest::header::COOKIE, format!("SID={}", sid));
                }
            }

            let resp = req.send().await?;
            if resp.status().as_u16() == 403 && attempt == 0 {
                // Auth expired, re-login
                *self.cookie_sid.write() = None;
                self.ensure_authenticated().await?;
                continue;
            }
            if !resp.status().is_success() {
                return Err(anyhow::anyhow!("qBittorrent API error ({}) HTTP {}", endpoint, resp.status()));
            }
            return Ok(resp);
        }

        Err(anyhow::anyhow!("Failed request to qBittorrent after re-authentication"))
    }

    async fn send_api_post(&self, endpoint: &str, params: &[(&str, &str)]) -> anyhow::Result<reqwest::Response> {
        self.ensure_authenticated().await?;
        let url = format!("{}{}", self.base_url, endpoint);

        for attempt in 0..2 {
            let mut req = self.http.post(&url).form(params);
            if let Some(ref sid) = *self.cookie_sid.read() {
                if sid != "auth_ok" {
                    req = req.header(reqwest::header::COOKIE, format!("SID={}", sid));
                }
            }

            let resp = req.send().await?;
            if resp.status().as_u16() == 403 && attempt == 0 {
                *self.cookie_sid.write() = None;
                self.ensure_authenticated().await?;
                continue;
            }
            if !resp.status().is_success() {
                return Err(anyhow::anyhow!("qBittorrent API POST error ({}) HTTP {}", endpoint, resp.status()));
            }
            return Ok(resp);
        }

        Err(anyhow::anyhow!("Failed POST request to qBittorrent after re-authentication"))
    }

    async fn send_api_multipart(&self, endpoint: &str, form: reqwest::multipart::Form) -> anyhow::Result<reqwest::Response> {
        self.ensure_authenticated().await?;
        let url = format!("{}{}", self.base_url, endpoint);

        let mut req = self.http.post(&url);
        if let Some(ref sid) = *self.cookie_sid.read() {
            if sid != "auth_ok" {
                req = req.header(reqwest::header::COOKIE, format!("SID={}", sid));
            }
        }

        let resp = req.multipart(form).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("qBittorrent API Multipart error ({}) HTTP {}", endpoint, resp.status()));
        }
        Ok(resp)
    }

    fn map_qbit_to_torrent(&self, q: QbitTorrentItem, numeric_id: i64) -> Torrent {
        let (status_code, error_str) = match q.state.as_str() {
            "downloading" | "stalledDL" | "forcedDL" => (4, None),
            "uploading" | "stalledUP" | "forcedUP" => (6, None),
            "pausedDL" | "pausedUP" => (0, None),
            "checkingDL" | "checkingUP" | "checkingResumeData" => (2, None),
            "queuedDL" => (3, None),
            "queuedUP" => (5, None),
            "error" | "missingFiles" => (16, Some("qBittorrent report: error or missing files".to_string())),
            _ => (0, None),
        };

        let mut tracker_stats = Vec::new();
        if !q.tracker.is_empty() {
            tracker_stats.push(TrackerStat {
                announce: q.tracker.clone(),
                host: q.tracker.replace("http://", "").replace("https://", "").split('/').next().unwrap_or(&q.tracker).to_string(),
                seeder_count: q.num_seeds,
                leecher_count: q.num_leechs,
                download_count: 0,
                last_announce_succeeded: true,
                last_announce_result: "Success".to_string(),
                is_backup: false,
            });
        }

        let total_sz = if q.total_size > 0 { q.total_size } else { q.size };

        Torrent {
            id: numeric_id,
            name: q.name,
            hash_string: q.hash,
            status: status_code,
            rate_upload: q.upspeed,
            rate_download: q.dlspeed,
            uploaded_ever: q.uploaded,
            downloaded_ever: q.downloaded,
            upload_ratio: q.ratio,
            total_size: total_sz,
            size_when_done: total_sz,
            left_until_done: q.amount_left,
            percent_done: q.progress,
            eta: q.eta,
            eta_idle: None,
            error: if error_str.is_some() { 1 } else { 0 },
            error_string: error_str.unwrap_or_default(),
            peers_connected: q.num_seeds + q.num_leechs,
            peers_sending_to_us: q.num_leechs,
            peers_getting_from_us: q.num_seeds,
            added_date: q.added_on,
            done_date: q.completion_on,
            download_dir: q.save_path,
            tracker_stats,
            queue_position: q.priority,
            files: None,
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
            sequential_download: q.seq_dl.unwrap_or(false),
            pieces: None,
            availability: None,
        }
    }
}

#[async_trait]
impl TorrentClientTrait for QBittorrentClient {
    fn node_name(&self) -> &str {
        &self.config.name
    }

    fn config(&self) -> &FetcherNodeConfig {
        &self.config
    }

    fn client_type(&self) -> RetrieverClientType {
        RetrieverClientType::QBittorrent
    }

    async fn get_torrents(&self, _ids: Option<Vec<i64>>) -> anyhow::Result<Vec<Torrent>> {
        let resp = self.send_api_get("/api/v2/torrents/info", &[]).await?;
        let items: Vec<QbitTorrentItem> = resp.json().await.unwrap_or_default();

        let mut torrents = Vec::with_capacity(items.len());
        for (idx, item) in items.into_iter().enumerate() {
            torrents.push(self.map_qbit_to_torrent(item, (idx + 1) as i64));
        }

        Ok(torrents)
    }

    async fn get_torrent_details(&self, id: i64) -> anyhow::Result<Torrent> {
        let torrents = self.get_torrents(None).await?;
        let t = torrents
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| anyhow::anyhow!("Torrent id {} not found on node {}", id, self.config.name))?;
        self.get_torrent_details_by_hash(&t.hash_string).await
    }

    async fn get_torrent_details_by_hash(&self, hash: &str) -> anyhow::Result<Torrent> {
        let resp = self.send_api_get("/api/v2/torrents/info", &[("hashes", hash)]).await?;
        let items: Vec<QbitTorrentItem> = resp.json().await.unwrap_or_default();
        let item = items.into_iter().next().ok_or_else(|| anyhow::anyhow!("Torrent {} not found", hash))?;

        let mut torrent = self.map_qbit_to_torrent(item, 1);

        // Fetch detailed files list
        if let Ok(files_resp) = self.send_api_get("/api/v2/torrents/files", &[("hash", hash)]).await {
            if let Ok(files_json) = files_resp.json::<Vec<serde_json::Value>>().await {
                let mut files = Vec::with_capacity(files_json.len());
                for f in files_json {
                    files.push(TorrentFile {
                        name: f.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                        bytes_completed: ((f.get("progress").and_then(|p| p.as_f64()).unwrap_or(0.0)) * (f.get("size").and_then(|s| s.as_f64()).unwrap_or(0.0))) as i64,
                        length: f.get("size").and_then(|s| s.as_i64()).unwrap_or(0),
                    });
                }
                torrent.files = Some(files);
            }
        }

        // Fetch piece states if available
        if let Ok(pieces_resp) = self.send_api_get("/api/v2/torrents/pieceStates", &[("hash", hash)]).await {
            if let Ok(pieces_arr) = pieces_resp.json::<Vec<i64>>().await {
                let mut pieces_b64 = String::new();
                for state in pieces_arr {
                    pieces_b64.push(if state == 2 { '1' } else { '0' });
                }
                torrent.pieces = Some(pieces_b64);
            }
        }

        // Fetch connected peers list from qBittorrent /sync/torrent_peers
        if let Ok(peers_resp) = self.send_api_get("/api/v2/sync/torrent_peers", &[("hash", hash)]).await {
            if let Ok(peers_json) = peers_resp.json::<serde_json::Value>().await {
                if let Some(peers_obj) = peers_json.get("peers").and_then(|p| p.as_object()) {
                    let mut peer_list = Vec::with_capacity(peers_obj.len());
                    for (addr_key, p) in peers_obj {
                        let ip = p.get("ip").and_then(|v| v.as_str()).unwrap_or(addr_key.as_str());
                        let port = p.get("port").and_then(|v| v.as_i64()).unwrap_or(0);
                        let client = p.get("client").and_then(|v| v.as_str()).unwrap_or("Unknown Client");
                        let dl_speed = p.get("dl_speed").and_then(|v| v.as_i64()).unwrap_or(0);
                        let up_speed = p.get("up_speed").and_then(|v| v.as_i64()).unwrap_or(0);
                        let progress = p.get("progress").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let flags = p.get("flags").and_then(|v| v.as_str()).unwrap_or("");
                        let connection = p.get("connection").and_then(|v| v.as_str()).unwrap_or("");
                        let is_encrypted = flags.contains('E') || connection.to_lowercase().contains("encrypted");

                        peer_list.push(TorrentPeer {
                            address: ip.to_string(),
                            client_name: client.to_string(),
                            client_is_choked: false,
                            client_is_interested: false,
                            flagstr: flags.to_string(),
                            is_downloading_from: dl_speed > 0,
                            is_encrypted,
                            is_incoming: flags.contains('I'),
                            is_uploading_to: up_speed > 0,
                            is_utp: connection.to_lowercase().contains("utp") || flags.contains('U'),
                            peer_is_choked: false,
                            peer_is_interested: false,
                            port,
                            progress,
                            rate_to_client: dl_speed,
                            rate_to_peer: up_speed,
                            country_code: p.get("country_code").and_then(|v| v.as_str()).map(|s| s.to_string()),
                            as_name: None,
                        });
                    }
                    torrent.peers = Some(peer_list);
                }
            }
        }

        Ok(torrent)
    }

    async fn add_torrent(
        &self,
        magnet_or_url: Option<&str>,
        metainfo_b64: Option<&str>,
        download_dir: Option<&str>,
        paused: bool,
    ) -> anyhow::Result<Value> {
        let paused_str = if paused { "true" } else { "false" };

        if let Some(meta_b64) = metainfo_b64 {
            use base64::Engine;
            let torrent_bytes = base64::engine::general_purpose::STANDARD.decode(meta_b64)
                .map_err(|e| anyhow::anyhow!("Failed to decode base64 metainfo: {}", e))?;

            let part = reqwest::multipart::Part::bytes(torrent_bytes)
                .file_name("release.torrent")
                .mime_str("application/x-bittorrent")?;

            let mut form = reqwest::multipart::Form::new()
                .part("torrents", part)
                .text("paused", paused_str.to_string());

            if let Some(dir) = download_dir {
                form = form.text("savepath", dir.to_string());
            }

            let resp = self.send_api_multipart("/api/v2/torrents/add", form).await?;
            let text = resp.text().await.unwrap_or_default();
            return Ok(json!({ "result": "success", "message": text }));
        }

        let mut params = Vec::new();
        if let Some(url) = magnet_or_url {
            params.push(("urls", url));
        }
        if let Some(dir) = download_dir {
            params.push(("savepath", dir));
        }
        params.push(("paused", paused_str));

        let resp = self.send_api_post("/api/v2/torrents/add", &params).await?;
        let text = resp.text().await.unwrap_or_default();
        Ok(json!({ "result": "success", "message": text }))
    }

    async fn start_torrents(&self, ids: &[i64], _now: bool) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        let joined = hashes.join("|");
        if let Err(e) = self.send_api_post("/api/v2/torrents/resume", &[("hashes", &joined)]).await {
            tracing::debug!("qBittorrent /resume failed ({}), attempting /start...", e);
            self.send_api_post("/api/v2/torrents/start", &[("hashes", &joined)]).await?;
        }
        Ok(())
    }

    async fn stop_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        let joined = hashes.join("|");
        if let Err(e) = self.send_api_post("/api/v2/torrents/pause", &[("hashes", &joined)]).await {
            tracing::debug!("qBittorrent /pause failed ({}), attempting /stop...", e);
            self.send_api_post("/api/v2/torrents/stop", &[("hashes", &joined)]).await?;
        }
        Ok(())
    }

    async fn verify_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        let joined = hashes.join("|");
        self.send_api_post("/api/v2/torrents/recheck", &[("hashes", &joined)]).await?;
        Ok(())
    }

    async fn reannounce_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        let joined = hashes.join("|");
        self.send_api_post("/api/v2/torrents/reannounce", &[("hashes", &joined)]).await?;
        Ok(())
    }

    async fn remove_torrents(&self, ids: &[i64], delete_local_data: bool) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        let joined = hashes.join("|");
        let del_str = if delete_local_data { "true" } else { "false" };
        self.send_api_post("/api/v2/torrents/delete", &[("hashes", &joined), ("deleteFiles", del_str)]).await?;
        Ok(())
    }

    async fn set_location(&self, ids: &[i64], location: &str, _move_data: bool) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        let joined = hashes.join("|");
        self.send_api_post("/api/v2/torrents/setLocation", &[("hashes", &joined), ("location", location)]).await?;
        Ok(())
    }

    async fn queue_move(&self, ids: &[i64], direction: &str) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        let joined = hashes.join("|");
        let endpoint = match direction.to_lowercase().as_str() {
            "top" => "/api/v2/torrents/topPrio",
            "up" => "/api/v2/torrents/increasePrio",
            "down" => "/api/v2/torrents/decreasePrio",
            "bottom" => "/api/v2/torrents/bottomPrio",
            other => return Err(anyhow::anyhow!("Invalid queue direction {}", other)),
        };
        self.send_api_post(endpoint, &[("hashes", &joined)]).await?;
        Ok(())
    }

    async fn set_sequential_download(&self, ids: &[i64], _enabled: bool) -> anyhow::Result<()> {
        let torrents = self.get_torrents(None).await?;
        let hashes: Vec<String> = torrents.into_iter().filter(|t| ids.contains(&t.id)).map(|t| t.hash_string).collect();
        if hashes.is_empty() {
            return Ok(());
        }
        let joined = hashes.join("|");
        self.send_api_post("/api/v2/torrents/toggleSequentialDownload", &[("hashes", &joined)]).await?;
        Ok(())
    }

    async fn rename_path(&self, id: i64, _path: &str, new_name: &str) -> anyhow::Result<Value> {
        let torrent = self.get_torrent_details(id).await?;
        self.send_api_post("/api/v2/torrents/rename", &[("hash", &torrent.hash_string), ("name", new_name)]).await?;
        Ok(json!({ "result": "success" }))
    }

    async fn set_turtle_mode(&self, _enabled: bool) -> anyhow::Result<()> {
        self.send_api_post("/api/v2/transfer/toggleSpeedLimitsMode", &[]).await?;
        Ok(())
    }

    async fn update_blocklist(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn get_free_space(&self, _path: &str) -> anyhow::Result<i64> {
        let resp = self.send_api_get("/api/v2/sync/maindata", &[]).await?;
        let json_val: Value = resp.json().await.unwrap_or_default();
        let free_bytes = json_val.get("server_state")
            .and_then(|s| s.get("free_space_on_disk"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        Ok(free_bytes)
    }

    async fn get_session(&self) -> anyhow::Result<Value> {
        let resp = self.send_api_get("/api/v2/app/preferences", &[]).await?;
        resp.json().await.map_err(|e| anyhow::anyhow!("Failed to parse qBittorrent preferences: {}", e))
    }

    async fn set_session(&self, settings: Value) -> anyhow::Result<()> {
        let json_str = serde_json::to_string(&settings)?;
        self.send_api_post("/api/v2/app/setPreferences", &[("json", &json_str)]).await?;
        Ok(())
    }

    async fn get_session_stats(&self) -> anyhow::Result<Value> {
        let resp = self.send_api_get("/api/v2/transfer/info", &[]).await?;
        resp.json().await.map_err(|e| anyhow::anyhow!("Failed to parse qBittorrent transfer info: {}", e))
    }

    async fn test_port(&self) -> anyhow::Result<bool> {
        Ok(true)
    }

    async fn replace_trackers(&self, id: i64, _tracker_list: &str, old_url: &str, new_url: &str) -> anyhow::Result<()> {
        let torrent = self.get_torrent_details(id).await?;
        let params = [
            ("hash", torrent.hash_string.as_str()),
            ("origUrl", old_url),
            ("newUrl", new_url),
        ];
        self.send_api_post("/api/v2/torrents/editTracker", &params).await?;
        Ok(())
    }
}

pub struct QBittorrentDriverFactory;

impl FetcherDriverFactory for QBittorrentDriverFactory {
    fn client_type(&self) -> RetrieverClientType {
        RetrieverClientType::QBittorrent
    }

    fn create_client(&self, config: FetcherNodeConfig) -> Arc<dyn TorrentClientTrait> {
        Arc::new(QBittorrentClient::new(config))
    }
}

