//! Live alerts from a Synapse daemon (`SubscribeAlerts`), re-published as [`NodeAlert`]s.
//!
//! The stream is a convenience on top of polling, never a dependency: a daemon that predates the
//! RPC just has none (the feed backs off for a long time instead of hammering it), and a dropped
//! connection reconnects with backoff.

use std::sync::Arc;
use std::time::Duration;

use fetcher_core::NodeAlert;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::client::SynapseClient as SdkClient;
use crate::error::SynapseClientError;
use crate::proto::v2::AlertEvent;

/// Alerts buffered per receiver before a slow one starts skipping.
const CHANNEL_CAPACITY: usize = 512;
/// How long to wait before asking a daemon again that does not have the RPC.
const UNSUPPORTED_RETRY: Duration = Duration::from_secs(600);

/// Keeps the alert task alive exactly as long as the node's client.
pub(crate) struct AlertFeed {
    pub(crate) sender: broadcast::Sender<NodeAlert>,
    _task: Arc<AbortOnDrop>,
}

struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Clone for AlertFeed {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            _task: self._task.clone(),
        }
    }
}

impl AlertFeed {
    pub(crate) fn spawn(client: SdkClient, node: String) -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        let tx = sender.clone();
        let task = tokio::spawn(async move { run(client, node, tx).await });
        Self {
            sender,
            _task: Arc::new(AbortOnDrop(task)),
        }
    }
}

/// Converts the daemon's event into a [`NodeAlert`]; `payload_json` is passed through as data.
pub fn alert_from_event(node: &str, event: &AlertEvent) -> NodeAlert {
    NodeAlert {
        node: node.to_string(),
        event_type: event.event_type.clone(),
        info_hash: event.info_hash.clone().filter(|h| !h.is_empty()),
        timestamp_ms: event.timestamp_ms,
        data: serde_json::from_str(&event.payload_json).unwrap_or(serde_json::Value::Null),
    }
}

async fn run(client: SdkClient, node: String, tx: broadcast::Sender<NodeAlert>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        // Nobody listening: do not hold a stream open for no one.
        if tx.receiver_count() == 0 {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }
        match client.subscribe_alerts(None).await {
            Ok(mut stream) => {
                debug!("Subscribed to alerts from Synapse node '{node}'");
                backoff = Duration::from_secs(1);
                while let Ok(Some(event)) = stream.message().await {
                    let _ = tx.send(alert_from_event(&node, &event));
                }
                debug!("Alert stream from '{node}' ended; reconnecting");
            }
            Err(SynapseClientError::Rpc {
                code: tonic::Code::Unimplemented,
                ..
            }) => {
                debug!("Synapse node '{node}' has no alert stream; not asking again for a while");
                tokio::time::sleep(UNSUPPORTED_RETRY).await;
                continue;
            }
            Err(e) => warn!("Could not subscribe to alerts from '{node}': {e}"),
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_alert_keeps_its_type_hash_time_and_json() {
        let event = AlertEvent {
            event_type: "TorrentFinished".into(),
            payload_json: r#"{"type":"TorrentFinished","info_hash":"ab","data":{"name":"x"}}"#
                .into(),
            timestamp_ms: 1234,
            info_hash: Some("ab".repeat(20)),
        };
        let alert = alert_from_event("seedbox", &event);
        assert_eq!(alert.node, "seedbox");
        assert_eq!(alert.event_type, "TorrentFinished");
        assert_eq!(alert.info_hash.as_deref(), Some("ab".repeat(20).as_str()));
        assert_eq!(alert.timestamp_ms, 1234);
        assert_eq!(alert.data["data"]["name"], "x");

        // A daemon-side hiccup (unparsable JSON, empty hash) degrades to nulls, never a panic.
        let odd = AlertEvent {
            event_type: "X".into(),
            payload_json: "not json".into(),
            timestamp_ms: 0,
            info_hash: Some(String::new()),
        };
        let alert = alert_from_event("n", &odd);
        assert!(alert.data.is_null());
        assert!(alert.info_hash.is_none());
    }
}
