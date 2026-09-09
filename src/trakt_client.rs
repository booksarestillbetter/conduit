// src/trakt_client.rs
//! Trakt API calls for the watch-status sync's read-back direction (`engines::watch_sync`).
//! The existing write direction (pushing local Plex scrobbles into Trakt's `/sync/history`) has
//! lived inline in `watch_sync.rs` since before this module existed — left as-is rather than
//! moved, to keep this change scoped to the new read-back functionality.

use serde::Deserialize;
use std::time::Duration;

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
    let res = auth_headers(http_client().get("https://api.trakt.tv/sync/watched/movies"), client_id, access_token)
        .send()
        .await?
        .error_for_status()?;
    Ok(res.json().await?)
}

/// `GET /sync/watched/shows` — every show Trakt has watched episodes for, nested
/// `seasons[].episodes[]` each with their own `last_watched_at`.
pub async fn fetch_watched_shows(client_id: &str, access_token: &str) -> anyhow::Result<Vec<WatchedShowEntry>> {
    let res = auth_headers(http_client().get("https://api.trakt.tv/sync/watched/shows"), client_id, access_token)
        .send()
        .await?
        .error_for_status()?;
    Ok(res.json().await?)
}
