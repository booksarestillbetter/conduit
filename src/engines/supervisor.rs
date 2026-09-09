// src/engines/supervisor.rs
//! Keeps a background engine loop alive. Each engine loop is an infinite `async fn`
//! that is not expected to return; if it panics (a bug, an unexpected `.unwrap()`
//! on hostile input, etc.) Tokio would otherwise let that task die silently and
//! permanently with no operator-visible signal beyond the absence of future log
//! lines. This wraps the loop in `catch_unwind`, logs the failure loudly, and
//! restarts it with capped exponential backoff — resetting the backoff once the
//! engine has run cleanly for a while.

use super::registry::EngineRegistry;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use tracing::error;

const BASE_BACKOFF: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// If an engine ran at least this long before dying, treat the next restart as fresh
/// (reset backoff to the base) rather than continuing to ramp it up.
const HEALTHY_RUN_THRESHOLD: Duration = Duration::from_secs(300);

/// Spawns `make_future()` under supervision. `make_future` is called again every
/// time the previous run exits (by panic or by unexpected early return) to produce
/// a fresh future — this is why engines are handed a factory closure rather than a
/// single future: the captured state (ConfigManager/Database/etc.) must be re-cloned
/// per attempt. `registry` is updated on every crash/restart so the Platform Health UI
/// panel can see it; per-cycle heartbeats are reported separately by each engine loop
/// via `registry::TaskHandle` (see engines/registry.rs).
pub fn spawn_supervised<F, Fut>(name: &'static str, registry: EngineRegistry, make_future: F)
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    super::registry::ensure_registered(&registry, name);

    tokio::spawn(async move {
        let mut backoff = BASE_BACKOFF;
        loop {
            let started = Instant::now();
            let result = AssertUnwindSafe(make_future()).catch_unwind().await;
            let ran_for = started.elapsed();

            let reason = match result {
                Ok(()) => "returned unexpectedly".to_string(),
                Err(panic_payload) => {
                    let msg = panic_payload
                        .downcast_ref::<&str>()
                        .map(|s| s.to_string())
                        .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "non-string panic payload".to_string());
                    format!("panicked: {}", msg)
                }
            };

            super::registry::mark_crashed(&registry, name, format!("{} after running for {:?}", reason, ran_for));

            if ran_for >= HEALTHY_RUN_THRESHOLD {
                backoff = BASE_BACKOFF;
            }

            error!(
                "⚠️ Background engine '{}' {} after running for {:?}. Restarting in {:?}.",
                name, reason, ran_for, backoff
            );

            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    });
}
