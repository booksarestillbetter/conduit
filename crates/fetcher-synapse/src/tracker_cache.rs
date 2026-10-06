//! Per-torrent tracker cache for the list view.
//!
//! Synapse's `TorrentSummary` stream carries no tracker data — trackers only arrive with the
//! per-torrent detail stream. Conduit's tracker sidebar/filter is built from each torrent's
//! `tracker_stats` in the *list*, so without this a Synapse node's torrents report no trackers
//! and its tracker list vanishes. This task fills the gap by fetching each torrent's trackers
//! once through the existing detail RPC (a few per cycle, so a big library never bursts) and
//! refreshing them occasionally. The proto is deliberately untouched — it's vendored from a
//! separate service.

use crate::client::SynapseClient;
use crate::live_cache::SynapseLiveCache;
use crate::proto::v2::TrackerStatus;
use fetcher_core::TrackerStat;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

const CYCLE: Duration = Duration::from_secs(3);
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_FETCHES_PER_CYCLE: usize = 25;
const REFRESH_AFTER: Duration = Duration::from_secs(600);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(60);

struct Entry {
    stats: Vec<TrackerStat>,
    fetched_at: Instant,
    ok: bool,
}

type Map = RwLock<HashMap<String, Entry>>;

struct AbortOnDrop(JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Clone)]
pub struct TrackerCache {
    entries: Arc<Map>,
    _task: Arc<AbortOnDrop>,
}

impl TrackerCache {
    pub fn spawn(client: SynapseClient, torrents: SynapseLiveCache) -> Self {
        let entries: Arc<Map> = Arc::new(RwLock::new(HashMap::new()));
        let weak = Arc::downgrade(&entries);
        let handle = tokio::spawn(async move { run(client, torrents, weak).await });
        Self { entries, _task: Arc::new(AbortOnDrop(handle)) }
    }

    /// Cached trackers for a torrent; empty until the first fetch for it completes.
    pub fn get(&self, hash: &str) -> Vec<TrackerStat> {
        self.entries.read().get(hash).map(|e| e.stats.clone()).unwrap_or_default()
    }
}

/// Maps Synapse tracker statuses to Conduit's `TrackerStat` (shared with the detail view).
pub fn tracker_stats_from_status(trackers: Vec<TrackerStatus>) -> Vec<TrackerStat> {
    trackers
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
        .collect()
}

/// List-view trackers built from the host list Synapse sends in `TorrentSummary.tracker_hosts`.
/// Host-only on purpose (no announce URL, no per-announce results) — the full status still
/// comes from the detail view. Enough for grouping and filtering by tracker.
pub fn tracker_stats_from_hosts(hosts: &[String]) -> Vec<TrackerStat> {
    hosts
        .iter()
        .map(|h| TrackerStat {
            announce: h.clone(),
            host: h.clone(),
            seeder_count: 0,
            leecher_count: 0,
            download_count: 0,
            last_announce_succeeded: true,
            last_announce_result: String::new(),
            is_backup: false,
        })
        .collect()
}

async fn fetch(client: &SynapseClient, hash: &str) -> Option<Vec<TrackerStat>> {
    let mut stream = tokio::time::timeout(FETCH_TIMEOUT, client.subscribe_torrent_detail(hash, 0))
        .await
        .ok()?
        .ok()?;
    let event = tokio::time::timeout(FETCH_TIMEOUT, stream.message()).await.ok()?.ok()??;
    Some(tracker_stats_from_status(event.trackers))
}

async fn run(client: SynapseClient, torrents: SynapseLiveCache, entries: Weak<Map>) {
    loop {
        tokio::time::sleep(CYCLE).await;
        // The owning client was dropped (node removed / reconfigured) — stop.
        let Some(entries) = entries.upgrade() else { return };
        if !torrents.is_connected() {
            continue;
        }

        // Torrents whose summary already carries tracker hosts (current Synapse) need no fetch.
        let all = torrents.list_torrents();
        let present: Vec<String> = all.iter().map(|t| t.hash.clone()).collect();
        let needs_fetch: std::collections::HashSet<&str> =
            all.iter().filter(|t| t.tracker_hosts.is_empty()).map(|t| t.hash.as_str()).collect();
        let now = Instant::now();
        let due: Vec<String> = {
            let mut map = entries.write();
            map.retain(|h, _| present.contains(h));
            present
                .into_iter()
                .filter(|h| needs_fetch.contains(h.as_str()))
                .filter(|h| match map.get(h) {
                    None => true,
                    Some(e) => now.duration_since(e.fetched_at) >= if e.ok { REFRESH_AFTER } else { RETRY_AFTER_FAILURE },
                })
                .take(MAX_FETCHES_PER_CYCLE)
                .collect()
        };

        for hash in due {
            let result = fetch(&client, &hash).await;
            let mut map = entries.write();
            match result {
                Some(stats) => {
                    map.insert(hash, Entry { stats, fetched_at: Instant::now(), ok: true });
                }
                None => {
                    // Keep any previously known trackers; just back off before retrying.
                    let stats = map.get(&hash).map(|e| e.stats.clone()).unwrap_or_default();
                    map.insert(hash, Entry { stats, fetched_at: Instant::now(), ok: false });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_list_becomes_one_tracker_per_host() {
        let stats = tracker_stats_from_hosts(&["tracker.a.org".to_string(), "udp.b.net:6969".to_string()]);
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].host, "tracker.a.org");
        assert_eq!(stats[1].host, "udp.b.net:6969");
        assert!(tracker_stats_from_hosts(&[]).is_empty());
    }
}
