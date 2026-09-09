// src/api/ws.rs
use crate::auth::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
};
use crate::events::TOPIC_TELEMETRY;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashSet;
use tracing::debug;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct WsAuthParams {
    /// Single-use ticket minted via `POST /api/auth/ws-ticket` — required, the connection is
    /// rejected with 401 without one.
    ticket: Option<String>,
}

/// Client -> server control message. `{"type":"subscribe","topics":["pipeline","events",...]}`
/// opts this connection into additional push channels beyond the always-on `telemetry` stream
/// (unchanged from before this existed, for backward compatibility with any client that never
/// sends a subscribe message at all). `{"type":"unsubscribe","topics":[...]}` removes them.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Subscribe { topics: Vec<String> },
    Unsubscribe { topics: Vec<String> },
}

#[utoipa::path(
    get,
    path = "/api/ws",
    tag = "System",
    summary = "Real-time telemetry & channel WebSocket stream",
    description = "Upgrades to a WebSocket connection (not representable as a normal HTTP response — OpenAPI has no native WebSocket support, so this entry documents the protocol only). \
\
Server pushes a `{\"type\":\"telemetry\",\"stats\":...,\"torrents\":[...]}` snapshot every ~1s unconditionally, for backward compatibility with clients that predate the channel system below. \
\
Client may additionally send `{\"type\":\"subscribe\",\"topics\":[\"events\",\"pipeline\",\"arr_stats\",\"platform_health\"]}` (or `\"unsubscribe\"` with the same shape) at any time to opt into push-on-write/push-on-poll updates on those topics, delivered as `{\"type\":\"channel\",\"topic\":\"...\",\"data\":...}`: \
`events` and `pipeline` publish the instant a row is written to the database (new audit-log line / new or updated Arr grab); \
`arr_stats` and `platform_health` publish at the end of their respective ~30s/~5s background poller cycles.",
    params(WsAuthParams),
    responses(
        (status = 101, description = "Switching Protocols — WebSocket connection established"),
        (status = 401, description = "Missing or invalid WebSocket ticket")
    )
)]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(params): Query<WsAuthParams>,
) -> impl IntoResponse {
    // The WS upgrade previously accepted any connection with no server-side auth check at all —
    // the frontend sent a JWT in the query string, but nothing here ever validated it. Require
    // a valid, single-use ticket minted via the authenticated /api/auth/ws-ticket endpoint.
    let valid_user = params
        .ticket
        .as_deref()
        .and_then(|t| state.ws_tickets.redeem(t));

    match valid_user {
        Some(_user_id) => ws.on_upgrade(move |socket| handle_socket(socket, state)).into_response(),
        None => (StatusCode::UNAUTHORIZED, "Missing or invalid WebSocket ticket").into_response(),
    }
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    debug!("New WebSocket client connected");

    let (mut sender, mut receiver) = socket.split();
    let mut bus_rx = state.event_bus.subscribe();
    let mut subscribed_topics: HashSet<String> = HashSet::new();

    // Send one immediate snapshot on connect rather than making a fresh client wait up to 1s
    // for the shared publisher's next tick (see engines::telemetry::compute_snapshot — this is
    // the only place outside that engine's own loop that computes a snapshot, so it doesn't
    // reintroduce the per-tick-per-connection cost this design replaced).
    let initial = crate::engines::telemetry::compute_snapshot(&state.fetcher_pool, &state.db);
    if let Ok(json_str) = serde_json::to_string(&initial) {
        if sender.send(Message::Text(json_str.into())).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            // 1. Push-on-write/push-on-poll-tick channel events. `telemetry` is forwarded to
            // every connection unconditionally — it's the always-on default stream every client
            // gets whether or not it ever sends a `subscribe` message, unchanged in shape/cadence
            // from before the channel system existed (see engines::telemetry, which now computes
            // this once per tick for all connections instead of each connection computing its
            // own copy). Every other topic is filtered to what this connection subscribed to
            // (see events::TOPIC_* — events, pipeline, arr_stats, platform_health).
            bus_msg = bus_rx.recv() => {
                match bus_msg {
                    Ok(evt) if evt.topic == TOPIC_TELEMETRY => {
                        if let Ok(json_str) = serde_json::to_string(&evt.payload) {
                            if sender.send(Message::Text(json_str.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Ok(evt) => {
                        if subscribed_topics.contains(&evt.topic) {
                            let msg_payload = serde_json::json!({
                                "type": "channel",
                                "topic": evt.topic,
                                "data": evt.payload,
                            });
                            if let Ok(json_str) = serde_json::to_string(&msg_payload) {
                                if sender.send(Message::Text(json_str.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    // A slow consumer fell behind the broadcast buffer — just resync to "now"
                    // rather than closing the connection; the client's own initial REST fetch
                    // already covers anything missed in the gap.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }

            // 2. Client control messages (subscribe/unsubscribe) and close detection.
            client_msg = receiver.next() => {
                match client_msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<ClientMessage>(&text) {
                            match cmd {
                                ClientMessage::Subscribe { topics } => {
                                    subscribed_topics.extend(topics);
                                }
                                ClientMessage::Unsubscribe { topics } => {
                                    for t in &topics {
                                        subscribed_topics.remove(t);
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    debug!("WebSocket client disconnected");
}
