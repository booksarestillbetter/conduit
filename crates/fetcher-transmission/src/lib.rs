use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use fetcher_core::{
    FetcherDriverFactory, FetcherNodeConfig, RetrieverClientType, Torrent, TorrentClientTrait,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::debug;

#[derive(Debug, Clone)]
pub struct TransmissionClient {
    config: FetcherNodeConfig,
    http: reqwest::Client,
    session_id: Arc<RwLock<Option<String>>>,
    url: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct RpcRequest {
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct RpcResponse {
    result: String,
    arguments: Option<Value>,
    tag: Option<i64>,
}

impl TransmissionClient {
    pub fn new(config: FetcherNodeConfig) -> Self {
        let scheme = if config.use_ssl { "https" } else { "http" };
        let rpc_path = if config.rpc_path.starts_with('/') {
            config.rpc_path.clone()
        } else {
            format!("/{}", config.rpc_path)
        };
        let url = format!("{}://{}:{}{}", scheme, config.host, config.port, rpc_path);

        let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(15));
        if !config.verify_tls {
            builder = builder.danger_accept_invalid_certs(true);
        }
        let http = builder.build().unwrap_or_default();

        Self {
            config,
            http,
            session_id: Arc::new(RwLock::new(None)),
            url,
        }
    }

    pub fn node_name(&self) -> &str {
        &self.config.name
    }

    pub fn config(&self) -> &FetcherNodeConfig {
        &self.config
    }

    pub async fn send_rpc(&self, method: &str, arguments: Option<Value>) -> anyhow::Result<Value> {
        let req_body = if let Some(ref args) = arguments {
            json!({
                "method": method,
                "arguments": args,
                "tag": 1,
            })
        } else {
            json!({
                "method": method,
                "tag": 1,
            })
        };

        for _attempt in 0..2 {
            let mut req = self.http.post(&self.url);

            if let (Some(u), Some(p)) = (&self.config.username, &self.config.password) {
                req = req.basic_auth(u, Some(p));
            }

            if let Some(ref sid) = *self.session_id.read() {
                req = req.header("X-Transmission-Session-Id", sid);
            }

            let resp = req.json(&req_body).send().await?;

            if resp.status().as_u16() == 409 {
                let new_sid = resp
                    .headers()
                    .get("X-Transmission-Session-Id")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string());
                if let Some(new_sid) = new_sid {
                    *self.session_id.write() = Some(new_sid);
                    debug!("Acquired new Transmission session ID for {}", self.config.name);
                    let _ = resp.bytes().await;
                    continue;
                }
            }

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!("Transmission RPC error (HTTP {}): {}", status, text));
            }

            let val: Value = resp.json().await?;

            if let Some(err_obj) = val.get("error") {
                let msg = err_obj.get("message").and_then(|m| m.as_str()).unwrap_or("JSON-RPC 2.0 error");
                return Err(anyhow::anyhow!("Transmission RPC error: {}", msg));
            }

            if let Some(res_str) = val.get("result").and_then(|r| r.as_str()) {
                if res_str != "success" {
                    return Err(anyhow::anyhow!("Transmission RPC returned failure: {}", res_str));
                }
            }

            if let Some(args) = val.get("arguments") {
                if !args.is_null() {
                    return Ok(args.clone());
                }
            }
            if let Some(res_obj) = val.get("result") {
                if res_obj.is_object() {
                    return Ok(res_obj.clone());
                }
            }

            return Ok(val);
        }

        Err(anyhow::anyhow!("Failed Transmission RPC request after 409 re-auth"))
    }

    pub async fn get_torrents(&self, ids: Option<Vec<i64>>) -> anyhow::Result<Vec<Torrent>> {
        let fields = vec![
            "id", "name", "hashString", "status", "rateUpload", "rateDownload",
            "uploadedEver", "downloadedEver", "uploadRatio", "totalSize",
            "sizeWhenDone", "leftUntilDone", "percentDone", "eta", "etaIdle",
            "error", "errorString", "peersConnected", "peersSendingToUs",
            "peersGettingFromUs", "addedDate", "doneDate", "downloadDir",
            "trackerStats", "queuePosition",
        ];

        let mut args = json!({ "fields": fields });
        if let Some(ids_list) = ids {
            args["ids"] = json!(ids_list);
        }

        let res = self.send_rpc("torrent-get", Some(args)).await?;
        let torrents: Vec<Torrent> = serde_json::from_value(res["torrents"].clone())
            .map_err(|e| anyhow::anyhow!("Failed to parse torrent-get response from node {}: {}", self.config.name, e))?;
        Ok(torrents)
    }

    pub async fn get_torrent_details(&self, id: i64) -> anyhow::Result<Torrent> {
        let fields = vec![
            "id", "name", "hashString", "status", "rateUpload", "rateDownload",
            "uploadedEver", "downloadedEver", "uploadRatio", "totalSize",
            "sizeWhenDone", "leftUntilDone", "percentDone", "eta", "etaIdle",
            "error", "errorString", "peersConnected", "peersSendingToUs",
            "peersGettingFromUs", "addedDate", "doneDate", "downloadDir",
            "trackerStats", "files", "peers", "comment", "creator", "dateCreated",
            "pieceCount", "pieceSize", "isPrivate", "magnetLink", "corruptEver",
            "secondsDownloading", "secondsSeeding", "activityDate",
            "queuePosition", "pieces",
        ];

        let args = json!({
            "ids": [id],
            "fields": fields,
        });

        let res = self.send_rpc("torrent-get", Some(args)).await?;
        let torrents: Vec<Torrent> = serde_json::from_value(res["torrents"].clone())
            .map_err(|e| anyhow::anyhow!("Failed to parse torrent-get response from node {}: {}", self.config.name, e))?;
        torrents.into_iter().next().ok_or_else(|| anyhow::anyhow!("Torrent not found on node {}", self.config.name))
    }

    pub async fn add_torrent(
        &self,
        magnet_or_url: Option<&str>,
        metainfo_b64: Option<&str>,
        download_dir: Option<&str>,
        paused: bool,
    ) -> anyhow::Result<Value> {
        let mut args = json!({ "paused": paused });
        if let Some(filename) = magnet_or_url {
            args["filename"] = json!(filename);
        }
        if let Some(meta) = metainfo_b64 {
            args["metainfo"] = json!(meta);
        }
        if let Some(dir) = download_dir {
            args["download-dir"] = json!(dir);
        }

        self.send_rpc("torrent-add", Some(args)).await
    }

    pub async fn start_torrents(&self, ids: &[i64], now: bool) -> anyhow::Result<()> {
        let method = if now { "torrent-start-now" } else { "torrent-start" };
        let args = json!({ "ids": ids });
        self.send_rpc(method, Some(args)).await?;
        Ok(())
    }

    pub async fn stop_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let args = json!({ "ids": ids });
        self.send_rpc("torrent-stop", Some(args)).await?;
        Ok(())
    }

    pub async fn verify_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let args = json!({ "ids": ids });
        self.send_rpc("torrent-verify", Some(args)).await?;
        Ok(())
    }

    pub async fn reannounce_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        let args = json!({ "ids": ids });
        self.send_rpc("torrent-reannounce", Some(args)).await?;
        Ok(())
    }

    pub async fn remove_torrents(&self, ids: &[i64], delete_local_data: bool) -> anyhow::Result<()> {
        let args = json!({
            "ids": ids,
            "delete-local-data": delete_local_data,
        });
        self.send_rpc("torrent-remove", Some(args)).await?;
        Ok(())
    }

    pub async fn set_location(&self, ids: &[i64], location: &str, move_data: bool) -> anyhow::Result<()> {
        let args = json!({
            "ids": ids,
            "location": location,
            "move": move_data,
        });
        self.send_rpc("torrent-set-location", Some(args)).await?;
        Ok(())
    }

    pub async fn queue_move(&self, ids: &[i64], direction: &str) -> anyhow::Result<()> {
        let method = match direction.to_lowercase().as_str() {
            "top" => "queue-move-top",
            "up" => "queue-move-up",
            "down" => "queue-move-down",
            "bottom" => "queue-move-bottom",
            other => return Err(anyhow::anyhow!("Invalid queue move direction '{}'", other)),
        };
        let args = json!({ "ids": ids });
        self.send_rpc(method, Some(args)).await?;
        Ok(())
    }

    pub async fn set_sequential_download(&self, ids: &[i64], _enabled: bool) -> anyhow::Result<()> {
        let _ = ids;
        Ok(())
    }

    pub async fn rename_path(&self, id: i64, path: &str, new_name: &str) -> anyhow::Result<Value> {
        let args = json!({
            "ids": [id],
            "path": path,
            "name": new_name,
        });
        self.send_rpc("torrent-rename-path", Some(args)).await
    }

    pub async fn set_turtle_mode(&self, enabled: bool) -> anyhow::Result<()> {
        let args = json!({
            "alt-speed-enabled": enabled,
        });
        self.send_rpc("session-set", Some(args)).await?;
        Ok(())
    }

    pub async fn update_blocklist(&self) -> anyhow::Result<i64> {
        let res = self.send_rpc("blocklist-update", None).await?;
        let size = res.get("blocklist-size")
            .or_else(|| res.get("blocklistSize"))
            .or_else(|| res.get("blocklist_size"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        Ok(size)
    }

    pub async fn get_free_space(&self, path: &str) -> anyhow::Result<i64> {
        let args = json!({ "path": path });
        let res = self.send_rpc("free-space", Some(args)).await?;
        let bytes = res.get("size-bytes")
            .or_else(|| res.get("sizeBytes"))
            .or_else(|| res.get("size_bytes"))
            .or_else(|| res.get("free_space"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        Ok(bytes)
    }

    pub async fn get_session(&self) -> anyhow::Result<Value> {
        self.send_rpc("session-get", None).await
    }

    pub async fn set_session(&self, settings: Value) -> anyhow::Result<()> {
        self.send_rpc("session-set", Some(settings)).await?;
        Ok(())
    }

    pub async fn get_session_stats(&self) -> anyhow::Result<Value> {
        self.send_rpc("session-stats", None).await
    }

    pub async fn test_port(&self) -> anyhow::Result<bool> {
        let res = self.send_rpc("port-test", None).await?;
        Ok(res["port-is-open"].as_bool().unwrap_or(false))
    }
}

#[derive(Debug, Clone)]
pub struct TransmissionAdapter {
    inner: TransmissionClient,
}

impl TransmissionAdapter {
    pub fn new(config: FetcherNodeConfig) -> Self {
        Self {
            inner: TransmissionClient::new(config),
        }
    }
}

#[async_trait]
impl TorrentClientTrait for TransmissionAdapter {
    fn node_name(&self) -> &str {
        self.inner.node_name()
    }

    fn config(&self) -> &FetcherNodeConfig {
        self.inner.config()
    }

    fn client_type(&self) -> RetrieverClientType {
        RetrieverClientType::Transmission
    }

    async fn get_torrents(&self, ids: Option<Vec<i64>>) -> anyhow::Result<Vec<Torrent>> {
        self.inner.get_torrents(ids).await
    }

    async fn get_torrent_details(&self, id: i64) -> anyhow::Result<Torrent> {
        self.inner.get_torrent_details(id).await
    }

    async fn get_torrent_details_by_hash(&self, hash: &str) -> anyhow::Result<Torrent> {
        let torrents = self.inner.get_torrents(None).await?;
        torrents
            .into_iter()
            .find(|t| t.hash_string.eq_ignore_ascii_case(hash))
            .ok_or_else(|| anyhow::anyhow!("Torrent with hash {} not found on node {}", hash, self.inner.node_name()))
    }

    async fn add_torrent(
        &self,
        magnet_or_url: Option<&str>,
        metainfo_b64: Option<&str>,
        download_dir: Option<&str>,
        paused: bool,
    ) -> anyhow::Result<Value> {
        self.inner.add_torrent(magnet_or_url, metainfo_b64, download_dir, paused).await
    }

    async fn start_torrents(&self, ids: &[i64], now: bool) -> anyhow::Result<()> {
        self.inner.start_torrents(ids, now).await
    }

    async fn stop_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        self.inner.stop_torrents(ids).await
    }

    async fn verify_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        self.inner.verify_torrents(ids).await
    }

    async fn reannounce_torrents(&self, ids: &[i64]) -> anyhow::Result<()> {
        self.inner.reannounce_torrents(ids).await
    }

    async fn remove_torrents(&self, ids: &[i64], delete_local_data: bool) -> anyhow::Result<()> {
        self.inner.remove_torrents(ids, delete_local_data).await
    }

    async fn set_location(&self, ids: &[i64], location: &str, move_data: bool) -> anyhow::Result<()> {
        self.inner.set_location(ids, location, move_data).await
    }

    async fn queue_move(&self, ids: &[i64], direction: &str) -> anyhow::Result<()> {
        self.inner.queue_move(ids, direction).await
    }

    async fn set_sequential_download(&self, ids: &[i64], enabled: bool) -> anyhow::Result<()> {
        self.inner.set_sequential_download(ids, enabled).await
    }

    async fn rename_path(&self, id: i64, path: &str, new_name: &str) -> anyhow::Result<Value> {
        self.inner.rename_path(id, path, new_name).await
    }

    async fn set_turtle_mode(&self, enabled: bool) -> anyhow::Result<()> {
        self.inner.set_turtle_mode(enabled).await
    }

    async fn update_blocklist(&self) -> anyhow::Result<i64> {
        self.inner.update_blocklist().await
    }

    async fn get_free_space(&self, path: &str) -> anyhow::Result<i64> {
        self.inner.get_free_space(path).await
    }

    async fn get_session(&self) -> anyhow::Result<Value> {
        self.inner.get_session().await
    }

    async fn set_session(&self, settings: Value) -> anyhow::Result<()> {
        self.inner.set_session(settings).await
    }

    async fn get_session_stats(&self) -> anyhow::Result<Value> {
        self.inner.get_session_stats().await
    }

    async fn test_port(&self) -> anyhow::Result<bool> {
        self.inner.test_port().await
    }

    async fn replace_trackers(&self, _id: i64, _tracker_list: &str, _old_url: &str, _new_url: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct TransmissionDriverFactory;

impl FetcherDriverFactory for TransmissionDriverFactory {
    fn client_type(&self) -> RetrieverClientType {
        RetrieverClientType::Transmission
    }

    fn create_client(&self, config: FetcherNodeConfig) -> Arc<dyn TorrentClientTrait> {
        Arc::new(TransmissionAdapter::new(config))
    }
}
