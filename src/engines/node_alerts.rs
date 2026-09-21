// src/engines/node_alerts.rs
//! Forwards live alerts pushed by nodes whose daemon has an alert stream (Synapse) onto the
//! `node_alerts` WebSocket topic.
//!
//! Polling remains the source of truth for torrent state; this only adds immediacy (a torrent
//! finished, a peer was banned, a tracker was announced to) and never blocks or replaces
//! anything. Nodes come and go as the configuration changes, so a light reconcile loop keeps one
//! forwarding task per node's *current* client and drops the task of a node that was removed or
//! rebuilt.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use super::registry::{heartbeat_sleep, TaskHandle};
use crate::events::{self, EventBus, TOPIC_NODE_ALERTS};
use crate::fetcher::FetcherPool;

const RECONCILE_INTERVAL: Duration = Duration::from_secs(10);

struct Forwarder {
    /// Identity of the client the task was started for; a rebuilt client gets a fresh task.
    client_id: usize,
    task: JoinHandle<()>,
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Publishes every alert from `rx` on the bus until the node's feed closes.
async fn forward(
    rx: &mut tokio::sync::broadcast::Receiver<fetcher_core::NodeAlert>,
    bus: &EventBus,
) {
    loop {
        match rx.recv().await {
            Ok(alert) => events::publish(bus, TOPIC_NODE_ALERTS, &alert),
            // A burst outran us: skip ahead, the REST snapshot covers the gap.
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

pub async fn run_node_alerts_loop(pool: Arc<FetcherPool>, bus: EventBus, handle: TaskHandle) {
    let mut forwarders: HashMap<String, Forwarder> = HashMap::new();
    loop {
        let mut present = Vec::new();
        for client in pool.get_all_clients() {
            let name = client.node_name().to_string();
            let client_id = Arc::as_ptr(&client) as *const () as usize;
            present.push(name.clone());
            if forwarders
                .get(&name)
                .is_some_and(|f| f.client_id == client_id)
            {
                continue;
            }
            let Some(mut rx) = client.subscribe_alerts() else {
                continue;
            };
            let bus = bus.clone();
            let task = tokio::spawn(async move { forward(&mut rx, &bus).await });
            forwarders.insert(name, Forwarder { client_id, task });
        }
        forwarders.retain(|name, _| present.contains(name));
        heartbeat_sleep(&handle, RECONCILE_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn alerts_from_a_node_reach_the_node_alerts_topic_until_the_feed_closes() {
        let bus = events::new_bus();
        let mut ws = bus.subscribe();
        let (tx, mut rx) = tokio::sync::broadcast::channel(8);
        let task = tokio::spawn({
            let bus = bus.clone();
            async move { forward(&mut rx, &bus).await }
        });
        tx.send(fetcher_core::NodeAlert {
            node: "seedbox".into(),
            event_type: "TorrentFinished".into(),
            info_hash: Some("ab".repeat(20)),
            timestamp_ms: 7,
            data: serde_json::json!({"name": "x"}),
        })
        .unwrap();
        let evt = tokio::time::timeout(Duration::from_secs(2), ws.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(evt.topic, TOPIC_NODE_ALERTS);
        assert_eq!(evt.payload["node"], "seedbox");
        assert_eq!(evt.payload["event_type"], "TorrentFinished");
        assert_eq!(evt.payload["data"]["name"], "x");
        drop(tx);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
    }
}
