// src/api/search_routes.rs
//! Universal search (the web UI's Ctrl/Cmd+K command palette): active/seeding torrents across
//! fetcher nodes, Sonarr's and Radarr's own catalog lookup (which reports whether a title is
//! already in the library, same as their own "Add Series"/"Add Movie" search), and historical
//! Plex watch / Ombi request / grab records. Lidarr isn't wired in yet — `arr_nodes`/`lookup_all`
//! below are written so adding it is one more call site, not new plumbing.

use crate::auth::{AppState, RequireAuth};
use crate::config::ArrNodeConfig;
use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use utoipa::{IntoParams, ToSchema};

const TORRENT_LIMIT: usize = 12;
const HISTORY_LIMIT: usize = 8;
const ARR_LOOKUP_LIMIT: usize = 8;
/// Live upstream calls on every keystroke (debounced client-side) need to fail fast — a slow or
/// unreachable Sonarr/Radarr must not make the rest of the search hang with it.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(6);
const LOOKUP_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Deserialize, IntoParams)]
pub struct UniversalSearchQuery {
    pub q: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrKind {
    Sonarr,
    Radarr,
}

impl ArrKind {
    fn label(self) -> &'static str {
        match self {
            ArrKind::Sonarr => "sonarr",
            ArrKind::Radarr => "radarr",
        }
    }
    fn lookup_path(self) -> &'static str {
        match self {
            ArrKind::Sonarr => "/api/v3/series/lookup",
            ArrKind::Radarr => "/api/v3/movie/lookup",
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ArrLookupResult {
    /// "sonarr" | "radarr"
    pub source: String,
    pub node_name: String,
    /// Already added to this Sonarr/Radarr's library (its own lookup call reports this — a
    /// non-zero `id` means it matched an existing item), as opposed to a TheTVDB/TMDB catalog
    /// match that isn't in the library yet.
    pub in_library: bool,
    pub library_id: Option<i64>,
    pub title: String,
    pub year: Option<i64>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub tvdb_id: Option<i64>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub status: Option<String>,
    /// Sonarr-only nicety (e.g. "Netflix"); always `None` for a Radarr result.
    pub network: Option<String>,
    /// Deep link into that Sonarr/Radarr's own web UI: the library item's page when
    /// `in_library`, otherwise its "add new" search prefilled with this title — Conduit doesn't
    /// replicate the root-folder/quality-profile add flow, just gets you to it.
    pub open_url: String,
}

#[derive(Debug, Serialize, ToSchema, Default)]
pub struct UniversalSearchHistory {
    pub grabs: Vec<crate::db::ArrGrabRecord>,
    pub scrobbles: Vec<crate::db::PlexScrobbleRecord>,
    pub ombi_requests: Vec<crate::db::OmbiRequestRecord>,
}

#[derive(Debug, Serialize, ToSchema, Default)]
pub struct UniversalSearchResponse {
    pub query: String,
    pub torrents: Vec<crate::fetcher::UnifiedTorrent>,
    pub history: UniversalSearchHistory,
    pub sonarr: Vec<ArrLookupResult>,
    pub radarr: Vec<ArrLookupResult>,
    /// Non-fatal per-source failures (e.g. a Sonarr instance timed out) — the rest of the
    /// response is still valid, so the UI can show a small inline notice instead of nothing.
    pub warnings: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/api/search",
    tag = "Search",
    summary = "Universal search across fetchers, Sonarr/Radarr and history",
    description = "Searches active/seeding torrents by name, Sonarr's and Radarr's own catalog lookup (which reports whether a title is already in the library), and historical Plex watch / Ombi request / grab records.",
    params(UniversalSearchQuery),
    responses(
        (status = 200, description = "Combined search results", body = UniversalSearchResponse),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn universal_search(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Query(params): Query<UniversalSearchQuery>,
) -> Json<UniversalSearchResponse> {
    let q = params.q.trim();
    if q.is_empty() {
        return Json(UniversalSearchResponse {
            query: q.to_string(),
            ..Default::default()
        });
    }

    let config = state.config.get().await;

    // Only the active ones — "what's currently fetching X" shouldn't surface a torrent that
    // finished and was removed months ago (that's what the history/archive results are for).
    let torrents: Vec<_> = state
        .fetcher_pool
        .get_torrents(None, Some(q), None)
        .into_iter()
        .filter(|t| {
            !matches!(
                t.status,
                crate::fetcher::TorrentStatus::Stopped
                    | crate::fetcher::TorrentStatus::Deleted
                    | crate::fetcher::TorrentStatus::Unknown
            )
        })
        .take(TORRENT_LIMIT)
        .collect();

    let grabs = state
        .db
        .search_arr_grabs(Some(q), None, None, HISTORY_LIMIT)
        .unwrap_or_default();
    let scrobbles = state
        .db
        .search_plex_scrobbles(q, HISTORY_LIMIT)
        .unwrap_or_default();
    let ombi_requests = state
        .db
        .search_ombi_requests(q, HISTORY_LIMIT)
        .unwrap_or_default();

    let mut warnings = Vec::new();

    let sonarr_nodes = arr_nodes(
        config.sonarr.enabled,
        config.sonarr.primary.as_ref(),
        &config.zones,
        |z| z.sonarr.as_ref(),
    );
    let sonarr = lookup_all(&sonarr_nodes, q, ArrKind::Sonarr, &mut warnings).await;

    let radarr_nodes = arr_nodes(
        config.radarr.enabled,
        config.radarr.primary.as_ref(),
        &config.zones,
        |z| z.radarr.as_ref(),
    );
    let radarr = lookup_all(&radarr_nodes, q, ArrKind::Radarr, &mut warnings).await;

    Json(UniversalSearchResponse {
        query: q.to_string(),
        torrents,
        history: UniversalSearchHistory {
            grabs,
            scrobbles,
            ombi_requests,
        },
        sonarr,
        radarr,
        warnings,
    })
}

/// The distinct Arr node configs to query for one kind: the global primary (if that service is
/// enabled) plus every zone's own override, deduplicated by `base_url` (a zone left unset falls
/// back to the global instance, which is already included).
fn arr_nodes<'a>(
    global_enabled: bool,
    global_primary: Option<&'a ArrNodeConfig>,
    zones: &'a [crate::config::ZoneConfig],
    zone_node: impl Fn(&'a crate::config::ZoneConfig) -> Option<&'a ArrNodeConfig>,
) -> Vec<&'a ArrNodeConfig> {
    let mut nodes = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();

    if global_enabled {
        if let Some(n) = global_primary {
            if seen_urls.insert(n.base_url.trim_end_matches('/').to_lowercase()) {
                nodes.push(n);
            }
        }
    }
    for zone in zones {
        if let Some(n) = zone_node(zone) {
            if seen_urls.insert(n.base_url.trim_end_matches('/').to_lowercase()) {
                nodes.push(n);
            }
        }
    }
    nodes
}

async fn lookup_all(
    nodes: &[&ArrNodeConfig],
    term: &str,
    kind: ArrKind,
    warnings: &mut Vec<String>,
) -> Vec<ArrLookupResult> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let calls = nodes.iter().map(|n| lookup_one(n, term, kind));
    let per_node_results = futures_util::future::join_all(calls).await;

    let mut merged = Vec::new();
    let mut seen_ids: std::collections::HashSet<(Option<i64>, Option<i64>)> =
        std::collections::HashSet::new();
    for (node, result) in nodes.iter().zip(per_node_results) {
        match result {
            Ok(items) => {
                for item in items {
                    // A title matched by more than one configured instance (e.g. the same
                    // library mirrored to a replica) should only show up once.
                    if seen_ids.insert((item.tvdb_id, item.tmdb_id)) {
                        merged.push(item);
                    }
                }
            }
            Err(e) => warnings.push(format!("{} ({}): {e}", kind.label(), node.name)),
        }
        if merged.len() >= ARR_LOOKUP_LIMIT {
            break;
        }
    }
    merged.truncate(ARR_LOOKUP_LIMIT);
    merged
}

async fn lookup_one(
    node: &ArrNodeConfig,
    term: &str,
    kind: ArrKind,
) -> Result<Vec<ArrLookupResult>, String> {
    let client = reqwest::Client::builder()
        .timeout(LOOKUP_TIMEOUT)
        .connect_timeout(LOOKUP_CONNECT_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let base = node.base_url.trim_end_matches('/');

    let res = client
        .get(format!("{base}{}", kind.lookup_path()))
        .header("X-Api-Key", &node.api_key)
        .query(&[("term", term)])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status()));
    }

    let items: Vec<serde_json::Value> =
        res.json().await.map_err(|e| format!("bad response: {e}"))?;
    Ok(items
        .into_iter()
        .take(ARR_LOOKUP_LIMIT)
        .map(|v| map_lookup_result(&v, node, kind, base))
        .collect())
}

fn map_lookup_result(
    v: &serde_json::Value,
    node: &ArrNodeConfig,
    kind: ArrKind,
    base: &str,
) -> ArrLookupResult {
    let library_id = v.get("id").and_then(|i| i.as_i64()).filter(|id| *id > 0);
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("Untitled")
        .to_string();
    let tvdb_id = v.get("tvdbId").and_then(|t| t.as_i64());
    let tmdb_id = v.get("tmdbId").and_then(|t| t.as_i64());
    let title_slug = v.get("titleSlug").and_then(|t| t.as_str());

    let open_url = match (library_id, kind) {
        (Some(_), ArrKind::Sonarr) => title_slug
            .map(|s| format!("{base}/series/{s}"))
            .unwrap_or_else(|| format!("{base}/series")),
        (Some(_), ArrKind::Radarr) => tmdb_id
            .map(|id| format!("{base}/movie/{id}"))
            .unwrap_or_else(|| format!("{base}/movie")),
        (None, _) => format!("{base}/add/new?term={}", urlencoding_light(&title)),
    };

    ArrLookupResult {
        source: kind.label().to_string(),
        node_name: node.name.clone(),
        in_library: library_id.is_some(),
        library_id,
        title,
        year: v.get("year").and_then(|y| y.as_i64()),
        overview: v.get("overview").and_then(|o| o.as_str()).map(String::from),
        poster_url: crate::api::arr_routes::extract_poster_url(Some(v)),
        tvdb_id,
        tmdb_id,
        imdb_id: v.get("imdbId").and_then(|i| i.as_str()).map(String::from),
        status: v.get("status").and_then(|s| s.as_str()).map(String::from),
        network: (kind == ArrKind::Sonarr)
            .then(|| v.get("network").and_then(|n| n.as_str()).map(String::from))
            .flatten(),
        open_url,
    }
}

/// Just enough percent-encoding for a query-string value in a deep link — this is a display
/// convenience (prefilling Sonarr/Radarr's own add-search box), not a security boundary.
fn urlencoding_light(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, base_url: &str) -> ArrNodeConfig {
        ArrNodeConfig {
            name: name.to_string(),
            base_url: base_url.to_string(),
            api_key: "key".to_string(),
            base_path: None,
            version: 3,
        }
    }

    #[test]
    fn arr_nodes_includes_the_global_primary_and_zone_overrides_deduplicated() {
        let global = node("Sonarr", "http://sonarr.local:8989");
        let zone_a_node = node("Sonarr 4K", "http://sonarr-4k.local:8989");
        let zone_b_node = node("Sonarr (same as global)", "http://sonarr.local:8989/"); // trailing slash, same host

        let zones = vec![
            crate::config::ZoneConfig {
                id: "z1".into(),
                name: "4K".into(),
                sonarr: Some(zone_a_node.clone()),
                radarr: None,
                lidarr: None,
                plex_node_names: vec![],
                fetcher_node_names: vec![],
                webhook_secret: None,
            },
            crate::config::ZoneConfig {
                id: "z2".into(),
                name: "General".into(),
                sonarr: Some(zone_b_node),
                radarr: None,
                lidarr: None,
                plex_node_names: vec![],
                fetcher_node_names: vec![],
                webhook_secret: None,
            },
        ];

        let nodes = arr_nodes(true, Some(&global), &zones, |z| z.sonarr.as_ref());
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(
            names,
            ["Sonarr", "Sonarr 4K"],
            "the same URL is only queried once, trailing slash and all"
        );
    }

    #[test]
    fn a_disabled_global_service_contributes_no_nodes_but_zones_still_do() {
        let global = node("Sonarr", "http://sonarr.local:8989");
        let zone_node = node("Sonarr 4K", "http://sonarr-4k.local:8989");
        let zones = vec![crate::config::ZoneConfig {
            id: "z1".into(),
            name: "4K".into(),
            sonarr: Some(zone_node),
            radarr: None,
            lidarr: None,
            plex_node_names: vec![],
            fetcher_node_names: vec![],
            webhook_secret: None,
        }];

        let nodes = arr_nodes(false, Some(&global), &zones, |z| z.sonarr.as_ref());
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "Sonarr 4K");
    }

    #[test]
    fn library_matches_link_to_the_item_and_new_ones_link_to_add_search() {
        let n = node("Sonarr", "http://sonarr.local:8989/");
        let in_library = serde_json::json!({
            "id": 42, "title": "Severance", "titleSlug": "severance", "tvdbId": 371980, "year": 2022
        });
        let r = map_lookup_result(&in_library, &n, ArrKind::Sonarr, "http://sonarr.local:8989");
        assert!(r.in_library);
        assert_eq!(r.library_id, Some(42));
        assert_eq!(r.open_url, "http://sonarr.local:8989/series/severance");

        let not_in_library = serde_json::json!({
            "id": 0, "title": "Some New Show", "tvdbId": 999, "year": 2026
        });
        let r = map_lookup_result(
            &not_in_library,
            &n,
            ArrKind::Sonarr,
            "http://sonarr.local:8989",
        );
        assert!(!r.in_library);
        assert_eq!(r.library_id, None);
        assert_eq!(
            r.open_url,
            "http://sonarr.local:8989/add/new?term=Some+New+Show"
        );
    }

    #[test]
    fn radarr_results_never_carry_a_network_field() {
        let n = node("Radarr", "http://radarr.local:7878");
        let v = serde_json::json!({"id": 0, "title": "A Movie", "tmdbId": 123, "network": "should be ignored"});
        let r = map_lookup_result(&v, &n, ArrKind::Radarr, "http://radarr.local:7878");
        assert_eq!(r.network, None);
    }
}
