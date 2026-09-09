# Changelog

All notable changes to the **Conduit** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.15.0] - 2026-09-09

### Added

- **Circuit Breaker Capability Negotiation & Passive Mode (`fetcher-core`, `fetcher-synapse`, `src/fetcher/pool.rs`, `src/engines/circuit_breaker.rs`)**:
  - Conduit now auto-detects, per node, whether the backend daemon has its own native tracker circuit breaker (currently: Synapse's `tracker_circuit_breaker_v1`, via a new `GetCapabilities` gRPC RPC / `features` field on `GET /api/v1/health`), and switches that node's trackers into **passive mode** — Conduit stops managing them externally and instead mirrors the node's own breaker state into the existing dashboard/API surface (new `breaker_mode: "active" | "passive"` field on `TrackerHealthStatus`, alongside `cb_state` and `recovery_progress_pct`).
  - Passive-mode overrides: `POST /api/system/circuit-breakers/{host}/trip` and `/reset` proxy through to the owning node's native breaker.
  - Nodes without the capability (Transmission, qBittorrent, Deluge, or an older Synapse) keep running Conduit's own external breaker (**active mode**) exactly as before, unchanged behavior for anyone not on a capable Synapse build.
  - `TorrentClientTrait` gained `get_capabilities`, `list_circuit_breakers`, `force_trip_circuit_breaker`, and `force_reset_circuit_breaker`, all with no-op default implementations — only `fetcher-synapse` overrides them for now.
- **Active-Mode Circuit Breaker Ramp-Up & Backoff Doubling (`src/engines/circuit_breaker.rs`, `src/fetcher/models.rs`, `src/config/model.rs`)**:
  - Conduit's own (active-mode) tracker breaker no longer resumes every paused torrent the instant a canary probe succeeds — it now enters a `Recovering` ramp phase, staggering resumption across a configurable window (`tracker_circuit_breaker.recovery_ramp_secs`, default 30s) and requiring a minimum number of consecutive successful canary checks (`recovery_success_threshold`, default 5) before declaring the tracker fully healthy.
  - Any canary failure during `Recovering` immediately re-trips with doubled backoff (`initial_backoff_secs`, default 30s, capped at `max_tripped_secs`), mirroring Synapse's own fast-relapse-abort behavior.
  - `ActiveCircuitBreaker` gained `state`, `recovery_started_at`, `consecutive_successes`, and `backoff_secs` fields, persisted across restarts via new `circuit_breakers` table columns (idempotent `ALTER TABLE` migration).

## [0.14.7] - 2026-09-07

### Added
- **Queued Status Filtering & Dashboard Badges**:
  - Added dedicated 'Queued' status tab in the UI sidebar (`web/src/components/Sidebar.tsx`) with dynamic counter aggregation for queued/waiting downloads (`queued`, `queuedseed`, `checkwait`).
  - Added amber badge styling and metadata tags for queued items in `TorrentTable.tsx` and `TorrentDetailsModal.tsx`.
- **Modular Cargo Workspace Driver Crates**:
  - Refactored monolithic downloader/fetcher implementations into decoupled, reusable workspace crates:
    - `crates/fetcher-core`: Canonical traits (`TorrentClientTrait`), unified data models (`Torrent`, `TorrentStatus`, `TrackerStat`, `NodeStats`), error handling (`FetcherError`), and dynamic driver SPI registry (`FetcherRegistry`, `FetcherDriverFactory`).
    - `crates/fetcher-transmission`: Standalone Transmission JSON-RPC client adapter and driver factory.
    - `crates/fetcher-qbittorrent`: Standalone qBittorrent Web API client adapter and driver factory.
    - `crates/fetcher-deluge`: Standalone Deluge JSON-RPC client adapter and driver factory.
    - `crates/fetcher-synapse`: High-performance Synapse 2.0 streaming gRPC driver integrated with `synapse-client` SDK and push-driven `SynapseLiveCache`.
  - Configured feature flags (`transmission`, `qbittorrent`, `deluge`, `synapse`) at workspace root with 100% backward-compatible re-exports in `conduit::downloader` and `conduit::fetcher`.

## [0.14.6] - 2026-08-31

### Changed
- **Finished the Transmission→Fetcher identifier rename** started in 0.14.5's alias/accessor scaffolding, now that Synapse/Deluge/qBittorrent are first-class backends alongside Transmission:
  - `src/transmission/` → `src/fetcher/` (module directory rename; `src/downloader/transmission.rs`, the actual Transmission-RPC protocol adapter, is untouched — it's a sibling of `deluge.rs`/`qbittorrent.rs`/`synapse.rs` and genuinely protocol-specific).
  - `TransmissionPool` → `FetcherPool` is now the real struct name (the `FetcherPool`/`RetrieverPool` type aliases it used to hide behind are gone).
  - `TransmissionNodeConfig` → `FetcherNodeConfig` is now the real struct name (same — the alias is gone).
  - `AppState.tr_pool` → `AppState.fetcher_pool`, and every `tr_pool` local variable renamed to `fetcher_pool`.
  - `ZoneConfig.transmission_node_names` → `ZoneConfig.fetcher_node_names` is now the real field (`#[serde(rename = "fetcher_node_names", alias = "transmission_node_names")]` so existing saved configs using the old JSON key still parse — the now-redundant `fetcher_node_names()` accessor method, which had no callers, was removed in favor of direct field access); mirrored on the frontend (`ZoneConfig` TS interface, `Settings.tsx`, `App.tsx`) and in Flutter (`ZoneConfig.fetcherNodeNames` in `mobile/lib/core/models/zone.dart`, reading either JSON key).
  - Swagger doc strings and log/tracing messages that describe genuinely multi-backend behavior (`GET /api/nodes`, `GET/PUT /api/nodes/{name}/session`, `POST /api/nodes/{name}/test-port`, the torrent list/add/remove endpoints, the health monitor and poller engine's per-node log lines, the top-level OpenAPI description) now say "fetcher" instead of "Transmission" — verified against each backend's actual `get_session`/`set_session` trait impl (qBittorrent and Deluge both genuinely implement these; only Synapse doesn't, which the frontend already handles separately). Left untouched: `TransmissionClient`/`TransmissionAdapter` and their RPC-internal strings (genuinely Transmission-protocol-specific), the copy2queue hook-script generator and its `TR_*` env-var-shaped payload struct (Transmission's own `script-torrent-done-filename` mechanism), and the `"transmission_node"` DB event-log category string (left alone since renaming it would fragment historical event-log queries).
  - Full workspace build, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace` (all suites green), frontend `tsc --noEmit`, and `dart analyze` all pass after the rename.

## [0.14.5] - 2026-08-31

### Added
- **Configurable dashboard title** (`system.dashboard_title` in `AppConfig`, `src/config/model.rs`) — a per-deployment display name shown in the navbar wordmark and browser tab, editable from Conduit Settings → System & Networking. Defaults to `"Conduit"`. Exposed on the unauthenticated `GET /api/health` response (`dashboard_title` field) so it's available on the login/setup screens before a session exists, not just once logged in. The frontend (`web/src/App.tsx`) fetches it on mount and re-fetches on a `settings-saved` event dispatched by `Settings.tsx` after a successful save, so an admin editing it sees the navbar/tab title update immediately without a full page reload.

### Changed
- **`client_type` wired through `NodeStats`** (`src/transmission/models.rs`, `src/transmission/pool.rs`, `web/src/types/index.ts`) — all 4 `NodeStats` construction sites in `pool.rs` now populate it, letting the frontend distinguish backend types per-node instead of assuming Transmission everywhere.
- **`DaemonSettings.tsx` fixed for non-Transmission nodes**: was reading `n.config.name`, but `/api/nodes` returns a flat `NodeStats` with no `.config` sub-object — this would have thrown at runtime for any user opening "Configure Daemon RPC". Now reads `n.node`, and Synapse nodes get a "Not available for Synapse nodes" notice panel instead of the broken Transmission-RPC-shaped session form (the "Apply to Daemon" button is disabled too).
- **Removed remaining "Transmission"-branded copy from generic, multi-backend-capable UI text** now that Synapse/Deluge/qBittorrent are all supported alongside Transmission: page title, login subtitle, and setup-wizard copy no longer say "Transmission Commander" (now "Media Fetching Commander"); "Transmission nodes" in Settings/Zones prose, Swagger doc strings (`settings_routes.rs`, `metrics_routes.rs`, `arr_routes.rs`, `webhook_routes.rs`), and internal comments (`config/model.rs`, `api/mod.rs`) now say "fetcher nodes" to match the app's own established vocabulary (the "Fetchers" nav item, "Total Fetchers" stat, "Retriever Instances" settings heading). The setup wizard's node-connect step is still Transmission-RPC-shaped under the hood (it doesn't have a client-type selector yet), so its copy was reworded to "Connect First Fetcher Node" with an explicit note that Synapse/Deluge/qBittorrent nodes can be added afterward in Conduit Settings → Retrievers, rather than silently overclaiming multi-backend setup support.
- Fixed a handful of stray "Conduit" strings left over from an earlier, since-reversed rename attempt (SetupWizard's finish button, a DaemonSettings error message, a stale file-path comment in `Settings.tsx`, and three Swagger doc strings) — the product name stays **Conduit**.
- Flutter: added the `'synapse'` case to `clientDisplayName` in `mobile/lib/core/models/node.dart` (was falling through to "Transmission").

## [0.14.4] - 2026-08-31

### Changed
- **`SynapseClient` rewritten to be push-driven, not poll-driven (`src/downloader/synapse.rs`)**: previously `get_torrents`/`get_torrent_details*`/every bulk action opened a fresh `SubscribeTorrents` stream, pulled a snapshot, and closed it — a full-list network round trip on every single poll tick, the same "full list every refresh" pattern as the Transmission/qBittorrent/Deluge backends (which have no alternative, since their APIs are request/response only — Synapse's isn't). Now the client holds one long-lived `SubscribeTorrents` stream open for the node's entire lifetime in a background task, applying snapshot/added/updated/removed events to an in-memory cache as they arrive; every read (`get_torrents`, hash resolution for bulk actions, id/hash lookups) is now an instant in-memory read with **zero network calls** — measured at ~1.5µs/call in a live test, vs. what would otherwise be a gRPC round trip every poll cycle. Freshness is now bounded by synapse's own ~100ms push latency, not by conduit's poll interval. A genuinely unreachable node correctly reports as disconnected (errors from `get_torrents`) rather than silently serving stale cached data, and the background stream reconnects automatically with capped exponential backoff — both verified live: killing and restarting a real synapse process mid-test, confirmed the client detects the outage within seconds and reconnects on its own with no client restart needed. The background task is aborted on `Drop` so a config-triggered client replacement doesn't leak it.
- **Clippy `unnecessary_unwrap` fixed in `src/engines/pipeline.rs`** (3 occurrences, all the same `if COND && x.is_some() { let y = x.unwrap(); ... }` pattern) — was failing CI, which runs clippy with `-D warnings`. Replaced with `if let (true, Some(y)) = (COND, x) { ... }`, a minimal-diff fix that doesn't require reindenting the (long) existing bodies.
- **New clippy `enum_variant_names` lint on tonic-generated proto code silenced** (`src/downloader/synapse_proto.rs`) — `TorrentState`'s `State*` variants are dictated by the wire schema in `proto/synapse.proto`, not something this codebase controls; without `#[allow(clippy::enum_variant_names)]` on the generated module, CI's `-D warnings` would fail on every regen.

## [0.14.3] - 2026-08-31

### Fixed
- **Database / Cache Lock Inversion Deadlock (`src/db/mod.rs`)**:
  - Fixed a deadlock between SQLite `conn.lock()` and `arr_grabs_cache.write()` in `get_arr_grabs_lookup_map()` where holding `arr_grabs_cache.write()` before acquiring `conn.lock()` collided with `record_arr_grab()` / `clear_arr_grabs()`.
  - Enforced strict unidirectional lock ordering (`conn.lock()` released prior to updating cache).
- **Self-Contained Protobuf Compilation (`build.rs`, `Cargo.toml`, `.gitlab-ci.yml`)**:
  - Integrated `protoc-bin-vendored` into build dependencies so `tonic-build` / `prost-build` automatically resolves `protoc` on any platform without requiring system packages.
  - Added `protobuf-compiler` to GitLab CI test & lint stages for multi-tier redundancy.

## [0.14.2] - 2026-08-31

### Fixed
- **Docker Build Script & Protobuf Inclusion (`Dockerfile`)**:
  - Added `build.rs` and `proto/` directory to Docker context copying in `rust-builder` stage.
  - Installed `protobuf-compiler` in builder image to ensure `tonic-build` / `prost-build` compiles `synapse.proto` stubs cleanly in containerized CI environments.

## [0.14.1] - 2026-08-31

### Fixed & Improved
- **MediaReplacer Queue Blocklisting & Cutoff-Bypass (`src/engines/pipeline.rs`)**:
  - Added automatic query and blocklist deletion (`DELETE /api/v3/queue/{id}?removeFromClient=false&blocklist=true&skipRedownload=false`) across Sonarr, Radarr, and Lidarr when dead, corrupted, or unregistered torrents are detected.
  - Added library file purging (`DELETE /api/v3/episodefile/{fileId}` / `DELETE /api/v3/moviefile/{fileId}`) to clear Quality Profile Cutoff locks, resetting corrupted/trumped media to Missing & Monitored so automated searches are never skipped.
  - Added explicit backup search dispatch (`EpisodeSearch`, `MoviesSearch`, `AlbumSearch`, `ArtistSearch`) via Arr `/api/v3/command` endpoints.

### Added
- **Native Synapse 2.0 Retriever Integration (`src/downloader/synapse.rs`, `src/config/model.rs`)**:
  - Registered `RetrieverClientType::Synapse` across configuration models and node routing.

## [0.14.0] - 2026-08-31

### Added
- **Real Synapse gRPC client (`src/downloader/synapse.rs`, `src/downloader/synapse_proto.rs`, `proto/synapse.proto`)**: replaced the placeholder stub (`get_torrents` always returned empty, every mutating call no-opped to `Ok(())`) with a working Tonic client built against synapse's vendored `synapse.v2` proto (client-only codegen in `build.rs`, no dependency on synapse's own server-side crate). Derives a stable, deterministic torrent id from each hash instead of the positional-index convention `qbittorrent.rs`/`deluge.rs` use (which reshuffles `compound_id` across polls if a backend's list ordering changes); batch actions loop per-hash and collect failures rather than abandoning the rest of the batch on the first one; genuinely unsupported operations (no RPC exists in `synapse.v2` for reannounce, queue reordering, sequential-download toggle, rename, turtle mode, tracker replace, port test) no-op consistently with how `deluge.rs` already handles its own capability gaps, rather than either erroring loudly or silently pretending success. Live-verified end-to-end against a running `synapsed` instance (add/list/detail/start/stop/verify/remove, plus auth rejection with no/wrong token).

### Fixed
- **Every connected WebSocket client independently recomputed the shared telemetry snapshot (`src/api/ws.rs`)**: each connection ran its own 1s timer and re-ran the grab-map enrichment loop over every torrent — O(N·M) work per second across N clients. Pulled into a single supervised `engines::telemetry` publisher on the existing event bus (new `TOPIC_TELEMETRY`); new connections still get an instant first frame via a one-off snapshot rather than waiting up to 1s for the shared publisher's next tick.
- **Deluge bulk actions could report false success or mask real failures (`src/downloader/deluge.rs`)**: `verify_torrents`/`reannounce_torrents` silently swallowed per-torrent RPC errors; `remove_torrents` used `?` inside its per-hash loop, which abandoned every remaining torrent in the batch after the first failure while also reporting torrents that had *already* succeeded as failed. All three now attempt every resolved hash regardless of earlier failures and report an aggregate error only when something actually failed.

### Notes
- JWT-in-localStorage (flagged in `docs/AUDIT-2026-08-27.md`) remains open — the real fix is a migration to httpOnly cookies + CSRF, out of scope for this pass.
- `TorrentClientTrait` has no `set_file_priority`/`set_rate_limits` methods — even though synapse now supports both server-side, conduit has no way to drive them without widening the trait. Flagged as follow-up.

## [0.13.2] - 2026-08-30

### Inbound Webhook Reconciliation, Lifecycle Timeline, qBittorrent & Mobile Client Parity
- **Inbound Webhook Reconciliation (`src/api/arr_routes.rs` & `src/db/mod.rs`)**:
  - Unified handling for Sonarr `WebhookImportPayload` and `WebhookImportCompletePayload` (handling root `sourcePath`, `destinationPath`, and `episodeFiles[]`).
  - Fixed `record.id` overwrite bug on `Download` (import) webhooks, updating canonical grab records in place without creating duplicate orphan records.
  - Added filename stem extraction from `sourcePath` / `destinationPath` and series + season + episode matching (`find_arr_grab_by_series_episode`).
  - Added automated database self-healing on startup to reconcile past imported grabs.
  - Enhanced title and season/episode parser in `mediaParser.ts` to cleanly format series releases.
- **Visual Lifecycle Timeline (`src/api/torrent_routes.rs`)**:
  - Added `🏆 Ingest Completed & Live in Library` stage to `TorrentTimelineEvent`, displaying the complete lifecycle from Sniffed $\rightarrow$ Active Fetch $\rightarrow$ Staged & Hardlinked $\rightarrow$ Live in Library $\rightarrow$ Seeding.
- **Fetcher Daemons & qBittorrent Support**:
  - Added full qBittorrent Web API v2 support in `src/downloader/qbittorrent.rs` with automatic fallback handling across 4.x and 5.x endpoints (`resume`/`start`, `pause`/`stop`).
  - Refactored Web and Mobile UI to generic "Fetcher Daemons" terminology and multi-client tags.
  - Supported `fetcher_node_names` alongside `transmission_node_names` across unzoned and zoned cluster setups.
- **Mobile Client Feature Parity (`mobile/`)**:
  - Implemented interactive onboarding setup wizard with node skip options.
  - Added rich media detail bottom sheet and enriched media metadata across Dashboard, Pipeline, and Torrent Detail screens.
  - Resolved Torrent ID lookup and wired real-time cluster nodes and pipeline views.

## [0.13.1] - 2026-08-30

### Ghost Archive Deleted & Replaced Lifecycle Query Fix
- **Root Cause**:
  - `arr_grabs` maintains the current-state snapshot of each media entity. When releases were deleted and subsequently replaced/upgraded, the row's `status` progressed from `deleted` back to `imported` while the deletion was faithfully recorded in `arr_grab_history`.
  - In `search_arr_grabs` and `list_pipeline_items`, filtering by `status = "deleted"` or `status = "replaced"` previously executed a strict scalar equality match against `arr_grabs.status`, causing all historical deletions and replaced items that progressed to an upgrade to be excluded from the Ghost Archive view.
  - Additionally, `MovieDelete`, `SeriesDelete`, and `ArtistDelete` webhooks logged events and sent notifications but did not record `status = "deleted"` in `arr_grabs`.
- **Fixes Applied**:
  - **Comprehensive Status Search Matching**: Enhanced `search_arr_grabs` and `list_pipeline_items` so the `"deleted"` filter queries current deletions, delete event types (`MovieDelete`, `SeriesDelete`, `MovieFileDelete`, etc.), and historical deletion occurrences in `arr_grab_history`.
  - **Replaced Lifecycle Matching**: Updated `"replaced"` filter to match `re_searched = 1`, `re_search_count > 0`, `status IN ('replaced', 're-searched')`, and historical replacement events in `arr_grab_history`.
  - **Full Media Delete Persistence**: `handle_arr_media_deleted` and `handle_lidarr_deleted` now locate or create the corresponding `ArrGrabRecord` with `status: "deleted"` and append to history.
  - **UI Deleted Badge**: Added `DELETED` status badge support with `Trash2` icon in `ArchivedGrabsView.tsx`.

## [0.13.0] - 2026-08-30

### Multi-Daemon Retriever Support & Interactive Pipeline Control (1.0 Milestone Roadmap Sections 1 & 2)

- **Section 1: Multi-Daemon Retriever Support (qBittorrent, Deluge, Cross-Node Migration)**:
  - **Unified `TorrentClientTrait` Abstraction**:
    - Created `src/downloader/traits.rs` with `TorrentClientTrait` providing a unified interface across all torrent daemon backends.
    - Implemented `TransmissionAdapter` (`src/downloader/transmission.rs`) wrapping Transmission RPC.
    - Implemented `QBittorrentClient` (`src/downloader/qbittorrent.rs`) supporting qBittorrent Web API v2 (`auth/login`, `torrents/info`, `torrents/resume`, `torrents/pause`, `torrents/delete`, `torrents/recheck`, `torrents/reannounce`, `torrents/setLocation`, `torrents/add`, `sync/maindata`, `torrents/files`, `torrents/pieceStates`, `torrents/editTracker`).
    - Implemented `DelugeClient` (`src/downloader/deluge.rs`) supporting Deluge Web JSON-RPC (`auth.login`, `core.get_torrents_status`, `core.pause_torrents`, `core.resume_torrents`, `core.remove_torrent`, `core.add_torrent_magnet`, `core.add_torrent_file`, `core.get_free_space`).
    - Added dynamic client factory `create_client` in `src/downloader/mod.rs` and updated `TransmissionPool` to manage heterogeneous `Arc<dyn TorrentClientTrait>`.
  - **Cross-Node Torrent Migration**:
    - Added `POST /api/torrents/migrate` supporting one-click transfers between any cluster nodes and client backends (Transmission, qBittorrent, Deluge) with magnet reconstruction, directory remapping, and source deletion options.
    - Added `migrateTorrent` frontend API client.
  - **Retriever Configuration UI**:
    - Updated `web/src/pages/Settings.tsx` to add a Daemon Type selector (`Transmission`, `qBittorrent`, `Deluge`) with automatic port and RPC path defaults.

- **Section 2: Interactive Media Pipeline & Arr Control (Release Search/Grab, Bazarr, Overseerr, Jellyfin)**:
  - **Interactive Indexer Release Search & Manual Grab**:
    - Added `GET /api/arr/search/releases` and `POST /api/arr/search/grab` in `src/api/arr_routes.rs` querying Sonarr, Radarr, and Lidarr indexers directly.
    - Added "Interactive Release Search" tab in `MediaDetailModal.tsx` allowing operators to query indexers, view quality/indexer/custom-format scores/rejections, and execute 1-click manual release grabs directly within Conduit.
  - **Bazarr Subtitle Webhook Ingestion**:
    - Added `POST /api/bazarr/inbound` (and `/api/webhook/bazarr`) in `src/api/webhook_routes.rs` with automatic event logging and notification dispatch (`bazarr.download`).
  - **Overseerr / Jellyseerr Webhook Ingestion**:
    - Added `POST /api/overseerr/inbound` (and `/api/webhook/overseerr`) in `src/api/webhook_routes.rs` saving user media requests directly into the Kibble Bowl pipeline.
  - **Jellyfin / Emby Webhook Ingestion**:
    - Added `POST /api/jellyfin/inbound` (and `/api/webhook/jellyfin`) in `src/api/webhook_routes.rs` tracking library additions and scrobbles.

## [0.12.2] - 2026-08-30

### Arr Poller Resiliency & Health Monitor False-Alarm Fix
- **Root Cause of Flapping Health Alerts**:
  - The Arr telemetry poller (`fetch_radarr_live_stats`, `fetch_sonarr_live_stats`, `fetch_lidarr_live_stats`) used an overly aggressive 4-second HTTP timeout when querying full library arrays (`/api/v3/movie`, `/api/v3/series`, `/api/v1/artist`). On large libraries, a transient delay in Radarr/Sonarr serializing the library caused `reqwest` to time out and reject the entire telemetry structure with `error sending request for url (http://...:7878/api/v3/movie)`.
  - The health monitor interpreted any error returned by the poller as total daemon unreachability, triggering a false `🚨 Conduit Health Alert` followed 30 seconds later by a `✅ Conduit Health Restored` notification.
- **Fixes Applied**:
  - **Increased Timeout & Connect Timeout**: Expanded HTTP timeout to `15s` with `5s` connect timeout across Sonarr, Radarr, and Lidarr telemetry pollers.
  - **Differentiated Health from Heavyweight Queries**: System status (`/api/v3/system/status`) remains the authoritative connectivity and daemon reachability check. Secondary statistics queries (movie array, series array, queue, wanted, health checks, diskspace) now fail gracefully to cached/empty defaults if delayed, logging debug messages without marking the entire service offline or flapping health monitor notifications.

## [0.12.1] - 2026-08-30

### Ombi Living Card Chaining & Notification Categorization Audit
- **Ombi Request & Media Added Living Card Unification**:
  - Fixed an issue where Ombi requests and Sonarr/Radarr `MovieAdded`/`SeriesAdd`/`ArtistAdd` webhooks produced disconnected standalone notification cards instead of linking into the pipeline's single Living Card.
  - In `handle_ombi_inbound_direct`, incoming Ombi requests now immediately seed an `ArrGrabRecord` with initial `requested` status and capture the Mattermost `mattermost_post_id`.
  - In `handle_arr_media_added` and `handle_lidarr_artist_added`, incoming media additions check for existing Ombi request records or grab entities, dispatch in-place card updates or thread notes using `dispatch_rich_or_update`, and persist `mattermost_post_id` into `ArrGrabRecord`.
  - When Sonarr/Radarr/Lidarr subsequently grab a release, the grab handler locates the existing grab record and updates the *same* living card seamlessly from Request → Tracking → Grabbed → Staged → Retrieved.
- **Notification Event Categorization Audit**:
  - Overhauled `categorize_event` in `src/notify/mod.rs` to comprehensively categorize all event types (`*.added`, `ombi.*`, `*.delete`, `*.health`, `*.update`, `*.manual`, `*.rename`, `torrent.started`, `torrent.deleted`, `torrent.enriched`) across the 8 notification categories (`grab`, `download`, `replacement`, `health`, `error`, `autopurge`, `sync`, `scrobble`), eliminating erroneous fallthrough into the `"error"` category.
- **Conduit 1.0 Milestone Roadmap**:
  - Conducted full architecture audit and authored `TODO.md` outlining roadmap items for the upcoming 1.0 release (qBittorrent/Deluge adapters, interactive search release picker, Bazarr integration, zone storage quotas, mobile push alerts, and OIDC SSO).

## [0.12.0] - 2026-08-30

### Trakt-Backed Multi-Server Plex Watch-Status Sync
- **Fixed a real gap**: `export_backup` UI promised "Passphrase (Optional) — Leave empty for unencrypted JSON," but the route handler has always required an 8+ character passphrase and never actually takes the unencrypted path — leaving it blank threw a real, specific 400 that `exportBackup()` then discarded in favor of a generic "Failed to export backup" message. Fixed both the UI copy and the swallowed error.
- **New: pick which Plex servers participate** in Trakt-backed watch-status sync — each `PlexNodeConfig` gains an "Include in Trakt-backed watch-status sync" toggle. `engines::watch_sync` now tags every incoming scrobble with its originating server (matched via the webhook's `Server.uuid`/`title` — previously scrobbles had no server-origin tracking at all) and only pushes scrobbles from opted-in servers to Trakt.
- **New: optional push-back to Plex** (`trakt.sync_watched_back_to_plex`, off by default and separate from the push-to-Trakt toggle above since it's the riskier direction) — Conduit now reads Trakt's `/sync/watched/movies` and `/sync/watched/shows`, and for anything watched since the last check, searches every participating server's own library for a matching item by IMDb/TMDb/TVDb ID and marks it watched there too. A server not owning a given title is logged as informational, never an error.
- New `src/trakt_client.rs` (watched-history reads) and additions to `src/plex_client.rs` (server identity detection, library GUID search, mark-watched) supporting the above.
- Known limitation, called out deliberately rather than hidden: GUID matching depends on both servers' metadata agents having resolved the same external ids — reliable on modern Plex agents, less so on older/legacy-agent libraries.

## [0.11.0] - 2026-08-29

### Notification Targets Settings UI
Frontend follow-up to 0.10.0's multi-target notification backend — the Notifications settings tab is fully rebuilt around the new model.

- Replaced the fixed Mattermost/Discord/Webhook config blocks and the global event-category checkboxes with a dynamic **Notification Targets** list: add any number of Mattermost/Discord/Pushover/Webhook targets, each with its own connection fields, an 8-category checkbox grid (grab/download/replacement/health/error/autopurge/sync/scrobble), an enabled toggle, and — when zones are configured — a zone-scope dropdown.
- Pushover gets its own field set (user key, API token, device, sound); Webhook targets get a Summary/Full payload-mode selector.
- "Send Test" now dispatches to one specific target by id instead of a bare channel-type string, matching the backend's `target_id`-based `POST /api/notifications/test`.
- Verified end-to-end against a real running instance: added a Pushover target, toggled its Health Alerts category, saved, reloaded the page, and confirmed both the credentials and category selection persisted through a real settings save/reload round trip; triggered a real `health.alert` event (via an intentionally-unreachable Transmission node) and confirmed it dispatched through the new per-target pipeline without error.

## [0.10.0] - 2026-08-29

### Full Grab/Import/Delete/Re-search Lineage (Media Item History)
- **Root cause fixed**: `arr_grabs` is a current-state table — "canonical entity reconciliation" in `save_arr_grab` deliberately collapses every grab/import/delete/re-search event for the same movie/series (matched by `movie_id`/`series_id`/`download_id`/scene name) onto one row, so the Pipeline/Fetchers/Ghost Archive views show one card per item rather than splitting on every re-fetch. The side effect: once an item was trumped and re-grabbed, its original import's details — and the fact a delete ever happened — were silently overwritten with no trace.
- **New: append-only `arr_grab_history` table** — every write to `arr_grabs` (via `save_arr_grab` or `mark_grab_status`, which covers every webhook handler, the re-search endpoint, and the MediaReplacer engine) now also appends one timeline row keyed by the same canonical grab id, recording event type, status, release detail, and timestamp without overwriting anything.
- **New `GET /api/arr/pipeline/{id}/history`** endpoint and a `GrabHistoryTimeline` component surfacing it in both the Ghost Archive's expanded row and the Pipeline Browser's Media Detail modal ("Recorded Event History", alongside the existing derived stage-overview stepper) — a trumped-and-replaced item now shows its full grabbed → imported → deleted → re-searched → re-imported history, not just its current status.
- Extended `test_arr_grab_lifecycle_and_lookup` to assert the full 3-event lineage (Grab → StatusChange → ReSearch) survives even though `arr_grabs` itself only ever reflects the final "replaced" state.

## [0.9.1] - 2026-08-29

### Long-Range Bandwidth History (up to 72h)
- **New: SQLite-backed downsampled bandwidth history** — the live Dashboard chart's in-memory buffer is still exactly 5 minutes at 1-second resolution (unchanged, no performance impact), but a new `bandwidth_history_downsampled` table now also persists a 1-minute average (smoothed from that same buffer) every minute, pruned to a 72h rolling window (~4320 rows). New `GET /api/torrents/bandwidth-history?hours=N` endpoint (N clamped to 1-72) serves it.
- **Dashboard**: the bandwidth chart now has a range selector (Live 5m / 3h / 12h / 24h / 72h) — anything past "Live 5m" fetches from the new endpoint and refreshes every 60s (matching its actual resolution). Per-torrent contributor breakdown in the hover tooltip is only available on the live 5-minute view, since the downsampled tier doesn't track it.
- Deliberately does not add a general historical query/chart engine against InfluxDB — for weeks/months-scale analysis, Grafana/Chronograf against the existing `influx_pusher` export remains the recommended path; this pass only covers the "just let me see more than 5 minutes without needing Influx" gap.

## [0.9.0] - 2026-08-29

### Peer IP Enrichment, Settings Reorganization, and Network/Influx Visibility
- **New: opt-in peer IP enrichment via iptoasn.com** (`src/ip_asn.rs`, `src/engines/ip_asn_updater.rs`) — shows each peer's country and network operator (AS description) in the torrent detail view. Deliberately built against iptoasn.com's free `ip2asn-combined.tsv.gz` instead of MaxMind GeoLite2, which requires an account/license. A new background engine downloads and refreshes the ~9MB gzipped database weekly, parsing it into an in-memory sorted-range lookup (~717K ranges, ~50-60MB resident) — off by default given that memory cost, toggleable in Settings → System & Network. `TorrentPeer.country_code` (previously always empty — Transmission's own RPC never actually populates it) and a new `as_name` field are now filled in by `get_torrent`/`enrich_torrent` when enabled.
- **Settings reorg**: "Nodes & Transmission" tab renamed to **Retrievers**; the Tracker Circuit Breaker & Swarm Pressure Relief settings moved there from System & Network, since they're specific to Transmission nodes rather than general system config.
- **New: Network Listener info card** in System & Network — shows the actual effective `http(s)://` and `ws(s)://` scheme/address/port Conduit is listening on, and clarifies that WebSocket traffic automatically shares the same native TLS termination as the HTTP API (no separate WS/WSS port or config exists, and a reverse proxy in front remains fully supported by simply leaving Conduit's own HTTPS disabled).
- **New: InfluxDB v2 Settings UI** — the `influx_pusher` engine (per-node bandwidth/torrent-count export every 30s) has existed since a previous pass but had no configuration UI at all; Settings → System & Network now has a full section to enable/configure it (host, port, org, bucket, token, TLS).

## [0.8.0] - 2026-08-29

### Multi-Tenant "Zones" — Combined + Per-Stack Views
Conduit can now represent fully independent stacks (e.g. a "General Library" and a separate "4K Library") sharing the same instance, following up on the zones architecture scoped in 0.7.0's planning notes.

- **New: `zones` config** (`AppConfig.zones`) — each zone groups one Sonarr/Radarr/Lidarr instance with a subset of Plex servers and Transmission nodes. Purely additive: `zones` defaults to empty, so a single-instance setup sees zero behavior change.
- **Zone-aware webhook routing**: the Sonarr/Radarr/Lidarr inbound webhooks accept an optional `?zone=<id>` query param — when it matches a configured zone, that zone's Arr instance/webhook secret is used for auth and the resulting grab is tagged with `zone_id`; absent or unmatched falls back to today's global-secret behavior exactly as before. Plex notify-takeover now only notifies a zone's own Plex server(s) once a zone is known, instead of every configured Plex node.
- **New `zone_id` column** on `arr_grabs`, with `zone` filter support on `GET /api/arr/pipeline` and `GET /api/arr/grabs`.
- **Frontend zone switcher**: a new "Zones" section in the sidebar (All Zones / one button per zone) filters the Dashboard, Pipeline Browser, Fetchers, and Ghost Archive views by the zone's Transmission nodes/grabs — hidden entirely with no zones configured. New "Zones" tab in Settings to add/edit/delete zones, configure each zone's Sonarr/Radarr/Lidarr connection, select its Plex servers and Transmission nodes, and copy its zone-scoped webhook URLs.
- **Completeness pass**: extended the `zone` filter to `GET /api/arr/grabs`/`search_arr_grabs` (previously only `GET /api/arr/pipeline` had it, leaving the Ghost Archive view zone-blind); added `zone_id` to the frontend `ArrGrabRecord` type; the Dashboard's activity feed now filters by the active zone client-side, with an explicit "not zone-scoped" callout on the still-global Arr stats cards; documented zone-scoped webhook routing in `docs/webhook_reference.md` §6.5 and refreshed the README's zones description (previously described it as "on the roadmap").
- **Fixed: Plex OAuth "Add Discovered Server" picked an unreachable connection** — it silently used whichever connection Plex's `/api/v2/resources` marked `local: true`, which reflects what the Plex Media Server itself considers local, not what's reachable from wherever Conduit runs (e.g. a containerized Plex server can advertise its own container-internal IP). Settings now shows every discovered connection in a dropdown plus an editable URL field, so a genuinely unreachable auto-pick can be overridden before adding the server.
- **Per-zone live Arr telemetry, closing the one deferred item from the initial zones pass**: `engines::arr_stats_poller` now computes and caches one `ArrStatsResponse` per configured zone (its own Sonarr/Radarr/Lidarr instance, its own tagged grabs) alongside the existing global snapshot, on the same 30s cycle. `GET /api/arr/stats?zone=<id>` returns a zone's own stats; the Dashboard's Sonarr/Radarr cards now show the active zone's instance name and numbers (polled every 30s while a zone is selected, since the `arr_stats` WebSocket topic itself stays global-only) instead of a "not zone-scoped" disclaimer.
- Added `artist_id`/`album_id` to the frontend `ArrGrabRecord` type — present in the API response since the Lidarr re-search fix but missing from the TS type, same class of gap as the `zone_id` fix above.

## [0.7.0] - 2026-08-29

### Plex OAuth Takeover, API/Swagger Completeness, and Publish-Readiness Pass
Conduit now takes over Sonarr/Radarr's built-in "notify Plex" responsibility using a proper account-linked OAuth flow instead of a pasted token, plus a full audit and cleanup of the REST API's Swagger documentation, a public-facing README rewrite, and a pre-publish security review of the git history.

- **New: Plex OAuth account linking** (`src/plex_client.rs`, `src/api/plex_oauth_routes.rs`) — a PIN-based device-linking flow against plex.tv, modeled directly on Sonarr's and Ombi's own implementations (`POST /api/plex/oauth/pin` → user approves at `app.plex.tv` → `GET /api/plex/oauth/poll/{pin_id}` → discovered servers via plex.tv's resource listing → `POST /api/plex/oauth/add-server`). Registers Conduit as its own stable device via a `client_identifier` UUID generated once at startup. New "Connect Plex Account" button in Settings replaces manual `X-Plex-Token` copying for owned servers (manual entry remains available for shared/non-owned servers).
- **New: Sonarr/Radarr → Plex notify-takeover** — on every import webhook, Conduit itself now triggers a *targeted* Plex library section refresh (`GET /library/sections/{id}/refresh?path=...`) for just the affected show/movie folder, replicating exactly what Sonarr/Radarr's own built-in "Plex Media Server" connection does (enumerate sections, match by root folder path, partial rescan — not a full library refresh). Gated behind a new `plex.notify_on_import` setting (on by default); Settings UI now prompts to disable Sonarr/Radarr's own Plex connection to avoid double-triggering scans.
- **API/Swagger completeness audit and fixes**: registered `GET /api/system/engines` (the v0.6.0 Platform Health endpoint) in the OpenAPI spec — it had a complete annotation but was never added to the paths list, so it was invisible in Swagger UI despite looking documented; added a full protocol description for `GET /api/ws`'s subscribe/channel WebSocket messages (previously undocumented entirely); reconciled inconsistent tags (`"System Logs"`/`"Transmission Nodes"` folded into `"System"`/`"Nodes"`; `"File Staging"` and `"Integrations"` now properly declared); `info.version` now reads from `Cargo.toml` via `env!("CARGO_PKG_VERSION")` instead of a hardcoded string that had drifted three versions stale; added `contact`/`license` metadata; added real error-response documentation (401/500, and clarified which "test connection" endpoints report failure as `success: false` in a 200 body rather than an HTTP error) to the highest-value previously-200-only endpoints: the three Arr webhook receivers and the Plex/Trakt/Arr "test connection" endpoints.
- **README rewrite**: replaced the stale pre-rebrand branding (the project had been renamed to Conduit back in v0.5.0 but the README was never updated) with a public-facing overview — what Conduit is, a feature comparison against transgui, current daemon/integration support status (Transmission fully supported; qBittorrent/Deluge researched but not yet built, per `docs/tracker_support.md`), and a refreshed architecture diagram including the v0.6.0 WebSocket channel system and Platform Health, plus the new Plex OAuth/notify-takeover flow. Added a proper `LICENSE` file (MIT, as the README already claimed with none actually present).
- **Pre-publish git history security audit**: scanned all 82 commits — no credentials, API keys, or private key material were ever committed. The only exposure was internal identifying info (an internal domain name, and `.gitlab-ci.yml`'s internal Docker deploy host plus two unrelated private GitLab project references) — nothing requiring history rewrite/squash, which would be destructive for no security benefit here. `.gitlab-ci.yml` was deliberately left as-is pending the user's own review, since it may still be load-bearing for their actual deployment pipeline.
- **Documented, not yet built**: a multi-tenant "zones" architecture (independent Sonarr/Radarr/Plex/Transmission stacks — e.g. a general library and a separate 4K library — with combined and per-zone views) was scoped in detail as a future initiative; see the session's planning notes for the recommended config/webhook/DB/UI build order.

## [0.6.0] - 2026-08-29

### Unified Background Task Registry & WebSocket Channel System
A full audit of every outbound integration and background engine, following up on the `/api/arr/stats` caching fix in 0.5.8, plus a new modular real-time push architecture replacing several REST polling loops.

**Backend performance fixes found in the audit:**
- `health_monitor.rs` and the new `arr_stats_poller` were independently polling the same Sonarr/Radarr/Lidarr `system/status` endpoints on two uncoordinated 30s timers. Consolidated: `health_monitor` now reads Sonarr/Radarr/Lidarr connectivity from the `arr_stats_poller`'s own cache (via new `sonarr_error`/`radarr_error`/`lidarr_error` fields on `ArrStatsResponse`) instead of re-polling those endpoints itself.
- `pipeline.rs` was building a fresh `reqwest::Client::new()` per broken torrent found, **with no timeout configured at all** — a dead Sonarr/Radarr could hang the whole engine loop indefinitely. Now built once at startup with a 10s timeout.
- `arr_sync.rs` was building a new `reqwest::Client` per replica per sync cycle (no connection reuse). Now built once per cycle and shared across replicas.
- `watch_sync.rs` was sending **one Trakt API call per unsynced Plex scrobble**, even though Trakt's `/sync/history` endpoint natively accepts a batch of movies/episodes in one call. Now batches all pending scrobbles into a single request per cycle.

**New: Platform Health — background engine/task visibility**
- New `engines::registry` module tracks every background engine (poller, space manager, pipeline, circuit breaker, file sync, arr sync, watch sync, influx pusher, health monitor, arr stats poller): current state (running/degraded/crashed/starting), last heartbeat, cycle count, and restart count. The existing panic-catching supervisor now feeds this registry on every crash/restart.
- New `GET /api/system/engines` endpoint and a new "Platform Health" tab in the Dashboard's health panel, showing every engine's live status — separate from the existing Arr-stack and Transmission-node health views.

**New: modular WebSocket channel system**
- Replaced Redis-as-a-consideration with a simple in-process `tokio::sync::broadcast` bus (`src/events.rs`) — Conduit is a single self-hosted binary, so an external pub/sub service would add deployment complexity for no capability actually needed here.
- The WebSocket endpoint now supports topic subscriptions (`{"type":"subscribe","topics":[...]}`) alongside the original always-on `telemetry` stream (unchanged, for backward compatibility). Four new topics: `events` and `pipeline` publish the instant a row is written to the database (`Database::log_event`/`save_arr_grab`, no polling involved — this data was never slow, just polled needlessly); `arr_stats` and `platform_health` publish at the end of their respective 30s/5s background poller cycles.
- The Dashboard, Pipeline Browser, and Events Log pages no longer poll their REST endpoints on a blind 5-second timer — they now update live from the WebSocket push (debounced where server-side filters mean a raw re-fetch is still needed), falling back to a slow safety-net poll only while the WebSocket is disconnected. Also fixes a loading-spinner flicker in the Pipeline Browser that was re-triggering on every poll tick.

## [0.5.8] - 2026-08-29

### Settings UX & Arr Stats Performance
- **Settings**: renamed the "Sonarr & Radarr" tab to "Arr Stack" (it already covers Lidarr too) and added the same inbound webhook URL card Lidarr had to Sonarr and Radarr, so all three `*arr` apps show a consistent, copyable webhook URL.
- **Fixed a stale UI default**: the Post-Staging Command field's placeholder/help text still said `cp -alv` (hardlink) was the default, but the backend default was changed to `cp -av` (copy) back in 0.5.4 to avoid SMB/CIFS lease collisions — the UI now matches, and clarifies that the command is read live on every hook execution (no re-install needed on any node to change it).
- **`GET /api/arr/stats` is now cached** — this endpoint was making ~15 sequential live HTTP calls to Sonarr/Radarr/Lidarr (6 calls per app: system status, library, queue, wanted/missing, health, diskspace) on every single request, which is what made it take multiple seconds to load on the dashboard. A new background poller (`engines::arr_stats_poller`, 30s cycle) now refreshes a shared cache, and the endpoint just serves it instantly; live computation only happens as a one-time fallback on a cold cache immediately after startup.
- Verified (no changes needed): the Lidarr re-search/routing changes from 0.5.6, the Space Manager's free-space check is already correctly evaluated per-node against each node's own threshold (never aggregated across nodes) before running its ratio/age/seeder-count purge criteria, and the main torrent dashboard already prefers the WebSocket telemetry stream over REST polling (with a slow 30s safety-net poll only while disconnected).

## [0.5.7] - 2026-08-29

### Torrent Detail Page: Seeding & Upload Telemetry
- **Active Seeding Hero Banner**:
  - Added a dedicated Seeding & Upload Hero banner when torrents are seeding or complete (`status === 'seeding'` or 100% done).
  - Prominently displays Share Ratio (`4.82x`), Total Uploaded vs Swarm Size, Current Upload Speed, Active Seeding Leechers count, Total Seeding Duration (`⏱️ Seed Time`), and a dynamic high-contrast ratio progress bar toward 2.0x target.
- **Comprehensive 4-Card Telemetry Grid**:
  - Added dedicated **Seeding & Upload Telemetry** card showcasing Share Ratio (with qualitative status badge `🎯 Target Met` / `🟢 Positive` / `🟡 Seeding`), Total Uploaded, Upload Speed, Uploading To (Leechers), Seeding Time, and Payload Ratio.
  - Upgraded **Intake & Download Telemetry**, **Swarm & Peer Dynamics**, and **Lifecycle & Activity Dates** cards with rich metrics (downloaded ever, piece specs, seeder/leecher breakdown, completion dates, and time since last swarm activity).
- **Mobile Torrent Details**:
  - Added Share Ratio, Total Uploaded, Total Downloaded, Upload/Download Speeds, Seeding Duration, and Leechers/Seeders breakdown to the Flutter mobile app transfer telemetry view.

## [0.5.6] - 2026-08-29

### Lidarr, Plex & Ombi Webhook Ingestion Fixes
Extends the Sonarr/Radarr webhook rebuild in 0.5.5 to the remaining three integrations, sourced directly from vendored Lidarr/Ombi source and Plex's official webhook documentation (see `docs/webhook_reference.md`).

- **Lidarr**:
  - Added dedicated handling for `ArtistAdd`, `ArtistDelete`/`AlbumDelete`, and `DownloadFailure`/`ImportFailure` (Lidarr's equivalent of Sonarr/Radarr's `ManualInteractionRequired` — previously dropped entirely).
  - Removed two dead event-type match arms (`AlbumDownload`, `TrackFileDelete`) that don't exist in Lidarr's real webhook protocol.
  - Fixed `Download` event parsing to read the real `trackFiles[]`/`tracks[]` arrays instead of a `release` object Lidarr's Download payload never actually sends.
  - Added `artist_id`/`album_id` columns to the grab-history table so Lidarr re-search can target the real album/artist instead of silently POSTing an empty `AlbumSearch` command that searched nothing.
  - Fixed the Lidarr artist/album lookup used by the torrent auto-enrich feature, which was reading fields at the wrong JSON nesting level (Lidarr's combined search endpoint nests artist/album data one level deeper than assumed).
- **Ombi**: fixed an auth bug where the webhook secret check never recognized Ombi's actual `Access-Token` header; fixed a status bug where every declined request was silently stored as `"pending"` because the code checked for `"requestdenied"` instead of Ombi's real `RequestDeclined` value; fixed poster/external-ID extraction to use Ombi's real `posterImage`/`providerId` fields instead of field names Ombi never sends; normalized `media_type` from Ombi's humanized `"TV Show"` string; surfaced `denyReason` on declined requests and added dedicated notification titles for partial availability and issue-tracker events.
- **Plex**: added an always-on alert for `admin.database.corrupted` (previously Plex's highest-severity event produced no notification at all) and a new `media.rate` notification surfacing the user's rating and the item's plot summary.

## [0.5.5] - 2026-08-29

### Sonarr & Radarr Webhook Ingestion & System Event Routing
- **Non-Media System Event Routing**:
  - `Health` and `HealthRestored` events (such as `RemovedMovieCheck`, `IndexerStatusCheck`, `DownloadClientCheck`) are now routed to system event logs and rich alert notifications rather than creating dummy/phantom grab records with "Unknown Movie" / "Unknown Series".
  - `ApplicationUpdate` events now dispatch dedicated upgrade announcements (`🚀 Radarr/Sonarr Updated to v...`) with version comparisons.
  - `ManualInteractionRequired` events extract and format human-readable error reasons from `downloadStatusMessages` (e.g. sample releases, missing preferred word upgrades) and dispatch high-priority actionable alerts (`🐕 Conduit Needs Help`).
- **Media Lifecycle & Batch Handling**:
  - `SeriesAdd` / `MovieAdded`: Added dedicated library tracking notifications with synopsis and add method details.
  - `SeriesDelete` / `MovieDelete`: Added removal tracking with disk cleanup and freed storage metrics.
  - `Rename`: Added dedicated file rename tracking without creating false media grabs.
  - Batch Season Packs (`episodeFiles`): Added full multi-file aggregation for Sonarr season pack imports.
  - Rich Media Details: Extracted resolution, video codec, dynamic range (HDR10 / Dolby Vision), audio codec, and channel count from `mediaInfo`.
  - Added extraction for indexer flags (Freeleech, Scene, Internal) and custom formats with scores.
  - Differentiated file delete events by `deleteReason` (`Upgrade`, `Manual`, `MissingFromDisk`).

## [0.5.4] - 2026-08-29

### CIFS Staging & Hardlink Collision Resolution
- **SMB Server-Side Copy & Lease Collision Fix**:
  - Resolved `[Errno 22] Invalid argument` / `STATUS_INVALID_PARAMETER` on CIFS network shares by switching default staging command from hardlink (`cp -alv`) to copy (`cp -av`).
  - Added `nolease` to CIFS mount configurations in `/etc/fstab` to eliminate SMB lease/oplock collisions when files are open concurrently for seeding in Transmission and imported by Sonarr/Radarr.
  - Re-linked and verified existing queued media files in `tvQueue`.

## [0.5.3] - 2026-08-28

### Web UI & Chart Tooltip Enhancements
- **Click-to-Pin Bandwidth Snapshot**:
  - Clicking anywhere on the 5-minute throughput chart locks the active tooltip snapshot in place, enabling smooth scrolling through all active swarms.
  - Added visual pinned badge (`PINNED`), close/unpin button (`X`), and keyboard `Escape` unpin shortcut.
  - Interactive swarm cards: clicking any swarm in the pinned list opens the full torrent details modal.
  - Resolved boundary clipping by removing chart container overflow cutoffs and adding custom-scrollable contributor list.

## [0.5.2] - 2026-08-28

### Mobile Pairing & Reachability Fixes
- **Dynamic Mobile QR Origin Grounding**:
  - Web UI `ProfileModal` now grounds QR code pairing payloads to `window.location.origin` so external domains (e.g. `https://conduit.example.com`) are correctly scanned instead of internal server bind listener addresses (`0.0.0.0` / `127.0.0.1`).
  - Backend `create_mobile_pair_token` endpoint updated to inspect `Host`, `X-Forwarded-Host`, and `X-Forwarded-Proto` request headers.
  - Mobile client now detects and handles any legacy loopback URLs gracefully with pre-filled address validation.

## [0.5.1] - 2026-08-28

### Live Telemetry & Swarm Attribution
- **Bandwidth Spike Attribution on Hover**:
  - `BandwidthPoint` telemetry model enhanced with `topTorrents` (`TorrentBandwidthContributor` objects with `id`, `name`, `node`, `downloadSpeed`, `uploadSpeed`).
  - Server-side pool snapshot recording (`record_bandwidth_snapshot`) now actively collects and sorts top active swarms per second by combined throughput.
  - Web `BandwidthChart` tooltip updated with an active contributors panel displaying individual torrent names, node badges, download and upload speeds, and proportional bandwidth contribution percentages.
  - Conduit Mobile Flutter chart updated with interactive touch tooltips displaying top active swarm contributors.

## [0.5.0] - 2026-08-27

### Application Rebranding (Conduit)
- **Rebranded Core Daemon & UI to Conduit**:
  - Rebranded the unified BitTorrent orchestrator and media automation daemon to **Conduit**, reserving the previous name strictly for the player portal and playback applications.
  - Cargo package and binary renamed to `conduit` (`/app/conduit`).
  - Frontend UI package updated to `conduit-web` v0.5.0 with updated page titles, headers, navigation labels, and documentation.
  - Dual environment variable support during the rename transition, with fallback to the previous naming scheme (fallback support later removed once the rename fully settled).
  - Dual session cookie support during the rename transition, with fallback to the previous naming scheme.
  - Mobile QR code pairing protocol updated with fallback support for the previous token naming scheme during the transition.
  - Updated hook script generators (`conduit-fetch-hook.py`, `conduit-fetch-hook.pl`, `conduit-fetch-hook.sh`) and endpoints.

### Native SSL / TLS & Nginx Reverse Proxy Fixes
- Added native SSL/TLS configuration card in Settings UI (`ssl_enabled`, `ssl_cert`, `ssl_key`).
- Resolved container loopback hang by decoupling internal daemon HTTP mode (`--ssl-enabled false` on loopback) from Nginx external TLS termination.
- Added Nginx error page 497 auto-redirect for plain HTTP requests hitting HTTPS ports.

## [0.4.0] - 2026-08-27

### Transmission-Derived Power Features (From 3rd/ Research)
- **Queue Position & Priority Management**:
  - Implemented single and bulk queue reordering controls: Move to Top (`⇈ Top`), Up (`↑ Up`), Down (`↓ Down`), and Bottom (`⇊ Bottom`).
  - Added `/api/torrents/{compound_id}/queue-move` and `/api/torrents/queue-move` backend endpoints mapping directly to Transmission RPC `queue-move-top`, `queue-move-up`, `queue-move-down`, and `queue-move-bottom`.
  - Added queue movement buttons to `BulkActionBar` and exposed Queue # badges in torrent details.
- **Sequential Download Mode (Media Preview / Fast Stream)**:
  - Added support for `sequentialDownload` across RPC client, add torrent payloads, and runtime toggles.
  - Added sequential download checkbox to `AddTorrentModal` and 1-click live toggle in `TorrentDetailsModal`.
  - Added sequential delivery badge indicators.
- **Interactive Visual Piece Availability & Heatmap Visualizer**:
  - Decoded base64 `pieces` bitfields and mapped swarm `availability` arrays into an interactive piece availability heatmap grid inside `TorrentDetailsModal`.
  - Displays completed pieces, rarest-first swarm availability depth, and missing pieces with real-time hover tooltips.
- **Peer GeoIP Flags, Encryption Locks & uTP Protocol Badges**:
  - Enhanced peer swarm table with GeoIP country flags, SSL/TLS encryption lock badges (🔒 SSL vs Plain), and uTP/TCP protocol badges.
- **In-App File & Path Renaming**:
  - Added inline file and directory renaming within active torrents via `torrent-rename-path` RPC in `TorrentDetailsModal` Files tab.
- **Global & Per-Node Turtle Mode (Alternate Speed Limits)**:
  - Added 1-click Turtle Mode (🐢) status button in the global navigation bar and `/api/nodes/turtle-mode` endpoints to quickly throttle speeds across all nodes during peak hours.
- **Batch Tracker Search & Replace**:
  - Added dedicated `/api/torrents/batch-replace-trackers` endpoint and modal dialog in `HealthPanel` to find and replace tracker announce URLs and announce tiers across all torrents or specific nodes.
- **Remote IP Blocklist Updating**:
  - Added `blocklist-update` RPC trigger and rule count verification in daemon RPC settings.

### Multi-Client Architecture Documentation
- Created `docs/tracker_support.md` specifying the roadmap, unified `TorrentClient` async trait, and API mapping requirements for qBittorrent Web API v2 and Deluge JSON-RPC.

### Tracker Swarm Health & Circuit Breaker Intelligence
- **Multi-Tier Visual Swarm Health & Ratio Meter**:
  - Tracker health is now categorized and visualised across granular tiers:
    - 🟢 **100% Online**: 0 errors across all swarms.
    - 🟡 **Nominal**: Minor errors (<15% failure ratio, e.g. 5 errors out of 689 torrents), displaying exact online/error count and percentage.
    - 🟠 **Degraded**: Moderate errors (15% - 49% failure ratio).
    - 🔴 **Critical**: High failure ratio ($\ge$50% failure ratio), indicating imminent breaker trip.
    - 🟣 **Circuit Broken**: Breaker tripped with 1 active canary probe running while pausing the remaining swarms.
  - Added a visual multi-segment Swarm Health Progress Meter directly to tracker cards in the Health Panel.
- **Ratio-Based Circuit Breaker Thresholds & False-Positive Prevention**:
  - Circuit breakers no longer trip aggressively on a single transient torrent error.
  - Added `failure_ratio_threshold` (default 50%) and `min_failures` (default 3) to `TrackerCircuitBreakerConfig`, ensuring breakers only trip when a critical mass of swarms are confirmed failing.
  - Expanded default error patterns to match `"The tracker is down"`, `"tracker is down"`, and `"unreachable"`.
- **Circuit Breaker Configuration UI**:
  - Exposed full Tracker Circuit Breaker settings in Settings (System tab) with adjustable failure ratio threshold slider, minimum failing swarms input, stuck-open recovery ceiling, and auto-resume toggles.

## [0.3.20] - 2026-08-27

### Performance & Scalability
- **Added In-Memory Caching for Arr Grabs Map**:
  - Eliminated full-table SQLite scans and mutex contention executing every 1.0s on all connected WebSocket streams and `/api/torrents` routes; invalidation occurs on write/delete/purge.
- **Omitted Unused Torrent File Lists in Periodic Daemon Polling**:
  - `get_torrents()` no longer queries the heavy `"files"` array on 1-2s poll cycles across all transmission nodes, drastically reducing Transmission JSON serialization CPU and network payload sizes while keeping full file listings in on-demand `get_torrent_details()`.
- **Enabled SQLite WAL Concurrency Pragmas**:
  - Added `PRAGMA busy_timeout = 5000;` and `PRAGMA synchronous = NORMAL;` to prevent transient `SQLITE_BUSY` contention errors on concurrent transactions.
- **Offloaded Argon2id Password Verification to Dedicated Blocking Pool**:
  - Wrapped CPU-heavy password hashing/verification in `tokio::task::spawn_blocking` to prevent blocking Tokio async event loop worker threads.

### Security & Hardening
- **Mitigated Username Enumeration via Constant-Time Dummy Verification**:
  - Login attempts with non-existent usernames now perform constant-time dummy Argon2 verification before returning 401, eliminating response latency discrepancies used to discover valid accounts.
- **Added Defensive HTTP Security Headers**:
  - Static asset serving now injects `X-Content-Type-Options: nosniff`, `X-Frame-Options: SAMEORIGIN`, and `Referrer-Policy: strict-origin-when-cross-origin`.
- **Bounded and Sanitized Public Hook Inputs**:
  - Clamped input payload lengths on `/api/sync/notify` and `/api/sync/classify` to prevent abuse.

### Bug Fixes
- **Fixed RateLimiter Map Eviction Logic**:
  - Corrected inverted retention predicate in `RateLimiter::record_failure` so stale unlocked attempt windows are properly pruned.
- **Fixed Database Rekey Persistence**:
  - `Database::rekey()` now updates and saves the encryption key to disk with restricted `0600` permissions so subsequent daemon restarts can decrypt SQLite storage.
- **Fixed InfluxDB Line Protocol Formatting**:
  - Node names with spaces, commas, or equals signs are now properly escaped according to InfluxDB Line Protocol specs.
- **Added Automatic Cleanup of Empty Ingestion Watch Directories**:
  - Directory sync now prunes empty source subdirectories after moving content when `delete_source_after_move` is enabled.

## [0.3.19] - 2026-08-27

### Security & Hardening (Audit Follow-Up — Remaining Findings Resolved)
- **Added Server-Side Authentication to the WebSocket Endpoint**:
  - `/api/ws` previously accepted any connection with no server-side auth check at all — the frontend sent a JWT in the query string, but nothing validated it. Added `POST /api/auth/ws-ticket` (authenticated) issuing a short-lived, single-use 30s ticket; the WS upgrade now rejects any connection without a valid, unredeemed ticket.
- **Added Login & 2FA Rate Limiting**:
  - New in-memory limiter locks an account out for 15 minutes after 5 failed attempts within a rolling 15-minute window, applied to both `/api/auth/login` and the Basic Auth path.
- **Added TOTP Replay Protection**:
  - Added `totp_last_step` tracking with an atomic `consume_totp_step()` DB check; a captured 6-digit code can no longer be reused within its validity window.
- **Enforced Secure Cookie Attribute Behind TLS**:
  - Session cookies now carry `Secure` whenever the request arrived over HTTPS (directly or via `X-Forwarded-Proto`/`X-Forwarded-Ssl`), without breaking plain-HTTP-only deployments.
- **Required a Passphrase for Backup Export**:
  - `POST /api/settings/backup` now rejects export without an 8+ character passphrase, closing the accidental-plaintext-secrets-dump path.
- **Raised Password Minimum to 12 Characters**:
  - Bumped from the 8-character interim minimum (0.3.14) to the originally-recommended 12, consistently across setup, change-password, and the setup wizard's stale "4 characters" placeholder.
- **Replaced the Offline QR Generator with a Standards-Compliant Library**:
  - The hand-rolled QR matrix generator from 0.3.14 drew finder/timing/alignment patterns correctly but had no real Reed-Solomon error correction, so it produced something QR-*shaped* that didn't actually scan. Replaced with `qrcode-generator`; verified end-to-end by decoding the rendered output back to the original `otpauth://` URL.

### Reliability & Performance
- **Added Background Engine Supervision**:
  - All 9 engines now run under a supervisor that catches panics, logs them loudly, and restarts with capped exponential backoff instead of dying silently and permanently.
- **Added a Circuit Breaker Stuck-Open Escape Hatch**:
  - New `max_tripped_secs` ceiling (default 6h) force-clears a breaker if canary-based recovery detection never fires, with a distinct "force-cleared" notification.
- **Bounded Manual Batch Enrichment**:
  - `POST /api/torrents/enrich-all` now caps concurrency (5 in flight) and items per call (100) instead of firing an unbounded burst of sequential Sonarr/Radarr/Lidarr requests.
- **Fixed Auto-Purge Free-Space Path & Purge Ordering**:
  - Space manager and node health stats now measure free space on the torrent's actual download directory instead of the root filesystem, and purge candidates are sorted largest-reclaimable-first with a free-space recheck between removals.
- **Added Atomic Staging for File Sync**:
  - Remote-sync and per-node file copies now land in a hidden same-directory staging name and are atomically renamed into place on success, so a killed transfer can no longer leave a partial file mistaken for "already synced."
- **Wrapped the Hot-Path Auth DB Check in `spawn_blocking`**:
  - The per-request auth lookup (hit by every authenticated call) now runs off the async executor via new `*_async` `Database` methods.
- **Wired Up Response Compression & Static Asset Caching**:
  - `tower-http`'s gzip compression feature was already a dependency but was never applied to the router; added. Hashed build assets now get `immutable, max-age=1y`; `index.html` gets `no-cache`.
- **Code-Split the Frontend Bundle**:
  - Route-based `React.lazy` splitting dropped the main JS chunk from 612 KB to 427 KB; Settings (~3,900 lines, the largest file in the app), Docs, Events Log, and Pipeline Browser now load on demand.
- **Memoized Torrent Table Rows & Reduced WS/Poll Redundancy**:
  - `TorrentTable` rows are now a memoized component so a telemetry tick updating one torrent no longer re-renders (and re-parses) every other row; the REST polling fallback now backs off to a 30s safety net once the WebSocket is connected instead of duplicating it every 2s.
- **Cached Static Regex Patterns**:
  - Hoisted all static classification/parsing regexes into `LazyLock` statics instead of recompiling them per request; the pipeline engine now only rebuilds its configured-pattern regex when the pattern text actually changes.

### Frontend Fixes
- **Fixed Bulk Action False-Success Reporting**:
  - `POST /api/torrents/bulk` now returns one result per compound ID (including explicit errors for malformed ones) instead of an aggregated per-node string; the UI reads it and shows real partial-success/failure toasts.
- **Added Session-Expiry Handling**:
  - A central `apiFetch` wrapper detects a 401 on an authenticated call and routes the user back to login with a toast, instead of leaving the UI silently stuck.
- **Removed the Non-Functional Theme Toggle**:
  - The light-mode toggle promised a theme no component actually implemented; removed rather than building full theming in a fix pass.
- **Fixed Add-Torrent Node Selection & Added URL Validation**:
  - The target-node dropdown could silently stay unselected if opened before the node list loaded; now self-corrects, plus a submit-time check for a valid `magnet:`/`http(s)://` value.
- **Added Settings Load Error/Retry UI**:
  - A failed settings fetch no longer leaves a permanent "Loading settings..." with no recourse; shows the error with a retry button.
- **Fixed Delete Confirmation Closing on Failure**:
  - The modal now stays open and shows the real error when a delete fails, instead of closing as if it succeeded.
- **Fixed In-Place State Mutation in Settings Forms**:
  - All 12 sites that mutated an array element from React state directly before `setState` now spread it correctly.

### Developer Tooling
- **Added ESLint**:
  - New flat config with `rules-of-hooks` enforced and `exhaustive-deps`/`no-explicit-any` as warnings — a deliberately-scoped baseline rather than the plugin's newer React-Compiler-oriented "recommended" preset, which would have flagged many long-standing working patterns as hard errors.
- **Expanded CI**:
  - Added `lint-rust` (`cargo clippy --all-targets -D warnings`) and `test-web` (`npm run lint && npm run build`) pipeline stages alongside the existing Rust test job.

---

## [0.3.18] - 2026-08-27

### Fixed
- **RustEmbed Asset Directory Fallback (`web/dist`)**:
  - Added `build.rs` and CI step to ensure `web/dist` and a fallback `index.html` exist prior to compilation, preventing `#[derive(RustEmbed)]` compile-time failure on clean CI checkouts.

---

## [0.3.17] - 2026-08-27

### Fixed
- **CI Toolchain & SQLCipher System Dependencies**:
  - Added `libssl-dev` and `pkg-config` package installation to the GitLab CI `test-rust` job container to provide C development headers required by `rusqlite` (`bundled-sqlcipher`) and OpenSSL linking.
  - Ensured temporary database and configuration files remain pinned in `TestContext` across the lifecycle of integration tests.

---

## [0.3.16] - 2026-08-27

### Added & Fixed
- **Automated Arr Grab Detection & Real Indexer Attribution**:
  - Auto-enrichment now inspects live Sonarr/Radarr/Lidarr active queues (`/api/v3/queue`) and history (`/api/v3/history`) by `downloadId` (hash) and release title.
  - Automatically identifies RSS / push automated grabs and attributes the real indexer (e.g. `BroadcasTheNet`, `PassThePopcorn`, `Redacted`) rather than defaulting to `Manual Intake`.
  - Dispatches proper `🐕 Conduit Grabbed (Sonarr Automation)` Mattermost cards with indexer, quality, episode details, and structured timeline timestamps.
- **Tracker Domain Inference Engine**:
  - Added intelligent tracker matcher (`infer_tracker_name_from_string`) that maps tracker hosts (e.g. `broadcasthe.net` $\to$ `BroadcasTheNet`, `passthepopcorn.me` $\to$ `PassThePopcorn`, `redacted.ch` $\to$ `Redacted`, `orpheus.network` $\to$ `Orpheus`, `animebytes.tv` $\to$ `AnimeBytes`) even when webhooks or queue entries are absent.
- **Inbound Webhook Path Aliasing**:
  - Added route aliases for standard Arr, Plex, and Ombi webhook URL paths (`/api/webhook/sonarr`, `/api/webhooks/sonarr`, `/webhook/sonarr`, `/api/webhook/radarr`, `/webhook/radarr`, `/api/webhook/lidarr`, `/webhook/lidarr`, `/api/webhook/plex`, `/webhook/plex`, `/api/webhook/ombi`, `/webhook/ombi`).
- **Live Ingest & Completion Toast Notifications**:
  - Integrated real-time toast popups in the web interface whenever new content is grabbed or a download completes (`🐕 Conduit Sniffed: <Title> • <Indexer>`, `🐕 Conduit Retrieved: <Title>`).

---

## [0.3.15] - 2026-08-27

### Fixed
- **CI/CD Test Runner Rust Toolchain Compatibility**:
  - Updated CI `test-rust` runner image from `rust:1.80-bullseye` to `rust:1.94-bookworm` to support modern crate manifests requiring `edition2024` (such as `sha2 v0.11+`) and match the production `Dockerfile` build image.

---

## [0.3.14] - 2026-08-27

### Security & Hardening (Comprehensive Audit Fixes)
- **Eliminated Command Injection in File Sync & Hook Scripts (Critical #1)**:
  - Replaced shell execution string formatting (`sh -c`, `shell=True`, Perl backticks) in `file_sync.rs`, `scripts/copy2queue.py`, `scripts/copy2queue.pl`, and dynamic hook generator `src/api/sync_routes.rs` with direct list-argument execution (`subprocess.run(shell=False)`, Perl `system(@cmd)`, Rust `Command::new()`).
  - Added strict root-directory guards preventing destructive queue cleanup on root paths (`/`, `/root`, `/etc`, etc.).
- **Prevented Unbounded Inbound Webhook Cross-Delegation (Critical #2)**:
  - Refactored media type forwarding between Sonarr, Radarr, and Lidarr to route directly to terminal processors without looping.
- **Fixed Done-Date Sign-Cast Underflow Bug in Space Manager (Critical #3)**:
  - Guarded `space_manager.rs` done_date age calculation against underflow causing spurious auto-purging of newly completed downloads.
- **Enforced 2FA on Basic Auth and Added Token Revocation (`token_version`) (Critical #4 & High #2)**:
  - Updated Basic Auth middleware to require `user:password:totp_code` when 2FA is active on an account.
  - Added `token_version` tracking to users and JWT claims; invalidates all active sessions immediately upon logout, password change, or 2FA toggle.
- **Decoupled Database Key from JWT Secret & Secure Key Storage (Critical #5)**:
  - Database initialization now checks `CONDUIT_DB_KEY`, dedicated `.key` files (with `0600` permissions), or generates unique 256-bit passphrases independently from `jwt_secret`.
- **Enforced Webhook Secrets Across All Inbound Endpoints (High #1)**:
  - Added constant-time (`subtle::ConstantTimeEq`) secret/token verification for Sonarr, Radarr, Lidarr, Plex, and Ombi inbound webhooks.
- **Self-Contained Offline 2FA QR Code Generator (High #3)**:
  - Replaced 3rd party QR code external image API requests with an offline, high-contrast client-side SVG QR matrix generator.
- **Enforced Strict Password Policy (High #4)**:
  - Raised minimum password requirement from 4 to 8 characters across setup, change password, and profile modals.
- **Gated Session Configuration Behind Admin Privileges (High #12)**:
  - Restricted `POST /api/nodes/{name}/session` to `RequireAdmin`.
- **Configurable TLS Certificate Verification (High #6)**:
  - Added `verify_tls` configuration setting to Transmission nodes, defaulting to strict certificate verification.
- **Added Explicit Timeouts to Outbound HTTP Clients (High #10)**:
  - Configured 10s-15s request timeouts and 5s connect timeouts across Discord, Mattermost, Generic Webhook, Arr Sync, Watch Sync, InfluxDB, and Plex library refreshers.
- **Fixed Circuit Breaker Boolean Logic & False-Positive OK Matches (High #9)**:
  - Fixed boolean expression clippy error and refined recovery string checks to avoid false positives on words like "broken".
- **Added Lidarr Automated Re-Search & Fixed Status Update (Medium #6)**:
  - Added Lidarr `AlbumSearch` command integration to `/api/arr/pipeline/{id}/re-search` and only updated status upon confirmed dispatcher execution.

---

## [0.3.13] - 2026-08-27

### Fixed & Optimized
- **Eliminated N+1 Database Lock Bottleneck on `/api/torrents` & WebSockets**:
  - Diagnosed and resolved severe API hang where `GET /api/torrents` and `/api/ws` were performing thousands of sequential un-indexed SQLCipher queries with `LIKE '%' || ...` full table scans across the entire swarm while holding the SQLite mutex.
  - Replaced $O(N)$ database query iterations with a single batch `get_arr_grabs_lookup_map()` query, performing $O(1)$ in-memory lookups.
  - Added dedicated SQLite indexes on `arr_grabs(release_title)` and `arr_grabs(title)`.
  - Replaced slow wildcards with indexed `COLLATE NOCASE` lookups in `find_arr_grab_by_hash_or_name`.

---

## [0.3.12] - 2026-08-27

### Fixed & Added
- **Transmission Node Cache & Fetcher List Initialization**:
  - Fixed issue where configured nodes showed count `0` and fetcher list was empty upon startup.
  - Initialized `TransmissionPool` nodes from configuration immediately in `main.rs` before server launch.
  - Optimized `sync_nodes_from_config` in `TransmissionPool` to avoid discarding live `TransmissionClient` connections every 500ms, preserving authenticated session tokens and immediately populating default node cache entries.
- **Interactive UI Toast Notification System**:
  - Added comprehensive `ToastProvider` and `useToast` hook across the web interface with Dog/Conduit themed icons (`CheckCircle2`, `AlertTriangle`, `XCircle`, `Info`).
  - Added live toast notifications for adding torrents, starting/stopping downloads, executing bulk operations, removing torrents from nodes, and user session changes.

---

## [0.3.11] - 2026-08-27

### Fixed & Optimized
- **Auto-Enrichment Throttle & Non-Blocking Execution**:
  - Fixed severe runtime congestion caused by un-throttled synchronous HTTP queries across entire historical swarm on startup.
  - Restricted background automatic enrichment strictly to active intake torrents (actively downloading `percent_done < 1.0` or added in the last 15 minutes).
  - Added in-memory hash negative cache (`checked_enrichment_hashes`) to ensure each torrent is only evaluated once and never queried again if no match exists.
  - Offloaded enrichment lookups to a decoupled background task (`tokio::spawn`) with a maximum batch limit of 2 candidates per 30-second cycle, eliminating SQLite contention and UI latency.
- **Service Health Monitor Optimization**:
  - Reduced probe timeout to 3s and cycle interval to 30s for minimal background overhead.

---

## [0.3.10] - 2026-08-27

### Added & Enhanced
- **Automatic Media Enrichment for Manual Downloads**:
  - Automatically enriches newly detected torrents added manually to Transmission without requiring manual clicks.
  - Background pipeline loop periodically discovers un-tracked torrents and queries Sonarr, Radarr, and Lidarr to associate metadata and create living cards.
- **Unified Media Lifecycle Living Cards**:
  - Replaced disjoint single-event notifications with persistent living cards across the entire media lifecycle.
  - Enhanced `notify-download` staging endpoint to auto-enrich and evolve cards in-place with hardlink staging threads.
  - Handled `MovieFileDelete`, `EpisodeFileDelete`, and `TrackFileDelete` gracefully by threading upgrade/cleanup notes under the parent living card rather than replacing or duplicating top-level cards.
  - Added smart cross-identifier lookups (`movie_id`, `series_id`, `imdb_id`, `tmdb_id`, `tvdb_id`, and fuzzy title matching) so delete and import webhooks always locate the parent card.
- **Conduit Service Health Monitor & Failure Alerts**:
  - Added background health monitoring engine (`src/engines/health_monitor.rs`) that continuously probes Transmission nodes, Sonarr, Radarr, Lidarr, and Plex.
  - Automatically dispatches high-priority alert notifications (`🚨 Conduit Health Alert`) on persistent service failures and recovery notifications (`✅ Conduit Health Restored`) on restoration.
- **Full Prometheus Metrics Exposition (`/metrics`)**:
  - Expanded `/metrics` with per-node download/upload speeds, active torrents, RPC latency, Arr stack service configuration status, tracker circuit breaker metrics, and host hardware telemetry.
  - Added dedicated **Prometheus Metrics** tab in the Swarm & Client Health Panel with copyable endpoints and sample job configurations.

---

## [0.3.9] - 2026-08-26

### Added & Enhanced
- **Continuous Changelog Maintenance**:
  - Established persistent project changelog updating across all release version bumps.

---

## [0.3.8] - 2026-08-26

### Added & Enhanced
- **Kibble Requests (Ombi) & Media Portal Integration**:
  - Rebranded Ombi request pipeline to **Kibble Requests** across all notifications, audit logs, and settings UI:
    - `🐕🦴 Conduit Sniffing for Kibble • <Title> (<Year>)`
    - `🐕🥣 Conduit Approved Kibble Bowl • <Title> (<Year>)`
    - `🐕🎉 Conduit Delivered Kibble Bowl • <Title> (<Year>)`
    - `🐕🚫 Conduit Dropped Kibble Request • <Title> (<Year>)`
    - Thread comments: `🐕 Kibble bowl update: <event> by <user> -> Status: <status>`
  - Added dedicated **Kibble Requests** and **Media Portal** quick links in the left sidebar under the Media Stack section (later made fully configurable via `system.request_portal_url` / `system.media_portal_url`).
  - Updated Torrent Lifecycle timeline with `🦴 Conduit Sniffed Kibble (<App>)`.
  - Updated Settings UI with Kibble branding 🍖 and quick test simulator.

---

## [0.3.7] - 2026-08-26

### Added & Enhanced
- **Left Sidebar Media Stack Quick Links**:
  - Added direct quick links with Lucide icons for configured services:
    - 📺 **Sonarr TV**
    - 🎬 **Radarr Movies**
    - 🎵 **Lidarr Music**
- **Transmission Hook Script Debug Diagnostics**:
  - Added `--debug` / `-d` flags and `CONDUIT_DEBUG=1` / `DEBUG=1` environment variables to Python, Perl, and Bash hook script generators (`/api/sync/hook-script`).
  - Added automatic capture and printing of the server's exact response body on non-200 HTTP codes (`[DEBUG] Conduit Server Response: ...`) and outbound payload logging.

---

## [0.3.6] - 2026-08-26

### Added & Enhanced
- **Torrent Details Modal Active Download Progress Bar**:
  - Added a prominent full-width animated gradient progress bar near the top of the modal body when a torrent is actively downloading.
  - Displays real-time download and upload speed badges, downloaded vs. total bytes, active peer count, and dynamic ETA duration countdown.
- **Ombi Unified Living Card Pipeline & Webhook Audit Logging**:
  - Added `mattermost_post_id` column and migration to `ombi_requests` table.
  - Linked Ombi media requests directly into the single living Mattermost card lifecycle: Ombi request card seamlessly updates when Sonarr/Radarr/Lidarr grabs the release, when Transmission stages it, when the Arr stack imports it, and when Ombi marks it available.
  - Added dual audit logging under both `ombi` and `webhook` event types in SQLite.

---

## [0.3.5] - 2026-08-25

### Added & Enhanced
- **Database Self-Healing Retroactive Migration**:
  - Automatically heals existing historical rows in SQLite (`arr_grabs`) that were previously misclassified as `"series"` / `"Unknown Series"` to `"music"`.
  - Updates title to parsed release name for clear presentation in the Pipeline Browser and Torrent Lifecycle timeline.
- **Code Quality & Warning Elimination**:
  - Removed all unused imports and added appropriate dead code attributes across all subsystems.
  - Zero compiler warnings across `cargo check --tests` and `cargo check --bin conduit`.

---

## [0.3.4] - 2026-08-25

### Added & Enhanced
- **Dog-Themed Conduit Intake Hook Rebranding**:
  - Rebranded all Transmission client staging hook references from `copy2queue` to **`conduit-fetch-hook`** across the UI, settings, and timeline.
  - Updated Torrent Lifecycle timeline with `"🎾 Conduit Intake Staged & Hardlinked"`.
- **Transmission Hook Script Multiline & JSON Escaping Fixes**:
  - Fixed `HTTP 400 Bad Request` in shell hook scripts caused by multiline `TR_TORRENT_TRACKERS` and unescaped special characters.
  - Added POSIX `escape_json` helper and numeric coercion for `priority` and `bytes_downloaded`.
  - Added dual authentication headers (`Authorization: Bearer` and `X-Api-Key`).
- **Cross-App Media & Tracker Auto-Classification**:
  - Enhanced classifier with tracker aliases (`RED`, `OPS`, `Orpheus`, `Redacted`, `Rutracker`, `PTP`, `BTN`, etc.).
  - Added auto-delegation between Sonarr, Radarr, and Lidarr inbound webhooks for cross-app media detection.
  - Added built-in offline music regex heuristics (`[V0]`, `FLAC`, `MP3`, `Lossless`) routing directly to `/media/queue/musicQueue/`.

---

## [0.3.3] - 2026-08-25

### Added & Enhanced
- **Lidarr Music Integration & Inbound Webhooks (`/api/lidarr/inbound`)**:
  - Full ingestion of Lidarr music lifecycle events: `Grab`, `Download` / `AlbumDownload`, `TrackFileDelete`, `Rename`, and `Test`.
  - Extracts Artist Name, Album Title, Release Year, Genres, Album Artwork/Cover URL, Quality profile, Indexer, and Infohash.
  - Persists records to `arr_grabs` table with `item_type = "music"`.
  - Dispatches dog-themed rich notifications (`🐕🐾 Conduit Tracked Album`, `🐕🏆 Conduit Imported Album`) and evolves Living Cards with in-place updates.
- **Classification Tier 1 Routing for Music**:
  - Music grabs automatically classify torrents to `media_type = "music"` and route them into the `/media/queue/musicQueue/` directory.
- **Lidarr Multi-Node Master-Replica Synchronization**:
  - Automatically synchronizes artists, albums, quality profiles, and root folders between Lidarr master and replica nodes.
  - Added support for `/api/v1/artist` propagation via MusicBrainz ID matching.
- **Lidarr Settings & Pipeline Browser UI**:
  - Added dedicated **Lidarr (Music Automation)** card to **Settings $\to$ Arr Stack** with test connection button and one-click copyable webhook endpoint.
  - Updated **Pipeline Browser** with `Lidarr Music` intake telemetry, artist/album artwork badges, and music filters.

---

## [0.3.2] - 2026-08-25

### Added & Enhanced
- **Plex Media Server Webhook Inbound & Scrobble Ingestion (`/api/plex/inbound`)**:
  - Full support for Plex webhooks (`multipart/form-data` with `payload` JSON field as well as direct `application/json`).
  - Real-time ingestion of playback events (`media.scrobble`, `media.play`, `media.pause`, `media.stop`, `library.new`).
  - Extracts Series, Season/Episode numbers, Movie titles, User/Account name, view offsets, duration, and GUIDs (`imdb://`, `tmdb://`, `tvdb://`).
  - Stores scrobbles in dedicated SQLite `plex_scrobbles` table and `event_logs` audit trail.
  - Automatically dispatches dog-themed watch notifications (`🐕🍿 Conduit Watched`) to Mattermost and Discord.
- **Trakt Watch History Sync Engine Integration**:
  - Automatically pairs un-synced Plex scrobble events with Trakt (`/sync/history` API) when Trakt is configured.
  - Updates `trakt_synced` status in SQLite database.
- **Ombi Request Webhook Inbound (`/api/ombi/inbound`)**:
  - Real-time ingestion of Ombi media requests (`NewRequest`, `RequestApproved`, `RequestAvailable`, `RequestDenied`, `IssueCreated`).
  - Extracts requester, title, media type, release year, poster URL, overview, and external IDs.
  - Stores requests in SQLite `ombi_requests` table.
  - Automatically dispatches dog-themed request notifications (`🐕🙋 Conduit Sniffed Request`) to Mattermost and Discord.
- **Webhook Management & History in Settings UI**:
  - Added dedicated **Plex Webhook & Scrobble Ingestion** and **Ombi Webhook & Request Ingestion** cards to **Settings $\to$ Plex & Trakt**.
  - One-click copyable webhook endpoints (`/api/plex/inbound` and `/api/ombi/inbound`) with setup guides.
  - Live simulation test buttons and interactive recent history views.

---

## [0.3.1] - 2026-08-25

### Added & Enhanced
- **Mattermost Bot Token & REST API v4 Integration**:
  - Full support for official Mattermost Bot Token authentication (`Authorization: Bearer <bot_token>`).
  - Direct integration with `/api/v4/posts` REST endpoints with fallback to standard incoming webhooks.
- **Evolving "Living" Cards (In-Place Updates)**:
  - Persistent tracking of Mattermost `post_id` in SQLite across the entire media lifecycle.
  - Cards evolve in-place:
    - **Step 1 (Grab)**: `🟡 Sniffed & Fetching (0%)`
    - **Step 2 (Staged)**: `🟢 Staged & Hardlinked -> /media/queue/`
    - **Step 3 (Imported)**: `🏆 Imported & Library Ready`
- **Threaded Lifecycle Activity**:
  - Hardlink staging execution logs, replacement searches, and auto-purge details are threaded beneath the parent media card (`root_id: post_id`), reducing channel clutter.
- **Mattermost Test Verification in Settings**:
  - Added live verification sequence in Settings UI to test initial card creation and confirm in-place edit permissions.

---

## [0.3.0] - 2026-08-25

### Fixed & Improved
- **Zero-Latency Dashboard Telemetry Cache**:
  - Implemented module-level in-memory cache for aggregate bandwidth metrics, transmission speeds, and active fetcher lists.
  - Eliminates loading flashes/skeletons when navigating between tabs (Dashboard, Fetchers, Pipeline, Settings).
- **Ghost Notification Elimination**:
  - Filtered out non-media lifecycle webhooks (e.g. Sonarr/Radarr connection test pings, health checks) from generating empty `Unknown Series (0)` cards.
  - Required actionable media events (`Grab`, `Download`, `Rename`, `Delete`) with resolved titles before generating rich notifications.
- **Enhanced Arr Webhook Audit Trail**:
  - Preserved complete raw JSON payloads in `arr_grabs.payload_json` and `event_logs.details` for advanced debugging and future automation analysis.

---

## [0.2.9] - 2026-08-25

### Fixed & Improved
- **Stationary Status Sidebar Counts**:
  - Decoupled sidebar status filter counts (`downloading`, `seeding`, `active`, `paused`, `error`) from the active status filter in `useTelemetry.ts`.
  - Sidebar counts remain stationary and accurate when clicking through filters.
- **Enhanced 4-Tier Media Classification Engine**:
  - Added multi-protocol and host extraction for tracker announce URLs (supporting `http://`, `udp://`, ports, and multi-line strings).
  - Evaluated both structured `tracker_mappings` and `tracker_rules` map (supporting `landof.tv`, `passthepopcorn.me`, `morethantv.me`, `bitmetv.org`, etc.).
  - Added high-accuracy built-in heuristic classifiers for TV episodes (`S01E02`, `1x02`, `Season 1`, `S01`), Feature Films (`Hadestown.2026.1080p...`), Audio/Music, and Anime.
  - Added `.media` alias alongside `.media_type` in `ClassifyFileResponse` for seamless compatibility with ingestion scripts.
- **System Logs & Notification Feed Integration**:
  - Normalized event log level indexing to lowercase `"info"` so all download and staging events are immediately visible in the UI logs.
  - Added quick **System Logs & Staging Activity** shortcut button in the Navbar.

---

## [0.2.8] - 2026-08-25

### Fixed & Added
- **Full Circuit Breaker State Persistence in SQLite**:
  - Added persistent `circuit_breakers` database table tracking tripped trackers, canary probe IDs, failing error strings, and full lists of paused torrents.
  - Automatically restores all active circuit breaker states into `TransmissionPool` on daemon startup/restart.
  - Preserves swarm pressure relief and canary monitoring across container restarts and upgrades.
  - Automatically cleans up resolved circuit breakers from SQLite upon successful tracker announce recovery.

---

## [0.2.7] - 2026-08-25

### Fixed
- **Host Network Mode Port Collision**:
  - Relocated internal backend daemon port (`CONDUIT_INTERNAL_PORT`) from generic `3000` to isolated `42420`.
  - Fixes `Address already in use (os error 98)` when running with Docker `network_mode: "host"` on servers with existing services on port 3000.
  - Updated container healthcheck and Nginx proxy upstream accordingly.

---

## [0.2.6] - 2026-08-25

### Added
- **Dynamic Nginx SSL Reverse Proxy & Container Bundling**:
  - Integrated Nginx reverse proxy into production container image with dynamic `/data/ssl/` certificate detection.
  - Added TLSv1.2 & TLSv1.3 support, WebSocket upgrade mapping, and automatic HTTPS redirect option.
- **Batch Metadata Enrichment**:
  - Added `POST /api/torrents/enrich-all` and an **"Enrich All"** button in Navbar for bulk Sonarr/Radarr synchronization.
- **Canonical Lifecycle Reconciliation**:
  - Enhanced `save_arr_grab` to reconcile multi-stage pipeline grabs and prevent split cards.
- **Dog Movie Poster Placeholders**:
  - Bundled 20 canine parody movie posters as default fallback artwork across all UI views.
- **Scoped API Tokens**:
  - Added scoped API tokens (`fetcher`, `sync`, `torrents:read`, `*`) and `X-Api-Key` support for Transmission script integration.

---

## [0.2.5] - 2026-08-25

### Added
- **Integrated Nginx Reverse Proxy with Dynamic SSL/TLS**:
  - Bundled high-performance Nginx reverse proxy with full URL/URI routing (`/`, `/api/*`, `/ws/*`, `/metrics`).
  - Added dynamic SSL auto-detection (`CONDUIT_SSL_ENABLED=auto`, `true`, `false`).
  - Seamless certificate dropping: place `conduit.crt` and `conduit.key` (or `fullchain.pem` / `privkey.pem`) in `/data/ssl/` and Conduit automatically enables SSL.
  - Added modern TLS security (TLSv1.2 & TLSv1.3, Mozilla intermediate ciphers, session caching).
  - Support for dual HTTP & HTTPS modes and optional HTTPS 301 redirection (`CONDUIT_SSL_REDIRECT=true`).
  - WebSocket connection upgrade mapping with keepalive tuning and streaming buffering optimization.
  - Created `scripts/generate-cert.sh` for convenient certificate generation.
  - Added standalone `nginx/conduit.conf` reference configuration.
- **Batch "Enrich All" Action**:
  - Added `POST /api/torrents/enrich-all` endpoint to batch correlate and enrich un-enriched torrents across all nodes with Arr metadata.
  - Added **"Enrich All"** button in Navbar with live progress feedback.
- **Canonical Entity Reconciliation in Pipeline Feed**:
  - Enhanced `save_arr_grab` database layer to reconcile incoming webhooks by `download_id` (info hash), `movie_id`, `(series_id, season_number)`, or matching scene release names.
  - Eliminates duplicate/split cards in the Pipeline activity feed during lifecycle transitions.
- **Dog Movie Poster Placeholders**:
  - Bundled 20 canine parody movie posters in `/placeholders/` as default artwork for unenriched swarm torrents.
  - Deterministic hash-based poster assignment across table, cards, and modal views.
- **Scoped API Tokens & Transmission Inbound Auth**:
  - Added token scoping (`fetcher`, `sync`, `torrents:read`, `*`) in token generator and authentication middleware.
  - Added `X-Api-Key` header extraction support for Transmission completion hooks (`copy2queue.sh`).

---

## [0.2.4] - 2026-08-25

### Added
- **Intelligent Media Release & Season/Episode Parsing**:
  - Implemented `mediaParser.ts` utility to extract series titles, season numbers, episode ranges, and quality specs.
  - Automatically identifies **Season Packs** (e.g. `Nashville.2012.S05...` $\to$ `Nashville (2012) - Season 5`) and assigns prominent `📦 Season X Pack` badges.
  - Formats single/multi episodes (e.g. `House.of.the.Dragon.S02E01...` $\to$ `House of the Dragon (2022) - S02E01`).
  - Added duplicate-year suppression so titles like `Nashville (2012)` are never displayed as `Nashville (2012) (2012)`.
  - Backend `manual_enrich_torrent` now parses and stores `season_number`, `episode_numbers`, and `quality` in `ArrGrabRecord`.

### Changed
- **Dynamic Frontend Version Synchronization**:
  - Wired Vite's `define` configuration to inject `__APP_VERSION__` directly from `package.json` into the Navbar and Settings header badges.

---

## [0.2.3] - 2026-08-25

### Added
- **Comprehensive API Endpoint Test Suite**:
  - Full end-to-end integration test coverage across all REST endpoints in `tests/api_endpoint_tests.rs`:
    - Authentication, 2FA setup/verification, password management, profile updates, and API token lifecycle.
    - Torrent and fetcher operations, timeline generation, node filtering, and bulk actions.
    - Transmission node sessions, port testing, and media directory overrides.
    - Application settings, encrypted/plaintext backup exports and restores, and database rekeying.
    - Sonarr and Radarr inbound webhooks (Grab, Download, Rename, Delete), connection tests, and pipeline management.
    - Integrations (Plex, Trakt, Notifications, Pipeline Regex tester).
    - Queue routing, 4-tier file classification, and event audit log queries.
    - Prometheus `/metrics`, `/api/system/stats`, and `/api/system/health`.
- **Route & Security Hardening**:
  - Validated parameter binding and SQL injection immunity across all SQLite queries.
  - Enforced continuous swarm pressure relief in background circuit breaker loop.
  - Hardened lifecycle timeline status calculation for circuit-broken torrents and canary probes.

---

## [0.2.2] - 2026-08-25

### Added
- **Swarm & Transmission Client Health Panel**:
  - Live health telemetry endpoint `GET /api/system/health` exposing cluster daemon statuses, round-trip latencies, free storage, and tracker swarm states.
  - Interactive dashboard Health Panel with tabbed views for **Trackers** (circuit-broken count, active canary probes with live announce response messages, affected nodes) and **Transmission Daemons** (online/offline status, latency, error counts, free disk space).
  - Quick-action shortcuts to jump directly to active canary probe fetchers and swarm listings.
- **Color Palette Consistency**:
  - Aligned Download (Emerald / Green `#10b981`) and Upload (Sky / Blue `#0ea5e9`) telemetry metrics across top dashboard stat cards, charts, and table badges.
- **Richer Engine Audit Logging**:
  - Transmission Poller, Circuit Breaker, Space Manager, and Auto-Correction Pipeline engines now log startup events, cluster telemetry heartbeats, probe status scans, and connection state transitions.
  - Events Log default view now defaults to **All Log Levels** so operational audit trails are immediately visible.

---

## [0.2.1] - 2026-08-25

### Added
- **Transmission 4.x RPC Protocol Fix**:
  - Removed standard JSON-RPC 2.0 wrapper keys from outgoing Transmission requests to resolve `-32601` (`Method not found`) errors across Transmission 4.0+ daemons.
- **Enhanced Arr Metadata Enrichment**:
  - Added robust release title cleaning and media-type heuristics (`S01`, `Season`, release years) with automatic multi-arr fallbacks for movies and TV series.
- **Local Data Missing (Error 3) Self-Healing**: Automated 2-step healing workflow for Transmission `TR_STAT_LOCAL_ERROR` ("No data found! Ensure your drives are connected..."):
  - Automatically triggers Transmission's `torrent-verify` and resumes seeding if files are intact or drives remount.
  - Automatically invokes `MediaReplacer` (re-search in Sonarr/Radarr) and safely purges broken 0% torrents if data is permanently lost.
  - Dispatches self-healing alerts to Mattermost, Discord, and Webhooks.
- **Canary Probe Keep-Alive**: The Tracker Circuit Breaker now ensures the designated canary probe is always active (`start_torrents`) so announce queries continue during swarm outages.
- **User-Pause Isolation**: Auto-recovery selectively unpauses only torrents it paused itself, while preserving torrents paused manually by the user.
- **Graceful Tracker Backpressure Handling**: `TR_STAT_TRACKER_WARNING` notices (rate limits, retry timers) no longer falsely mark torrents as broken errors; amber warning badges are displayed in the Tracker Swarm inspector.
- **Project Changelog**: Added `CHANGELOG.md` tracking all historical releases and features.

---

## [0.2.0] - 2026-08-25

### Added
- **Real-Time Sonarr & Radarr Telemetry Polling**: Concurrently queries remote Sonarr/Radarr instances (`/api/v3/system/status`, `/api/v3/series`, `/api/v3/movie`, `/api/v3/queue`, `/api/v3/wanted/missing`, `/api/v3/health`, `/api/v3/diskspace`):
  - Live library sizes and monitored item counts.
  - Missing wanted episode/movie counters.
  - Active intake queue monitoring.
  - Disk mount storage metrics and health issue diagnostics.
- **OpenAPI 3.0 & Swagger UI Exposition**: Full OpenAPI 3.0 documentation covering all 35+ REST endpoints with interactive Swagger UI at `/swagger-ui` and spec at `/api-docs/openapi.json`.
- **Horizontal Lifecycle Timeline**: Responsive, horizontally scrolling 5-stage scent trail (`🦴 Sniffed`, `🐾 Fetching`, `🎾 Staged`, `🏆 Retrieved`) in Torrent Details modal.
- **Manual Metadata Enrichment ("Enrich from Arr")**: `POST /api/torrents/{compound_id}/enrich` endpoint and UI button to enrich unlinked torrents with posters, plot summaries, and IDs.
- **Version Exposure**: System telemetry (`GET /api/system/stats`), health check (`GET /api/health`), UI Navbar, and Settings header now expose the runtime daemon version.

---

## [0.1.0] - 2026-08-24

### Added
- **Multi-Node Transmission Control Plane**: Unified management of multiple Transmission daemons with Compound IDs (`{node}:{id}`).
- **Tracker Circuit Breaker & Swarm Pressure Relief**: Detects HTTP `530`, `502`, `503`, timeouts, and connection errors, pausing swarms while maintaining 1 canary probe.
- **4-Tier Content Classification Engine**:
  - Tier 1: Arr Webhook Correlation (`/api/{app}/inbound`).
  - Tier 2: Central Tracker Mapping Rules.
  - Tier 3: Regex Pattern Matching & UHD Markers (`tvUHD`, `movieUHD`).
  - Tier 4: Node Destination Overrides.
- **Conduit Pipeline Browser (`/pipeline`)**: Dedicated interface to inspect media moving through the pipeline with poster art and re-search triggers.
- **Hook Script Generator (`copy2queue`)**: Automated hook script generation supporting Shell, Python, and Perl with zero-copy hardlinks (`cp -al`).
- **Multi-Channel Notification Dispatcher**: Dog-themed alerts for Mattermost, Discord, and Webhooks.
- **Enterprise Security & SQLCipher Encryption**: TOTP 2FA, Argon2id hashing, JWT authentication, scoped API tokens (`cnd_...`), and 256-bit AES-GCM database encryption.

