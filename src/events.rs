// src/events.rs
//! The process-wide pub/sub bus behind Conduit's WebSocket channels. One in-process
//! `tokio::sync::broadcast` channel, not Redis or any external service — Conduit is a single
//! self-hosted binary, every publisher and every WebSocket connection already live in the same
//! process, so a broadcast channel gives cheap fan-out to N subscribers with zero deployment
//! surface. See docs/webhook_reference.md-adjacent architecture notes for the fuller rationale.
//!
//! Publishing is always push-on-write or push-on-poll-tick, never a separate broadcast timer:
//! DB-backed data (events, pipeline items) publishes from inside the same write that already
//! happens (`Database::log_event`, `Database::save_arr_grab`); external-API-backed data
//! (arr stats, platform health) publishes at the end of the same background-poller cycle that
//! already refreshes its REST-facing cache — so the cache and the broadcast can never drift out
//! of sync with each other.

use serde::Serialize;
use tokio::sync::broadcast;

/// Generous enough that a burst of writes (e.g. a season-pack import logging several events at
/// once) doesn't lag a slow WebSocket consumer into missed messages; a lagged receiver just
/// skips ahead rather than blocking any publisher, which is the correct behavior for live
/// telemetry (a client that reconnects re-fetches the REST snapshot anyway).
const CHANNEL_CAPACITY: usize = 256;

pub const TOPIC_EVENTS: &str = "events";
pub const TOPIC_PIPELINE: &str = "pipeline";
pub const TOPIC_ARR_STATS: &str = "arr_stats";
pub const TOPIC_PLATFORM_HEALTH: &str = "platform_health";
/// Live alerts pushed by nodes whose daemon streams them (Synapse): torrent finished, peer
/// banned, tracker announced, ... Payload is a `fetcher_core::NodeAlert`. Subscribe-gated like
/// every topic except telemetry.
pub const TOPIC_NODE_ALERTS: &str = "node_alerts";
/// The always-on cluster telemetry snapshot (aggregate stats + enriched torrent list),
/// published once per tick by `engines::telemetry::run_telemetry_publisher` and forwarded
/// unconditionally (not gated by a client's `subscribed_topics`) to every WebSocket connection
/// by `api::ws::handle_socket` — this is the pre-channel-system default stream every client
/// gets regardless of whether it ever sends a `subscribe` message.
pub const TOPIC_TELEMETRY: &str = "telemetry";

#[derive(Debug, Clone, Serialize)]
pub struct ChannelEvent {
    pub topic: String,
    pub payload: serde_json::Value,
}

pub type EventBus = broadcast::Sender<ChannelEvent>;

pub fn new_bus() -> EventBus {
    let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
    tx
}

/// Serializes `payload` and publishes it on `topic`. Silently drops if there are currently no
/// subscribers (a fresh `broadcast::Sender::send` returning `Err` just means "nobody's
/// listening right now," not a failure) or if serialization somehow fails.
pub fn publish(bus: &EventBus, topic: &str, payload: &impl Serialize) {
    if let Ok(value) = serde_json::to_value(payload) {
        let _ = bus.send(ChannelEvent { topic: topic.to_string(), payload: value });
    }
}
