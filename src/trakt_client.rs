// src/trakt_client.rs
//! Trakt API calls for the watch-status sync's read-back direction (`engines::watch_sync`).
//! The existing write direction (pushing local Plex scrobbles into Trakt's `/sync/history`) has
//! lived inline in `watch_sync.rs` since before this module existed — left as-is rather than
//! moved, to keep this change scoped to the new read-back functionality.

use crate::config::ConfigManager;
use serde::Deserialize;
use std::time::Duration;
use tracing::{info, warn};

pub const API_BASE: &str = "https://api.trakt.tv";

/// Refresh this long before the token actually expires, so a request in flight when it lapses
/// doesn't fail.
const REFRESH_MARGIN_SECS: i64 = 3600;

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default()
}

#[derive(Debug, Deserialize)]
pub struct TraktIds {
    pub imdb: Option<String>,
    pub tmdb: Option<i64>,
    pub tvdb: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct TraktMovie {
    pub ids: TraktIds,
}

#[derive(Debug, Deserialize)]
pub struct WatchedMovieEntry {
    pub last_watched_at: String,
    pub movie: TraktMovie,
}

#[derive(Debug, Deserialize)]
pub struct TraktShow {
    pub ids: TraktIds,
}

#[derive(Debug, Deserialize)]
pub struct WatchedEpisode {
    pub number: i64,
    pub last_watched_at: String,
}

#[derive(Debug, Deserialize)]
pub struct WatchedSeason {
    pub number: i64,
    pub episodes: Vec<WatchedEpisode>,
}

#[derive(Debug, Deserialize)]
pub struct WatchedShowEntry {
    pub show: TraktShow,
    #[serde(default)]
    pub seasons: Vec<WatchedSeason>,
}

fn auth_headers(req: reqwest::RequestBuilder, client_id: &str, access_token: &str) -> reqwest::RequestBuilder {
    req.header("Content-Type", "application/json")
        .header("trakt-api-version", "2")
        .header("trakt-api-key", client_id)
        .header("Authorization", format!("Bearer {}", access_token))
}

/// `GET /sync/watched/movies` — every movie Trakt has ever marked watched for this account, each
/// with a `last_watched_at` timestamp. The caller (`engines::watch_sync`) filters this against
/// its own stored watermark rather than this function doing any date filtering itself, since
/// Trakt's `/sync/watched/*` endpoints don't support a server-side `start_at` filter (unlike
/// `/sync/history/*`, which does but paginates differently) — sizes here are a single user's
/// movie library, not expected to be large enough for that to matter.
pub async fn fetch_watched_movies(client_id: &str, access_token: &str) -> anyhow::Result<Vec<WatchedMovieEntry>> {
    let res = auth_headers(http_client().get(format!("{API_BASE}/sync/watched/movies")), client_id, access_token)
        .send()
        .await?
        .error_for_status()?;
    Ok(res.json().await?)
}

/// `GET /sync/watched/shows` — every show Trakt has watched episodes for, nested
/// `seasons[].episodes[]` each with their own `last_watched_at`.
pub async fn fetch_watched_shows(client_id: &str, access_token: &str) -> anyhow::Result<Vec<WatchedShowEntry>> {
    let res = auth_headers(http_client().get(format!("{API_BASE}/sync/watched/shows")), client_id, access_token)
        .send()
        .await?
        .error_for_status()?;
    Ok(res.json().await?)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    #[serde(default)]
    created_at: Option<i64>,
}

/// A fresh token pair from Trakt. `expires_at` is a Unix timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshedTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

/// `POST /oauth/token` with the `refresh_token` grant. Trakt refresh tokens are single use: the
/// response carries a new one, and the old one stops working, so the caller must store the
/// result before doing anything else.
pub async fn refresh_tokens(
    base_url: &str,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> anyhow::Result<RefreshedTokens> {
    let res = http_client()
        .post(format!("{}/oauth/token", base_url.trim_end_matches('/')))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "refresh_token": refresh_token,
            "client_id": client_id,
            "client_secret": client_secret,
            "redirect_uri": "urn:ietf:wg:oauth:2.0:oob",
            "grant_type": "refresh_token",
        }))
        .send()
        .await?;
    let status = res.status();
    if !status.is_success() {
        anyhow::bail!("Trakt refused the token refresh (HTTP {status}); re-authorize Trakt in Settings");
    }
    let t: TokenResponse = res.json().await?;
    Ok(RefreshedTokens {
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_at: t.created_at.unwrap_or_else(|| chrono::Utc::now().timestamp()) + t.expires_in,
    })
}

/// Whether a stored token should be refreshed now: we hold what a refresh needs (client secret
/// and refresh token) and the token's expiry is within the margin, past, or not recorded yet (a
/// token pasted in by hand has none; refreshing once records one).
fn needs_refresh(trakt: &crate::config::TraktConfig, now: i64) -> bool {
    let has_refresh = trakt.refresh_token.as_deref().is_some_and(|t| !t.is_empty());
    !trakt.client_secret.is_empty()
        && has_refresh
        && trakt.token_expiry.is_none_or(|exp| now >= exp - REFRESH_MARGIN_SECS)
}

/// Protects freshly refreshed tokens from a settings page that was opened before the refresh.
/// Refreshing only ever moves `token_expiry` forward, so an incoming config whose expiry is
/// older than the stored one carries a stale token pair (and a refresh token Trakt has already
/// retired); the stored pair is kept instead of being overwritten by it.
pub fn keep_newer_tokens(stored: &crate::config::TraktConfig, incoming: &mut crate::config::TraktConfig) {
    let stale = match (stored.token_expiry, incoming.token_expiry) {
        (Some(s), Some(i)) => i < s,
        (Some(_), None) => incoming.refresh_token == stored.refresh_token || incoming.refresh_token.is_none(),
        _ => false,
    };
    if stale {
        incoming.access_token = stored.access_token.clone();
        incoming.refresh_token = stored.refresh_token.clone();
        incoming.token_expiry = stored.token_expiry;
    }
}

/// The access token to use for this sync cycle, refreshing it first when it is about to
/// expire and saving the new pair to the config. Returns `None` if Trakt isn't set up. If a
/// refresh fails, the current token is returned anyway (it may still work, and the failure is
/// logged); the next cycle tries again.
pub async fn current_access_token(config_mgr: &ConfigManager, base_url: &str, now: i64) -> Option<String> {
    let config = config_mgr.get().await;
    let trakt = &config.trakt;
    let access = trakt.access_token.clone().filter(|t| !t.is_empty());

    if !needs_refresh(trakt, now) {
        return access;
    }

    let refresh_token = trakt.refresh_token.clone().unwrap_or_default();
    match refresh_tokens(base_url, &trakt.client_id, &trakt.client_secret, &refresh_token).await {
        Ok(new) => {
            // Read the config again just before saving so edits made in the meantime survive;
            // only the three token fields change.
            let mut latest = config_mgr.get().await;
            latest.trakt.access_token = Some(new.access_token.clone());
            latest.trakt.refresh_token = Some(new.refresh_token);
            latest.trakt.token_expiry = Some(new.expires_at);
            match config_mgr.update(latest).await {
                Ok(()) => info!("Refreshed the Trakt access token"),
                Err(e) => warn!(
                    "Refreshed the Trakt token but could not save it ({e}); Trakt may need to be re-authorized after a restart"
                ),
            }
            Some(new.access_token)
        }
        Err(e) => {
            warn!("Trakt token refresh failed: {e}");
            access
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TraktConfig;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// One-shot Trakt stand-in: answers every request with `status`/`body` and records the
    /// request bodies it received.
    async fn trakt(status: u16, body: serde_json::Value) -> (String, Arc<Mutex<Vec<serde_json::Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let start = loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break None;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break Some(i + 4);
                    }
                };
                let Some(start) = start else { continue };
                let head = String::from_utf8_lossy(&buf[..start]).to_ascii_lowercase();
                let len: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
                while buf.len() < start + len {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                if let Ok(v) = serde_json::from_slice(&buf[start..start + len]) {
                    seen2.lock().unwrap().push(v);
                }
                let text = body.to_string();
                let resp = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                    text.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });
        (base, seen)
    }

    fn cfg(expiry: Option<i64>) -> TraktConfig {
        TraktConfig {
            enabled: true,
            client_id: "cid".into(),
            client_secret: "secret".into(),
            access_token: Some("old-access".into()),
            refresh_token: Some("old-refresh".into()),
            token_expiry: expiry,
            sync_interval_mins: 360,
            sync_watched_back_to_plex: false,
        }
    }

    async fn manager(trakt: TraktConfig) -> (ConfigManager, tempfile::NamedTempFile) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mgr = ConfigManager::load_or_init(tmp.path().to_str().unwrap()).await.unwrap();
        let mut c = mgr.get().await;
        c.trakt = trakt;
        mgr.update(c).await.unwrap();
        (mgr, tmp)
    }

    #[test]
    fn only_a_token_that_is_close_to_expiring_and_refreshable_is_refreshed() {
        let now = 1_000_000;
        assert!(!needs_refresh(&cfg(Some(now + 7200)), now), "plenty of time left");
        assert!(needs_refresh(&cfg(Some(now + 600)), now), "inside the margin");
        assert!(needs_refresh(&cfg(Some(now - 5)), now), "already expired");
        assert!(needs_refresh(&cfg(None), now), "unknown expiry: refresh once to learn it");

        let mut no_secret = cfg(Some(now - 5));
        no_secret.client_secret.clear();
        assert!(!needs_refresh(&no_secret, now));
        let mut no_refresh = cfg(Some(now - 5));
        no_refresh.refresh_token = None;
        assert!(!needs_refresh(&no_refresh, now));
    }

    #[tokio::test]
    async fn an_expiring_token_is_refreshed_and_the_new_pair_is_saved() {
        let (base, seen) = trakt(
            200,
            serde_json::json!({"access_token":"new-access","refresh_token":"new-refresh","expires_in":86400,"created_at":2_000_000}),
        )
        .await;
        let now = 1_999_990;
        let (mgr, _tmp) = manager(cfg(Some(now + 60))).await;

        let token = current_access_token(&mgr, &base, now).await;
        assert_eq!(token.as_deref(), Some("new-access"));

        let saved = mgr.get().await.trakt;
        assert_eq!(saved.access_token.as_deref(), Some("new-access"));
        assert_eq!(saved.refresh_token.as_deref(), Some("new-refresh"), "refresh tokens are single use");
        assert_eq!(saved.token_expiry, Some(2_000_000 + 86400));
        assert_eq!(saved.client_secret, "secret", "nothing else changes");

        let sent = seen.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["grant_type"], "refresh_token");
        assert_eq!(sent[0]["refresh_token"], "old-refresh");
        assert_eq!(sent[0]["client_id"], "cid");
        assert_eq!(sent[0]["client_secret"], "secret");
    }

    #[tokio::test]
    async fn a_token_with_time_left_is_used_without_calling_trakt() {
        let (base, seen) = trakt(500, serde_json::json!({})).await;
        let now = 1_000;
        let (mgr, _tmp) = manager(cfg(Some(now + 100_000))).await;
        assert_eq!(current_access_token(&mgr, &base, now).await.as_deref(), Some("old-access"));
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_refused_refresh_keeps_the_current_token_and_the_saved_config() {
        let (base, _seen) = trakt(401, serde_json::json!({"error":"invalid_grant"})).await;
        let now = 5_000;
        let (mgr, _tmp) = manager(cfg(Some(now - 1))).await;

        assert_eq!(current_access_token(&mgr, &base, now).await.as_deref(), Some("old-access"));
        let saved = mgr.get().await.trakt;
        assert_eq!(saved.refresh_token.as_deref(), Some("old-refresh"));
        assert_eq!(saved.token_expiry, Some(now - 1));
    }

    #[test]
    fn a_stale_settings_page_cannot_overwrite_refreshed_tokens() {
        let mut stored = cfg(Some(2_000));
        stored.access_token = Some("new-access".into());
        stored.refresh_token = Some("new-refresh".into());

        // Opened before the refresh: older expiry, old tokens.
        let mut stale = cfg(Some(1_000));
        keep_newer_tokens(&stored, &mut stale);
        assert_eq!(stale.access_token.as_deref(), Some("new-access"));
        assert_eq!(stale.refresh_token.as_deref(), Some("new-refresh"));
        assert_eq!(stale.token_expiry, Some(2_000));

        // Unrelated edits in the same save are untouched.
        let mut edited = cfg(Some(1_000));
        edited.sync_interval_mins = 30;
        keep_newer_tokens(&stored, &mut edited);
        assert_eq!(edited.sync_interval_mins, 30);

        // Same or newer expiry (an intentional change) is respected.
        let mut manual = cfg(Some(2_000));
        manual.access_token = Some("pasted".into());
        keep_newer_tokens(&stored, &mut manual);
        assert_eq!(manual.access_token.as_deref(), Some("pasted"));
    }

    #[tokio::test]
    async fn no_token_means_no_sync() {
        let mut t = cfg(None);
        t.access_token = None;
        t.refresh_token = None; // nothing to refresh with either
        let (mgr, _tmp) = manager(t).await;
        assert_eq!(current_access_token(&mgr, API_BASE, 0).await, None);
    }
}
