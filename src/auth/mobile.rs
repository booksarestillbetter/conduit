// src/auth/mobile.rs
use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Clone)]
pub struct MobilePairingSession {
    pub pair_code: String,
    pub user_id: String,
    pub username: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Default)]
pub struct MobilePairingStore {
    inner: Arc<Mutex<HashMap<String, MobilePairingSession>>>,
}

impl MobilePairingStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Generates a new 3-minute pairing session for mobile app QR scan
    pub fn create_session(&self, user_id: &str, username: &str, is_admin: bool) -> MobilePairingSession {
        let mut map = self.inner.lock();
        let now = Utc::now();

        // Evict expired sessions
        map.retain(|_, s| s.expires_at > now);

        let random_part = Alphanumeric.sample_string(&mut rand::rng(), 32);
        let pair_code = format!("conduit_pair_{}", random_part);
        let expires_at = now + Duration::seconds(180);

        let session = MobilePairingSession {
            pair_code: pair_code.clone(),
            user_id: user_id.to_string(),
            username: username.to_string(),
            is_admin,
            created_at: now,
            expires_at,
        };

        map.insert(pair_code, session.clone());
        session
    }

    /// Single-use redemption of mobile pairing session
    pub fn redeem_session(&self, pair_code: &str) -> Option<MobilePairingSession> {
        let mut map = self.inner.lock();
        let now = Utc::now();

        // Clean expired
        map.retain(|_, s| s.expires_at > now);

        if let Some(session) = map.remove(pair_code) {
            if session.expires_at > now {
                return Some(session);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatePairTokenResponse {
    pub pair_code: String,
    pub qr_payload: String,
    pub server_url: Option<String>,
    pub expires_in_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MobilePairRequest {
    pub pair_code: String,
    pub device_name: Option<String>,
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MobilePairResponse {
    pub token: String,
    pub user: crate::db::UserRecord,
    pub server_version: String,
    pub ws_endpoint: String,
}
