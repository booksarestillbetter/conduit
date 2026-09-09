# Multi-Torrent Daemon Support Analysis (Synapse, Transmission, qBittorrent & Deluge)

This document outlines the architectural analysis, API specifications, feature parity matrix, data differences, and modular crate design for supporting **Synapse**, **Transmission**, **qBittorrent**, and **Deluge** concurrently in Conduit.

---

## 1. Architectural Strategy: Unified `TorrentClientTrait` Abstraction

Conduit communicates with download nodes via modular workspace crates (`crates/fetcher-*`) implementing the canonical `TorrentClientTrait` async trait:

```rust
#[async_trait::async_trait]
pub trait TorrentClientTrait: Send + Sync {
    fn client_type(&self) -> RetrieverClientType; // Synapse, Transmission, QBittorrent, Deluge
    fn node_name(&self) -> &str;

    async fn get_torrents(&self) -> anyhow::Result<Vec<UnifiedTorrent>>;
    async fn get_torrent_details(&self, hash: &str) -> anyhow::Result<DetailedTorrent>;
    async fn start_torrents(&self, hashes: &[String]) -> anyhow::Result<()>;
    async fn stop_torrents(&self, hashes: &[String]) -> anyhow::Result<()>;
    async fn remove_torrents(&self, hashes: &[String], delete_data: bool) -> anyhow::Result<()>;
    async fn add_torrent(&self, payload: &AddTorrentPayload) -> anyhow::Result<()>;
    async fn get_free_space(&self, path: &str) -> anyhow::Result<i64>;
    async fn get_session_stats(&self) -> anyhow::Result<NodeStats>;
    async fn test_connection(&self) -> anyhow::Result<bool>;
}
```

### Identifier Standardization
- **Transmission**: Supports both numeric IDs (`1, 2, 3`) and 40-character `hashString`s.
- **qBittorrent & Deluge**: Use 40-character infohashes exclusively.
- **Harmonized `compound_id`**: Standardization to `"{node_name}:{info_hash}"` makes swarm management, circuit breaker canary probing, space manager auto-purging, and UI routing 100% universal across all three daemons.

---

## 2. qBittorrent Integration Analysis (Web API v2)

qBittorrent exposes a comprehensive REST Web API (default port `8080`).

### Communication Architecture
- **Authentication**: `POST /api/v2/auth/login` with `username` and `password`. Returns session cookie `SID=...`.
- **Listing Swarms**: `GET /api/v2/torrents/info` (supports filtering by tag, category, hash, and status).
- **Tracker Telemetry**: `GET /api/v2/torrents/trackers?hash={hash}`.
- **Batch Controls**:
  - `POST /api/v2/torrents/pause` (`hashes=hash1|hash2`)
  - `POST /api/v2/torrents/resume` (`hashes=hash1|hash2`)
  - `POST /api/v2/torrents/delete` (`hashes=hash1|hash2&deleteFiles=true|false`)
  - `POST /api/v2/torrents/add` (supports magnet URLs, base64 / multipart `.torrent` uploads, save paths, category tags).
- **Free Space & Stats**: `GET /api/v2/sync/maindata` or `GET /api/v2/app/defaultSavePath`.

### Advantages Gained
1. **Delta / Incremental Sync (`/api/v2/sync/maindata?rid={rid}`)**:
   - qBittorrent can return only the modified swarm deltas rather than serializing all torrents on every poll tick, drastically cutting CPU and network usage on large libraries.
2. **Native Categories & Tags**:
   - Native support for categories (`tv`, `movies`, `music`, `sonarr`, `radarr`) and arbitrary tag arrays.
3. **Granular Seeding & Activity Timers**:
   - Directly exposes `time_active`, `seeding_time`, `reseed_complete`, and `content_path`.
4. **Per-Torrent Speed Limits**:
   - Upload/download bandwidth limits can be applied to individual torrents.

### Considerations & Gaps
1. **Tracker Retrieval**:
   - `/api/v2/torrents/info` provides the current active tracker. Backup tracker tiers require `/api/v2/torrents/trackers?hash=...` on-demand in `get_torrent_details()`.
2. **Done-Hook Invocation**:
   - Completed torrents execute scripts via qBittorrent's parameter tokens (e.g. `python3 conduit-fetch-hook.py --name "%N" --hash "%I" --dir "%D" --tracker "%T" --category "%L" --bytes "%Z"`).
   - Conduit's `conduit-fetch-hook.py` already supports CLI argument parsing out of the box.

---

## 3. Deluge Integration Analysis (Web JSON-RPC)

Deluge utilizes a client-daemon architecture (`deluged` on port `58846` and `deluge-web` on port `8112`). The cleanest and most stable integration in Rust is via **Deluge Web's JSON-RPC endpoint** (`POST http://host:8112/json`).

### Communication Architecture
- **Authentication**: `{"method": "auth.login", "params": ["password"], "id": 1}`. Returns cookie `_session_id=...`.
- **Listing Swarms**: `{"method": "core.get_torrents_status", "params": [{}, ["name", "hash", "state", "progress", "upload_payload_rate", "download_payload_rate", "total_uploaded", "total_done", "total_size", "tracker_status", "trackers", "save_path", "time_added", "seeds_peers_ratio"]], "id": 2}`.
- **Batch Controls**:
  - `core.pause_torrent` / `core.pause_torrents` (supports arrays of hashes).
  - `core.resume_torrent` / `core.resume_torrents`.
  - `core.remove_torrent` (`[hash, remove_data]`).
  - `core.add_torrent_magnet` / `core.add_torrent_file`.
- **Free Space**: `{"method": "core.get_free_space", "params": ["/path"], "id": 3}`.

### Advantages Gained
1. **Dynamic Field Masking**:
   - Queries can request exact required fields in `core.get_torrents_status`, keeping serialization light.
2. **Direct Tracker Response Strings**:
   - `tracker_status` provides exact error messages (e.g. `"Announce OK"`, `"Error: Timed out"`), enabling Conduit's **Multi-Tier Swarm Health** and **Circuit Breaker** to function natively.
3. **Plugin Extensibility**:
   - Integrates with Deluge's Label and AutoAdd plugins.

### Considerations & Gaps
1. **Deluge Web Dependency**:
   - Requires `deluge-web` to be running alongside `deluged`.
2. **Session Expiry Management**:
   - Sessions expire periodically and return `{"error": {"code": 1, "message": "Not authenticated"}}`, handled via automatic reconnect/re-login in the adapter.
3. **Done-Hook Invocation**:
   - Handled via Deluge's official **Execute** plugin (`conduit-fetch-hook.py <torrent-id> <torrent-name> <save-path>`).

---

## 4. Feature Parity Matrix

| Feature in Conduit | Synapse (gRPC) | Transmission | qBittorrent | Deluge (Web RPC) |
|---|:---:|:---:|:---:|:---:|
| **Unified Swarm Telemetry & Speed Rates** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Circuit Breaker & Canary Probing** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Multi-Tier Swarm Health Ratio** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Space Manager Auto-Purge & Disk Checks** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Multi-Tier File Sync & Intake Staging** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Arr Pipelines & Living Notifications** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Torrent Done Script Hooks** | ✅ gRPC Stream | ✅ Native Env Vars | ✅ CLI Arg Tokens (`%N`, `%I`) | ✅ Execute Plugin Args |
| **Torrent Add (Magnet & Base64 .torrent)** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Torrent Details (Files, Peers, Pieces)** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Delta / Incremental Polling** | 🟢 Native gRPC Delta Push | ❌ Full list | 🟢 Native (`/sync/maindata`) | ❌ Full list |

---

## 5. Modular Workspace Driver Crates

1. **`crates/fetcher-core`**:
   - Canonical `TorrentClientTrait` async trait, `Torrent`, `NodeStats`, `FetcherRegistry`, and `FetcherDriverFactory`.
2. **`crates/fetcher-synapse`**:
   - Native gRPC streaming delta driver using `synapse-client` and `SynapseLiveCache`.
3. **`crates/fetcher-transmission`**:
   - Standalone Transmission JSON-RPC client adapter and driver factory.
4. **`crates/fetcher-qbittorrent`**:
   - Standalone qBittorrent Web API client adapter and driver factory.
5. **`crates/fetcher-deluge`**:
   - Standalone Deluge JSON-RPC client adapter and driver factory.
