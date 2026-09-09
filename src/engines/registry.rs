// src/engines/registry.rs
//! Lightweight visibility into every background engine — "is it alive, when did it last
//! complete a cycle, has it crashed and been restarted." This is deliberately coarse (a
//! per-cycle heartbeat plus crash/restart tracking from the supervisor), not a full metrics
//! system: it exists so the "Platform Health" UI panel can answer "is anything stuck or
//! crash-looping" at a glance, matching the visibility Sonarr/Radarr/node connectivity already
//! has via `FetcherPool::get_system_health`.

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EngineState {
    /// Registered but hasn't completed its first cycle yet.
    Starting,
    /// Most recent cycle completed without error.
    Running,
    /// Most recent cycle completed but reported an error (e.g. a poll that failed).
    Degraded,
    /// Panicked; the supervisor is restarting it with backoff.
    Crashed,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EngineStatus {
    pub name: String,
    pub state: EngineState,
    pub last_tick_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub tick_count: u64,
    pub restart_count: u32,
}

pub type EngineRegistry = Arc<RwLock<HashMap<String, EngineStatus>>>;

pub fn new_registry() -> EngineRegistry {
    Arc::new(RwLock::new(HashMap::new()))
}

pub fn snapshot(registry: &EngineRegistry) -> Vec<EngineStatus> {
    let reg = registry.read();
    let mut list: Vec<EngineStatus> = reg.values().cloned().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

/// Ensures `name` is listed (state `Starting`) even before its first tick or any restart —
/// called once by `spawn_supervised` right before spawning, so the registry always reflects
/// every engine the process intends to run, not just the ones that have ticked at least once.
pub fn ensure_registered(registry: &EngineRegistry, name: &str) {
    let mut reg = registry.write();
    reg.entry(name.to_string()).or_insert_with(|| EngineStatus {
        name: name.to_string(),
        state: EngineState::Starting,
        last_tick_at: None,
        last_error: None,
        tick_count: 0,
        restart_count: 0,
    });
}

pub fn mark_crashed(registry: &EngineRegistry, name: &str, err: String) {
    let mut reg = registry.write();
    if let Some(status) = reg.get_mut(name) {
        status.state = EngineState::Crashed;
        status.last_error = Some(err);
        status.restart_count += 1;
    }
}

/// Handle given to each engine loop so it can report a completed cycle. Cheap to clone
/// (an `Arc<RwLock<..>>` + a name), re-created per supervised attempt the same way the other
/// captured state (ConfigManager/Database/etc.) is re-cloned per attempt.
#[derive(Clone)]
pub struct TaskHandle {
    registry: EngineRegistry,
    name: &'static str,
}

impl TaskHandle {
    pub fn new(registry: EngineRegistry, name: &'static str) -> Self {
        ensure_registered(&registry, name);
        Self { registry, name }
    }

    pub fn tick(&self) {
        let mut reg = self.registry.write();
        if let Some(status) = reg.get_mut(self.name) {
            status.state = EngineState::Running;
            status.last_tick_at = Some(Utc::now());
            status.tick_count += 1;
            status.last_error = None;
        }
    }

    pub fn tick_err(&self, err: impl Into<String>) {
        let mut reg = self.registry.write();
        if let Some(status) = reg.get_mut(self.name) {
            status.state = EngineState::Degraded;
            status.last_tick_at = Some(Utc::now());
            status.tick_count += 1;
            status.last_error = Some(err.into());
        }
    }
}

/// Marks one heartbeat then sleeps — a drop-in replacement for a bare
/// `tokio::time::sleep(dur).await` at the end of an engine loop's cycle, so every engine gets
/// registry visibility with a single call-site change and no other loop logic touched.
pub async fn heartbeat_sleep(handle: &TaskHandle, dur: Duration) {
    handle.tick();
    tokio::time::sleep(dur).await;
}

/// Publishes the full registry snapshot on the `platform_health` WebSocket topic every 5s.
/// Deliberately a small standalone poll rather than threading the event bus through every one
/// of the other ~10 engines' call sites: the payload is tiny (a handful of small structs) and
/// this is an in-memory HashMap read, not a network call — a world apart from the external-API
/// polling this whole architecture pass exists to get rid of.
pub async fn run_platform_health_publisher(registry: EngineRegistry, bus: crate::events::EventBus, handle: TaskHandle) {
    loop {
        let snap = snapshot(&registry);
        crate::events::publish(&bus, crate::events::TOPIC_PLATFORM_HEALTH, &snap);
        heartbeat_sleep(&handle, Duration::from_secs(5)).await;
    }
}
