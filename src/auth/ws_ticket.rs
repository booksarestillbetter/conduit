// src/auth/ws_ticket.rs
//! Short-lived, single-use tickets for authenticating the WebSocket upgrade.
//!
//! Browsers' native WebSocket API cannot set an `Authorization` header, so the only ways to
//! authenticate a WS handshake are a cookie (already sent automatically) or something in the
//! URL. Putting the long-lived JWT itself in the URL means it rides through proxy/access logs
//! and browser history on every connect and reconnect. Instead, an already-authenticated client
//! exchanges its JWT for a one-time ticket via a normal (header-authenticated) REST call, and
//! uses only that short-lived ticket in the WS URL — worthless the moment it's used or expires.

use parking_lot::Mutex;
use rand::RngCore;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const TICKET_TTL: Duration = Duration::from_secs(30);

struct TicketInfo {
    user_id: String,
    expires_at: Instant,
}

#[derive(Clone)]
pub struct WsTicketStore {
    inner: Arc<Mutex<HashMap<String, TicketInfo>>>,
}

impl Default for WsTicketStore {
    fn default() -> Self {
        Self::new()
    }
}

impl WsTicketStore {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())) }
    }

    pub fn issue(&self, user_id: &str) -> String {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let ticket = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes);

        let mut map = self.inner.lock();
        // Opportunistic cleanup of anything that expired without being redeemed.
        let now = Instant::now();
        map.retain(|_, info| info.expires_at > now);

        map.insert(ticket.clone(), TicketInfo { user_id: user_id.to_string(), expires_at: now + TICKET_TTL });
        ticket
    }

    /// Consumes the ticket if valid and unexpired — a ticket can only ever redeem once.
    pub fn redeem(&self, ticket: &str) -> Option<String> {
        let mut map = self.inner.lock();
        match map.remove(ticket) {
            Some(info) if info.expires_at > Instant::now() => Some(info.user_id),
            _ => None,
        }
    }
}
