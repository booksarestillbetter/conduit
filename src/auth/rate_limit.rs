// src/auth/rate_limit.rs
//! In-memory login/2FA throttle. Tracks failed attempts per key (typically
//! `username` or `username|ip`) and locks the key out for a fixed window after
//! too many failures within a rolling window. Deliberately simple — this is a
//! single-instance daemon, not a distributed service, so no external store is
//! needed.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_ATTEMPTS: u32 = 5;
const MAX_IP_ATTEMPTS: u32 = 25;
const WINDOW: Duration = Duration::from_secs(15 * 60);
const LOCKOUT: Duration = Duration::from_secs(15 * 60);
/// Opportunistic cleanup threshold so a spray of distinct usernames/IPs can't grow this
/// map without bound.
const MAX_TRACKED_KEYS: usize = 20_000;

#[derive(Default)]
struct Attempts {
    failures: u32,
    window_started: Option<Instant>,
    locked_until: Option<Instant>,
}

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, Attempts>>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())) }
    }

    fn normalize(key: &str) -> String {
        key.trim().to_lowercase()
    }

    /// Returns `Err(seconds_remaining)` if `key` is currently locked out.
    pub fn check(&self, key: &str) -> Result<(), u64> {
        let key = Self::normalize(key);
        let mut map = self.inner.lock();
        if let Some(rec) = map.get_mut(&key) {
            if let Some(until) = rec.locked_until {
                let now = Instant::now();
                if now < until {
                    return Err((until - now).as_secs());
                }
                // Lockout expired — reset.
                *rec = Attempts::default();
            }
        }
        Ok(())
    }

    pub fn record_failure(&self, key: &str) {
        self.record_failure_with_max(key, MAX_ATTEMPTS);
    }

    pub fn record_failure_with_max(&self, key: &str, max_attempts: u32) {
        let key = Self::normalize(key);
        let mut map = self.inner.lock();

        if map.len() > MAX_TRACKED_KEYS {
            let now = Instant::now();
            map.retain(|_, rec| {
                rec.locked_until.map(|u| u > now).unwrap_or(false)
                    || (rec.failures > 0 && rec.window_started.map(|w| now.duration_since(w) < WINDOW).unwrap_or(false))
            });
        }

        let rec = map.entry(key).or_default();
        let now = Instant::now();
        let window_expired = rec
            .window_started
            .map(|w| now.duration_since(w) > WINDOW)
            .unwrap_or(true);
        if window_expired {
            rec.failures = 0;
            rec.window_started = Some(now);
        }
        rec.failures += 1;
        if rec.failures >= max_attempts {
            rec.locked_until = Some(now + LOCKOUT);
        }
    }

    pub fn record_success(&self, key: &str) {
        let key = Self::normalize(key);
        self.inner.lock().remove(&key);
    }

    /// Check both `(username, ip)` and per-IP rate limits.
    pub fn check_login(&self, ip: &str, username: &str) -> Result<(), u64> {
        let pair_key = format!("u:{}|ip:{}", username.trim(), ip.trim());
        self.check(&pair_key)?;
        if !ip.trim().is_empty() && ip != "unknown" {
            let ip_key = format!("ip:{}", ip.trim());
            self.check(&ip_key)?;
        }
        Ok(())
    }

    /// Record a failed attempt against both `(username, ip)` and per-IP buckets.
    pub fn record_login_failure(&self, ip: &str, username: &str) {
        let pair_key = format!("u:{}|ip:{}", username.trim(), ip.trim());
        self.record_failure_with_max(&pair_key, MAX_ATTEMPTS);
        if !ip.trim().is_empty() && ip != "unknown" {
            let ip_key = format!("ip:{}", ip.trim());
            self.record_failure_with_max(&ip_key, MAX_IP_ATTEMPTS);
        }
    }

    /// Clear the `(username, ip)` bucket on successful authentication.
    pub fn record_login_success(&self, ip: &str, username: &str) {
        let pair_key = format!("u:{}|ip:{}", username.trim(), ip.trim());
        self.record_success(&pair_key);
    }
}
