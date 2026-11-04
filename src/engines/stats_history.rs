// src/engines/stats_history.rs
//! Persists periodic points backing the Stats page's historical trend charts. Two cadences on
//! one supervised loop, same tick-counting style as `engines::poller`: the cheap aggregate
//! snapshot (`stats::compute_stats_snapshot`, no extra network cost -- it just reuses what the
//! poller already cached) every 5 minutes, and the far more expensive peer-client sample
//! (`stats::sample_peer_clients`, one detail RPC per sampled torrent) every 15 minutes.

use super::registry::{heartbeat_sleep, TaskHandle};
use crate::db::Database;
use crate::fetcher::FetcherPool;
use crate::stats::{self, DEFAULT_PEER_CLIENT_SAMPLE_SIZE};
use std::sync::Arc;
use std::time::Duration;

const TICK_INTERVAL_SECS: u64 = 60;
const STATS_SNAPSHOT_EVERY_TICKS: u64 = 5;
const PEER_CLIENT_SAMPLE_EVERY_TICKS: u64 = 15;

pub async fn run_stats_history_loop(pool: Arc<FetcherPool>, db: Database, handle: TaskHandle) {
    let mut tick_count = 0u64;

    loop {
        tick_count += 1;

        if tick_count.is_multiple_of(STATS_SNAPSHOT_EVERY_TICKS) {
            let snapshot = stats::compute_stats_snapshot(&pool, &db);
            let _ = db.insert_stats_snapshot(&snapshot);
        }

        if tick_count.is_multiple_of(PEER_CLIENT_SAMPLE_EVERY_TICKS) {
            let snapshot = stats::sample_peer_clients(&pool, DEFAULT_PEER_CLIENT_SAMPLE_SIZE).await;
            let _ = db.insert_peer_client_snapshot(&snapshot);
        }

        heartbeat_sleep(&handle, Duration::from_secs(TICK_INTERVAL_SECS)).await;
    }
}
