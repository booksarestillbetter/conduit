use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use async_trait::async_trait;
use fetcher_core::{
    FetcherDriverFactory, FetcherNodeConfig, RetrieverClientType, Torrent, TorrentClientTrait,
};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::debug;

/// Ordinary RPC calls (add, pause, set-location, ...) should answer quickly.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
/// A whole-library `torrent-get` on a busy or disk-bound daemon can legitimately take a
/// long time. Transmission serves RPC from its single main loop, so under I/O pressure this
/// is the call that stalls first; giving it room avoids reporting a merely slow daemon as
/// unreachable.
const LIST_TIMEOUT: Duration = Duration::from_secs(90);
/// A delta poll (`ids: "recently-active"`) only covers changes in Transmission's trailing
/// 60 s window. If we haven't heard from the daemon for longer than this, we might have
/// missed changes, so the next poll must be a full one.
const DELTA_MAX_GAP: Duration = Duration::from_secs(40);
/// Safety net: even when every delta succeeds, re-read the full list this often.
const FULL_REFRESH_EVERY: Duration = Duration::from_secs(300);

/// Client-side mirror of the daemon's torrent list, kept current with delta polls.
#[derive(Debug, Default)]
struct ListCache {
    torrents: HashMap<i64, Torrent>,
    last_ok: Option<Instant>,
    last_full: Option<Instant>,
    /// Cleared if the daemon rejects `recently-active` (very old Transmission).
    delta_unsupported: bool,
}

#[derive(Debug, Clone)]
pub struct TransmissionClient {
    config: FetcherNodeConfig,
    http: reqwest::Client,
    session_id: Arc<RwLock<Option<String>>>,
    url: String,
    list_cache: Arc<Mutex<ListCache>>,
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

        let mut builder = reqwest::Client::builder().timeout(RPC_TIMEOUT);
        if !config.verify_tls {
            builder = builder.danger_accept_invalid_certs(true);
        }
        let http = builder.build().unwrap_or_default();

        Self {
            config,
            http,
            session_id: Arc::new(RwLock::new(None)),
            url,
            list_cache: Arc::new(Mutex::new(ListCache::default())),
        }
    }

    pub fn node_name(&self) -> &str {
        &self.config.name
    }

    pub fn config(&self) -> &FetcherNodeConfig {
        &self.config
    }

    pub async fn send_rpc(&self, method: &str, arguments: Option<Value>) -> anyhow::Result<Value> {
        self.send_rpc_with_timeout(method, arguments, None).await
    }

    pub async fn send_rpc_with_timeout(
        &self,
        method: &str,
        arguments: Option<Value>,
        timeout: Option<Duration>,
    ) -> anyhow::Result<Value> {
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
            if let Some(t) = timeout {
                req = req.timeout(t);
            }

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

    fn list_fields() -> Vec<&'static str> {
        vec![
            "id", "name", "hashString", "status", "rateUpload", "rateDownload",
            "uploadedEver", "downloadedEver", "uploadRatio", "totalSize",
            "sizeWhenDone", "leftUntilDone", "percentDone", "eta", "etaIdle",
            "error", "errorString", "peersConnected", "peersSendingToUs",
            "peersGettingFromUs", "addedDate", "doneDate", "downloadDir",
            "trackerStats", "queuePosition",
        ]
    }

    fn parse_torrents(&self, value: &Value) -> anyhow::Result<Vec<Torrent>> {
        serde_json::from_value(value.clone())
            .map_err(|e| anyhow::anyhow!("Failed to parse torrent-get response from node {}: {}", self.config.name, e))
    }

    /// List torrents. With `ids: None` this returns the whole library, but after the first
    /// full read it only asks the daemon for what changed (`recently-active`) and merges
    /// that into a local copy, so a big library on a slow daemon costs a handful of rows per
    /// poll instead of a full re-serialisation every couple of seconds.
    pub async fn get_torrents(&self, ids: Option<Vec<i64>>) -> anyhow::Result<Vec<Torrent>> {
        if let Some(ids_list) = ids {
            let args = json!({ "fields": Self::list_fields(), "ids": ids_list });
            let res = self.send_rpc("torrent-get", Some(args)).await?;
            return self.parse_torrents(&res["torrents"]);
        }

        let use_delta = {
            let cache = self.list_cache.lock();
            !cache.delta_unsupported
                && cache.last_ok.is_some_and(|t| t.elapsed() < DELTA_MAX_GAP)
                && cache.last_full.is_some_and(|t| t.elapsed() < FULL_REFRESH_EVERY)
        };

        if use_delta {
            let args = json!({ "fields": Self::list_fields(), "ids": "recently-active" });
            match self.send_rpc_with_timeout("torrent-get", Some(args), Some(LIST_TIMEOUT)).await {
                Ok(res) => {
                    let changed = self.parse_torrents(&res["torrents"])?;
                    let removed: Vec<i64> = res
                        .get("removed")
                        .and_then(|r| r.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
                        .unwrap_or_default();
                    let mut cache = self.list_cache.lock();
                    for id in removed {
                        cache.torrents.remove(&id);
                    }
                    for t in changed {
                        cache.torrents.insert(t.id, t);
                    }
                    cache.last_ok = Some(Instant::now());
                    return Ok(cache.torrents.values().cloned().collect());
                }
                Err(e) if e.downcast_ref::<reqwest::Error>().is_some() => {
                    // Network trouble: don't trust the mirror, and don't pile a full read on
                    // top of a struggling daemon within the same poll.
                    self.list_cache.lock().last_ok = None;
                    return Err(e);
                }
                Err(e) => {
                    debug!("{} rejected recently-active ({}); using full reads", self.config.name, e);
                    self.list_cache.lock().delta_unsupported = true;
                }
            }
        }

        let args = json!({ "fields": Self::list_fields() });
        let res = self.send_rpc_with_timeout("torrent-get", Some(args), Some(LIST_TIMEOUT)).await;
        let res = match res {
            Ok(r) => r,
            Err(e) => {
                self.list_cache.lock().last_ok = None;
                return Err(e);
            }
        };
        let torrents = self.parse_torrents(&res["torrents"])?;
        let mut cache = self.list_cache.lock();
        cache.torrents = torrents.iter().map(|t| (t.id, t.clone())).collect();
        cache.last_ok = Some(Instant::now());
        cache.last_full = Some(Instant::now());
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

    /// `sequentialDownload` is a Transmission 4.1.0+ (`rpc-version-semver` 6.0.0+) `torrent-set`
    /// argument. Transmission's RPC server ignores unrecognized `torrent-set` arguments rather
    /// than erroring, so this is a safe no-op against an older daemon rather than a failure --
    /// there is no reliable way to distinguish "ignored" from "applied" without also comparing
    /// `torrent-get`'s echoed `sequentialDownload` field, which isn't worth the extra round-trip
    /// for a best-effort toggle.
    pub async fn set_sequential_download(&self, ids: &[i64], enabled: bool) -> anyhow::Result<()> {
        let args = json!({
            "ids": ids,
            "sequentialDownload": enabled,
        });
        self.send_rpc("torrent-set", Some(args)).await?;
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

    /// Uses `trackerList` (Transmission 4.0.0+, `rpc-version-semver` 5.3.0+), the non-deprecated
    /// full-announce-list replacement mechanism -- `trackerAdd`/`trackerRemove`/`trackerReplace`
    /// are all deprecated in favor of it. `tracker_list` is the caller's already-computed,
    /// post-replacement announce list (newline-separated, blank line between tiers), so this is
    /// a direct `torrent-set` call with no extra lookup needed. Ignored (not an error) by an
    /// older daemon, same caveat as `set_sequential_download` above.
    async fn replace_trackers(&self, id: i64, tracker_list: &str, _old_url: &str, _new_url: &str) -> anyhow::Result<()> {
        let args = json!({
            "ids": [id],
            "trackerList": tracker_list,
        });
        self.inner.send_rpc("torrent-set", Some(args)).await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Minimal scripted Transmission RPC endpoint: answers each request with the next
    /// canned JSON body and records the request bodies it saw.
    async fn mock(replies: Vec<Value>) -> (u16, Arc<Mutex<Vec<Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        tokio::spawn(async move {
            let mut replies = replies.into_iter();
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let body_start = loop {
                    let n = sock.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        break None;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break Some(i + 4);
                    }
                };
                let Some(start) = body_start else { continue };
                let head = String::from_utf8_lossy(&buf[..start]).to_ascii_lowercase();
                let len: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
                while buf.len() < start + len {
                    let n = sock.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                seen2.lock().push(serde_json::from_slice(&buf[start..start + len]).unwrap());
                let reply = replies.next().unwrap_or_else(|| json!({"result":"success","arguments":{"torrents":[]}}));
                let body = reply.to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });
        (port, seen)
    }

    fn client(port: u16) -> TransmissionClient {
        let cfg: FetcherNodeConfig = serde_json::from_value(json!({
            "name": "t", "client_type": "transmission", "host": "127.0.0.1", "port": port,
            "rpc_path": "/transmission/rpc", "enabled": true
        }))
        .unwrap();
        TransmissionClient::new(cfg)
    }

    fn row(id: i64, name: &str) -> Value {
        json!({"id": id, "name": name, "hashString": format!("{id:040}")})
    }

    #[tokio::test]
    async fn after_a_full_read_polls_only_ask_for_what_changed() {
        let (port, seen) = mock(vec![
            json!({"result":"success","arguments":{"torrents":[row(1,"a"),row(2,"b"),row(3,"c")]}}),
            json!({"result":"success","arguments":{"torrents":[row(2,"b2"),row(4,"d")],"removed":[3]}}),
        ])
        .await;
        let c = client(port);

        let full = c.get_torrents(None).await.unwrap();
        assert_eq!(full.len(), 3);
        let merged = c.get_torrents(None).await.unwrap();

        let mut names: Vec<_> = merged.iter().map(|t| t.name.clone()).collect();
        names.sort();
        assert_eq!(names, ["a", "b2", "d"], "changed rows replace, new rows join, removed rows leave");

        let seen = seen.lock();
        assert!(seen[0]["arguments"].get("ids").is_none(), "first read is a full read");
        assert_eq!(seen[1]["arguments"]["ids"], "recently-active");
    }

    #[tokio::test]
    async fn a_daemon_that_rejects_recently_active_falls_back_to_full_reads() {
        let (port, seen) = mock(vec![
            json!({"result":"success","arguments":{"torrents":[row(1,"a")]}}),
            json!({"result":"invalid argument"}),
            json!({"result":"success","arguments":{"torrents":[row(1,"a"),row(2,"b")]}}),
            json!({"result":"success","arguments":{"torrents":[row(1,"a"),row(2,"b"),row(3,"c")]}}),
        ])
        .await;
        let c = client(port);
        assert_eq!(c.get_torrents(None).await.unwrap().len(), 1);
        assert_eq!(c.get_torrents(None).await.unwrap().len(), 2);
        assert_eq!(c.get_torrents(None).await.unwrap().len(), 3);

        let seen = seen.lock();
        assert_eq!(seen[1]["arguments"]["ids"], "recently-active");
        assert!(seen[2]["arguments"].get("ids").is_none(), "retried as a full read");
        assert!(seen[3]["arguments"].get("ids").is_none(), "and stays on full reads");
    }

    #[tokio::test]
    async fn a_stale_mirror_is_never_trusted() {
        let (port, seen) = mock(vec![
            json!({"result":"success","arguments":{"torrents":[row(1,"a")]}}),
            json!({"result":"success","arguments":{"torrents":[row(1,"a"),row(2,"b")]}}),
        ])
        .await;
        let c = client(port);
        c.get_torrents(None).await.unwrap();
        // Simulate a long silence: changes in the gap would be outside the 60 s window.
        c.list_cache.lock().last_ok = Some(Instant::now() - DELTA_MAX_GAP - Duration::from_secs(1));
        assert_eq!(c.get_torrents(None).await.unwrap().len(), 2);
        assert!(seen.lock()[1]["arguments"].get("ids").is_none());
    }
}
