// src/plex_client.rs
//! All plex.tv (account/OAuth) and direct Plex Media Server HTTP calls live here — shared by
//! the OAuth device-linking flow (src/api/plex_oauth_routes.rs) and the Sonarr/Radarr -> Plex
//! notify-takeover (wired into src/api/arr_routes.rs's import handling), which together replace
//! Sonarr/Radarr's own built-in "Plex Media Server" notification connection.
//!
//! Every plex.tv/server call identifies Conduit as one stable registered device via
//! `X-Plex-Client-Identifier` (a UUID generated once at startup and persisted in
//! `PlexConfig.client_identifier` — see `ConfigManager::load_or_init`), matching the
//! Sonarr/Radarr/Ombi pattern this was modeled on.

use crate::config::PlexNodeConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, warn};

const PLEX_PRODUCT: &str = "Conduit";
const PLEX_DEVICE_NAME: &str = "Conduit Media Server";
const PLEX_VERSION: &str = env!("CARGO_PKG_VERSION");

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default()
}

/// Every `X-Plex-*` identity header Plex expects on both plex.tv and direct-server calls.
fn identity_headers(req: reqwest::RequestBuilder, client_identifier: &str) -> reqwest::RequestBuilder {
    req.header("X-Plex-Client-Identifier", client_identifier)
        .header("X-Plex-Product", PLEX_PRODUCT)
        .header("X-Plex-Platform", "Conduit")
        .header("X-Plex-Device-Name", PLEX_DEVICE_NAME)
        .header("X-Plex-Version", PLEX_VERSION)
        .header("Accept", "application/json")
}

// ── plex.tv OAuth / PIN device linking ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PlexPinStatus {
    pub id: i64,
    pub code: String,
    #[serde(rename = "authToken")]
    pub auth_token: Option<String>,
}

/// `POST https://plex.tv/api/v2/pins?strong=true` — step 1 of the OAuth flow. The returned
/// `code` is what the user enters (implicitly, via the sign-in URL) at plex.tv; `id` is what we
/// poll on next.
pub async fn create_pin(client_identifier: &str) -> anyhow::Result<PlexPinStatus> {
    let res = identity_headers(http_client().post("https://plex.tv/api/v2/pins"), client_identifier)
        .query(&[("strong", "true")])
        .send()
        .await?
        .error_for_status()?;
    Ok(res.json::<PlexPinStatus>().await?)
}

/// Builds the URL the frontend opens in a popup for the user to actually log in and approve —
/// this step has to happen in the browser, it's Plex's own hosted sign-in page.
pub fn build_signin_url(client_identifier: &str, pin_code: &str, forward_url: &str) -> String {
    let query = format!(
        "clientID={}&code={}&forwardUrl={}&context%5Bdevice%5D%5Bproduct%5D={}",
        urlencoding_encode(client_identifier),
        urlencoding_encode(pin_code),
        urlencoding_encode(forward_url),
        urlencoding_encode(PLEX_PRODUCT),
    );
    format!("https://app.plex.tv/auth#?{}", query)
}

fn urlencoding_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// `GET https://plex.tv/api/v2/pins/{id}` — step 2, polled every ~2s by the frontend via our
/// `/api/plex/oauth/poll/{pin_id}` endpoint. `auth_token` stays `None` until the user approves.
pub async fn poll_pin(client_identifier: &str, pin_id: i64) -> anyhow::Result<PlexPinStatus> {
    let res = identity_headers(
        http_client().get(format!("https://plex.tv/api/v2/pins/{}", pin_id)),
        client_identifier,
    )
    .send()
    .await?
    .error_for_status()?;
    Ok(res.json::<PlexPinStatus>().await?)
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PlexDiscoveredServer {
    pub name: String,
    pub connections: Vec<PlexDiscoveredConnection>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PlexDiscoveredConnection {
    pub uri: String,
    pub local: bool,
}

#[derive(Debug, Deserialize)]
struct PlexResource {
    name: String,
    provides: String,
    owned: bool,
    #[serde(rename = "connections", default)]
    connections: Vec<PlexResourceConnection>,
}

#[derive(Debug, Deserialize)]
struct PlexResourceConnection {
    uri: String,
    #[serde(default)]
    local: bool,
}

/// `GET https://plex.tv/api/v2/resources` — step 3, once we have an account token. The same
/// account token this returns alongside these resources works directly as each server's own
/// `X-Plex-Token` — no further per-server exchange needed.
pub async fn discover_owned_servers(client_identifier: &str, account_token: &str) -> anyhow::Result<Vec<PlexDiscoveredServer>> {
    let res = identity_headers(http_client().get("https://plex.tv/api/v2/resources"), client_identifier)
        .header("X-Plex-Token", account_token)
        .query(&[("includeHttps", "1")])
        .send()
        .await?
        .error_for_status()?;
    let resources: Vec<PlexResource> = res.json().await?;

    Ok(resources
        .into_iter()
        .filter(|r| r.owned && r.provides.split(',').any(|p| p == "server"))
        .map(|r| PlexDiscoveredServer {
            name: r.name,
            connections: r
                .connections
                .into_iter()
                .map(|c| PlexDiscoveredConnection { uri: c.uri, local: c.local })
                .collect(),
        })
        .collect())
}

// ── Direct Plex Media Server calls (notify-takeover) ────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SectionsResponse {
    #[serde(rename = "MediaContainer")]
    media_container: SectionsContainer,
}

#[derive(Debug, Deserialize)]
struct SectionsContainer {
    #[serde(rename = "Directory", default)]
    directory: Vec<PlexSection>,
}

#[derive(Debug, Deserialize)]
struct PlexSection {
    key: String,
    #[serde(rename = "type")]
    section_type: String,
    #[serde(rename = "Location", default)]
    locations: Vec<PlexLocation>,
}

#[derive(Debug, Deserialize)]
struct PlexLocation {
    path: String,
}

async fn fetch_sections(base_url: &str, token: &str, client_identifier: &str) -> anyhow::Result<Vec<PlexSection>> {
    let url = format!("{}/library/sections", base_url.trim_end_matches('/'));
    let res = identity_headers(http_client().get(&url), client_identifier)
        .header("X-Plex-Token", token)
        .send()
        .await?
        .error_for_status()?;
    let parsed: SectionsResponse = res.json().await?;
    Ok(parsed.media_container.directory)
}

fn path_is_under(item_path: &str, section_location: &str) -> bool {
    let item = item_path.trim_end_matches(['/', '\\']);
    let loc = section_location.trim_end_matches(['/', '\\']);
    if item == loc {
        return true;
    }
    item.starts_with(&format!("{}/", loc)) || item.starts_with(&format!("{}\\", loc))
}

async fn trigger_section_refresh(base_url: &str, token: &str, client_identifier: &str, section_id: &str, item_path: &str) -> anyhow::Result<()> {
    let url = format!("{}/library/sections/{}/refresh", base_url.trim_end_matches('/'), section_id);
    identity_headers(http_client().get(&url), client_identifier)
        .header("X-Plex-Token", token)
        .query(&[("path", item_path)])
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// Called from the Sonarr/Radarr import webhook handlers (fire-and-forget, `tokio::spawn`ed —
/// see `src/api/arr_routes.rs`) in place of Sonarr/Radarr's own built-in Plex notification.
/// Mirrors `3rd/Sonarr/.../Plex/Server/PlexServerService.cs`: enumerate sections once, match the
/// item's root folder against each section's configured locations, and trigger a *targeted*
/// partial rescan (`?path=...`) rather than a full-library refresh. Notifies every enabled node
/// independently; a node whose libraries don't include this path simply finds no match and is
/// skipped — Conduit has no per-instance "zone" mapping yet (see docs/webhook_reference.md-adjacent
/// architecture notes), so this is deliberately permissive rather than silently doing nothing.
pub async fn notify_library_update(nodes: &[PlexNodeConfig], shared_token: &str, client_identifier: &str, item_root_path: &str, section_type: &str) {
    if item_root_path.is_empty() {
        return;
    }
    for node in nodes {
        let token = node
            .token_override
            .as_deref()
            .filter(|t| !t.is_empty())
            .unwrap_or(shared_token);
        if token.is_empty() {
            continue;
        }

        let sections = match fetch_sections(&node.url, token, client_identifier).await {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to fetch Plex library sections from '{}': {}", node.name, e);
                continue;
            }
        };

        let matched_section = sections
            .iter()
            .filter(|s| s.section_type == section_type)
            .find(|s| s.locations.iter().any(|loc| path_is_under(item_root_path, &loc.path)));

        match matched_section {
            Some(section) => {
                if let Err(e) = trigger_section_refresh(&node.url, token, client_identifier, &section.key, item_root_path).await {
                    warn!("Failed to trigger targeted Plex refresh on '{}' (section {}): {}", node.name, section.key, e);
                } else {
                    debug!("Triggered targeted Plex refresh on '{}' section {} for '{}'", node.name, section.key, item_root_path);
                }
            }
            None => {
                debug!(
                    "Plex node '{}' has no {} library section covering '{}' — skipping (no scan triggered on this node)",
                    node.name, section_type, item_root_path
                );
            }
        }
    }
}

// ── Watch-status sync (Trakt-backed, multi-server) ──────────────────────────────────────

#[derive(Debug, Deserialize)]
struct IdentityResponse {
    #[serde(rename = "MediaContainer")]
    media_container: IdentityContainer,
}

#[derive(Debug, Deserialize)]
struct IdentityContainer {
    #[serde(rename = "machineIdentifier")]
    machine_identifier: String,
}

/// Plex's permanent per-server machine identifier — survives renames, unlike `Server.title`.
/// Used to match an incoming webhook's `Server.uuid` back to a configured `PlexNodeConfig`.
pub async fn fetch_server_identity(base_url: &str, token: &str, client_identifier: &str) -> anyhow::Result<String> {
    let url = format!("{}/identity", base_url.trim_end_matches('/'));
    let res = identity_headers(http_client().get(&url), client_identifier)
        .header("X-Plex-Token", token)
        .send()
        .await?
        .error_for_status()?;
    let parsed: IdentityResponse = res.json().await?;
    Ok(parsed.media_container.machine_identifier)
}

#[derive(Debug, Deserialize)]
struct LibraryItemsResponse {
    #[serde(rename = "MediaContainer")]
    media_container: LibraryItemsContainer,
}

#[derive(Debug, Deserialize)]
struct LibraryItemsContainer {
    #[serde(rename = "Metadata", default)]
    metadata: Vec<LibraryItem>,
}

#[derive(Debug, Deserialize)]
struct LibraryItem {
    #[serde(rename = "ratingKey")]
    rating_key: String,
    #[serde(rename = "parentIndex", default)]
    parent_index: Option<i64>,
    #[serde(default)]
    index: Option<i64>,
    #[serde(rename = "Guid", default)]
    guid: Vec<PlexItemGuid>,
}

#[derive(Debug, Deserialize)]
struct PlexItemGuid {
    id: String,
}

/// The cross-server identity of a watched item — whichever of imdb/tmdb/tvdb ids Trakt gave us
/// for it. Matched against a Plex item's own `Guid[]` array, which uses the identical
/// `imdb://`/`tmdb://`/`tvdb://` id format already parsed from incoming webhooks (see
/// `api::webhook_routes`).
#[derive(Debug, Clone, Copy, Default)]
pub struct GuidMatch<'a> {
    pub imdb_id: Option<&'a str>,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<i64>,
}

fn guid_matches(item_guids: &[PlexItemGuid], target: &GuidMatch) -> bool {
    item_guids.iter().any(|g| {
        if let Some(rest) = g.id.strip_prefix("imdb://") {
            if let Some(imdb) = target.imdb_id {
                return rest == imdb;
            }
        }
        if let Some(rest) = g.id.strip_prefix("tmdb://") {
            if let Some(tmdb) = target.tmdb_id {
                return rest.parse::<i64>().ok() == Some(tmdb);
            }
        }
        if let Some(rest) = g.id.strip_prefix("tvdb://") {
            if let Some(tvdb) = target.tvdb_id {
                return rest.parse::<i64>().ok() == Some(tvdb);
            }
        }
        false
    })
}

async fn fetch_section_items(base_url: &str, token: &str, client_identifier: &str, section_key: &str) -> anyhow::Result<Vec<LibraryItem>> {
    let url = format!("{}/library/sections/{}/all", base_url.trim_end_matches('/'), section_key);
    let res = identity_headers(http_client().get(&url), client_identifier)
        .header("X-Plex-Token", token)
        .send()
        .await?
        .error_for_status()?;
    let parsed: LibraryItemsResponse = res.json().await?;
    Ok(parsed.media_container.metadata)
}

/// Searches this server's movie library sections for an item matching `target`, returning its
/// local `ratingKey` if found. `None` (not an error) means this server simply doesn't own the
/// title — an expected, common outcome in a multi-server setup, not a fault.
pub async fn find_movie_rating_key(base_url: &str, token: &str, client_identifier: &str, target: &GuidMatch<'_>) -> anyhow::Result<Option<String>> {
    let sections = fetch_sections(base_url, token, client_identifier).await?;
    for section in sections.iter().filter(|s| s.section_type == "movie") {
        let items = fetch_section_items(base_url, token, client_identifier, &section.key).await?;
        if let Some(item) = items.iter().find(|i| guid_matches(&i.guid, target)) {
            return Ok(Some(item.rating_key.clone()));
        }
    }
    Ok(None)
}

/// Same idea as `find_movie_rating_key` but for one specific episode: first finds the show by
/// `show_target`'s ids, then looks within that show's own episode list for a season/episode
/// number match. Returns `None` (not an error) if this server doesn't have the show at all, or
/// has the show but not that particular episode yet.
pub async fn find_episode_rating_key(
    base_url: &str,
    token: &str,
    client_identifier: &str,
    show_target: &GuidMatch<'_>,
    season: i64,
    episode: i64,
) -> anyhow::Result<Option<String>> {
    let sections = fetch_sections(base_url, token, client_identifier).await?;
    for section in sections.iter().filter(|s| s.section_type == "show") {
        let shows = fetch_section_items(base_url, token, client_identifier, &section.key).await?;
        let Some(show) = shows.iter().find(|s| guid_matches(&s.guid, show_target)) else {
            continue;
        };

        let leaves_url = format!("{}/library/metadata/{}/allLeaves", base_url.trim_end_matches('/'), show.rating_key);
        let res = identity_headers(http_client().get(&leaves_url), client_identifier)
            .header("X-Plex-Token", token)
            .send()
            .await?
            .error_for_status()?;
        let leaves: LibraryItemsResponse = res.json().await?;
        let matched = leaves.media_container.metadata.iter()
            .find(|e| e.parent_index == Some(season) && e.index == Some(episode))
            .map(|e| e.rating_key.clone());
        return Ok(matched);
    }
    Ok(None)
}

/// Marks an item watched on this server — Plex's own scrobble endpoint, the same one Sonarr/
/// Radarr/other tools use. Re-marking an already-watched item is a harmless no-op, which is why
/// the watch-sync engine doesn't bother excluding a title's origin server from this call.
pub async fn mark_watched(base_url: &str, token: &str, client_identifier: &str, rating_key: &str) -> anyhow::Result<()> {
    let url = format!("{}/:/scrobble", base_url.trim_end_matches('/'));
    identity_headers(http_client().get(&url), client_identifier)
        .header("X-Plex-Token", token)
        .query(&[("key", rating_key), ("identifier", "com.plexapp.plugins.library")])
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
