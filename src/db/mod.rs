// src/db/mod.rs
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    arr_grabs_cache: Arc<parking_lot::RwLock<Option<std::collections::HashMap<String, ArrGrabRecord>>>>,
    db_path: Arc<parking_lot::RwLock<Option<std::path::PathBuf>>>,
    /// Set once at startup via `set_event_bus` (after `init()`, once the bus exists) — `None`
    /// until then, e.g. in tests that construct a `Database` without wiring one up.
    event_bus: Arc<parking_lot::RwLock<Option<crate::events::EventBus>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserRecord {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub is_admin: bool,
    #[serde(default)]
    pub totp_enabled: bool,
    #[serde(skip_serializing)]
    pub totp_secret: Option<String>,
    #[serde(default = "default_token_version")]
    pub token_version: i64,
    #[serde(default, skip_serializing)]
    pub totp_last_step: i64,
    pub created_at: DateTime<Utc>,
}

fn default_token_version() -> i64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiTokenRecord {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing)]
    pub token_hash: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[allow(dead_code)]
pub struct PipelineRuleRecord {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub condition_field: String,
    pub condition_pattern: String,
    pub actions_json: String,
    pub created_at: DateTime<Utc>,
}

/// One append-only timeline entry for a canonical `ArrGrabRecord.id` — see
/// `arr_grab_history` and `Database::save_arr_grab`/`mark_grab_status` for why this exists
/// alongside (not instead of) the current-state `arr_grabs` table.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArrGrabHistoryEntry {
    pub id: i64,
    pub grab_id: String,
    pub event_type: String,
    pub status: String,
    pub release_title: Option<String>,
    pub scene_name: Option<String>,
    pub quality: Option<String>,
    pub size_bytes: Option<i64>,
    pub indexer: Option<String>,
    pub download_client: Option<String>,
    pub download_id: Option<String>,
    pub payload_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArrGrabRecord {
    pub id: String,
    pub scene_name: String,
    pub release_title: String,
    pub event_type: String,
    pub item_type: String, // "series" | "movie"
    pub series_id: Option<i64>,
    pub movie_id: Option<i64>,
    pub season_number: Option<i64>,
    pub episode_numbers: Option<String>, // e.g. "[1, 2]"
    pub episode_ids: Option<String>,     // e.g. "[101, 102]"
    pub indexer: Option<String>,
    pub download_client: Option<String>,
    pub download_id: Option<String>,     // Torrent info hash or client id
    pub status: String,                  // "fetched", "imported", "error_purged", "replaced"
    pub re_searched: bool,
    pub re_search_count: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub poster_url: Option<String>,
    #[serde(default)]
    pub genres: Option<String>,
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<i64>,
    #[serde(default)]
    pub imdb_id: Option<String>,
    #[serde(default)]
    pub tmdb_id: Option<i64>,
    #[serde(default)]
    pub tvdb_id: Option<i64>,
    #[serde(default)]
    pub runtime_mins: Option<i64>,
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default)]
    pub mattermost_post_id: Option<String>,
    pub payload_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub artist_id: Option<i64>,
    #[serde(default)]
    pub album_id: Option<i64>,
    /// Which configured zone (see `ZoneConfig`) this grab came from, if the webhook that
    /// produced it carried a `?zone=` query param matching a configured zone. `None` for
    /// webhooks with no zone param — the pre-zones default/legacy path.
    #[serde(default)]
    pub zone_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventLogRecord {
    pub id: i64,
    pub event_type: String,
    pub level: String,
    pub message: String,
    pub details_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlexScrobbleRecord {
    pub id: String,
    pub event: String,
    pub user_name: String,
    pub media_type: String, // "movie" | "episode" | "track"
    pub title: String,
    #[serde(default)]
    pub series_title: Option<String>,
    #[serde(default)]
    pub season_number: Option<i32>,
    #[serde(default)]
    pub episode_number: Option<i32>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub imdb_id: Option<String>,
    #[serde(default)]
    pub tmdb_id: Option<i64>,
    #[serde(default)]
    pub tvdb_id: Option<i64>,
    #[serde(default)]
    pub rating_key: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub view_offset_ms: Option<i64>,
    #[serde(default)]
    pub trakt_synced: bool,
    pub raw_json: String,
    pub created_at: DateTime<Utc>,
    /// Which configured `PlexNodeConfig.name` this scrobble came from, resolved from the
    /// webhook's `Server.uuid`/`Server.title` — `None` if it didn't match any configured node.
    /// Used by `engines::watch_sync` to only push scrobbles from opted-in servers to Trakt.
    #[serde(default)]
    pub source_node_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OmbiRequestRecord {
    pub id: String,
    pub event_type: String, // "NewRequest" | "RequestApproved" | "RequestAvailable" | "RequestDenied" | "IssueCreated"
    pub requested_by: String,
    pub media_type: String, // "movie" | "tv" | "music"
    pub title: String,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub poster_url: Option<String>,
    #[serde(default)]
    pub imdb_id: Option<String>,
    #[serde(default)]
    pub tmdb_id: Option<i64>,
    #[serde(default)]
    pub tvdb_id: Option<i64>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub mattermost_post_id: Option<String>,
    pub raw_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RekeyDbRequest {
    pub new_key: String,
}

impl Database {
    pub fn init<P: AsRef<Path>>(path: P, encryption_key: Option<&str>) -> anyhow::Result<Self> {
        let db_path_buf = path.as_ref().to_path_buf();
        let conn = Connection::open(&db_path_buf)?;

        // Apply SQLCipher encryption key if provided
        if let Some(key) = encryption_key {
            if !key.is_empty() {
                conn.pragma_update(None, "key", key)?;
            }
        }

        conn.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                is_admin INTEGER NOT NULL DEFAULT 1,
                totp_secret TEXT,
                totp_enabled INTEGER NOT NULL DEFAULT 0,
                token_version INTEGER NOT NULL DEFAULT 1,
                totp_last_step INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS api_tokens (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                token_hash TEXT UNIQUE NOT NULL,
                scopes TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT,
                last_used_at TEXT
            );

            CREATE TABLE IF NOT EXISTS pipeline_rules (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                condition_field TEXT NOT NULL,
                condition_pattern TEXT NOT NULL,
                actions_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS arr_grabs (
                id TEXT PRIMARY KEY,
                scene_name TEXT NOT NULL,
                release_title TEXT NOT NULL,
                event_type TEXT NOT NULL,
                item_type TEXT NOT NULL,
                series_id INTEGER,
                movie_id INTEGER,
                season_number INTEGER,
                episode_numbers TEXT,
                episode_ids TEXT,
                indexer TEXT,
                download_client TEXT,
                download_id TEXT,
                status TEXT NOT NULL DEFAULT 'fetched',
                re_searched INTEGER NOT NULL DEFAULT 0,
                re_search_count INTEGER NOT NULL DEFAULT 0,
                title TEXT,
                year INTEGER,
                overview TEXT,
                poster_url TEXT,
                genres TEXT,
                quality TEXT,
                size_bytes INTEGER,
                imdb_id TEXT,
                tmdb_id INTEGER,
                tvdb_id INTEGER,
                runtime_mins INTEGER,
                rating REAL,
                mattermost_post_id TEXT,
                payload_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                artist_id INTEGER,
                album_id INTEGER,
                zone_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_arr_grabs_scene ON arr_grabs(scene_name);
            CREATE INDEX IF NOT EXISTS idx_arr_grabs_rel ON arr_grabs(release_title);
            CREATE INDEX IF NOT EXISTS idx_arr_grabs_title ON arr_grabs(title);
            CREATE INDEX IF NOT EXISTS idx_arr_grabs_hash ON arr_grabs(download_id);

            CREATE TABLE IF NOT EXISTS event_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                level TEXT NOT NULL,
                message TEXT NOT NULL,
                details_json TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_event_logs_created ON event_logs(created_at);

            CREATE TABLE IF NOT EXISTS circuit_breakers (
                tracker_host TEXT PRIMARY KEY,
                canary_compound_id TEXT NOT NULL,
                canary_name TEXT NOT NULL,
                paused_torrents_json TEXT NOT NULL,
                failing_error TEXT NOT NULL,
                tripped_at INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS plex_scrobbles (
                id TEXT PRIMARY KEY,
                event TEXT NOT NULL,
                user_name TEXT NOT NULL,
                media_type TEXT NOT NULL,
                title TEXT NOT NULL,
                series_title TEXT,
                season_number INTEGER,
                episode_number INTEGER,
                year INTEGER,
                imdb_id TEXT,
                tmdb_id INTEGER,
                tvdb_id INTEGER,
                rating_key TEXT,
                duration_ms INTEGER,
                view_offset_ms INTEGER,
                trakt_synced INTEGER NOT NULL DEFAULT 0,
                raw_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                source_node_name TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_plex_scrobbles_created ON plex_scrobbles(created_at);
            CREATE INDEX IF NOT EXISTS idx_plex_scrobbles_event ON plex_scrobbles(event);
            CREATE INDEX IF NOT EXISTS idx_plex_scrobbles_user ON plex_scrobbles(user_name);

            -- Minimal general-purpose key/value store — first use is the Trakt watch-sync's
            -- last-checked watermark (see engines::watch_sync), so each cycle only processes
            -- items newer than last time instead of re-scanning Trakt's entire watched history.
            CREATE TABLE IF NOT EXISTS sync_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ombi_requests (
                id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                requested_by TEXT NOT NULL,
                media_type TEXT NOT NULL,
                title TEXT NOT NULL,
                year INTEGER,
                overview TEXT,
                poster_url TEXT,
                imdb_id TEXT,
                tmdb_id INTEGER,
                tvdb_id INTEGER,
                status TEXT NOT NULL DEFAULT 'pending',
                mattermost_post_id TEXT,
                raw_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_ombi_requests_created ON ombi_requests(created_at);
            CREATE INDEX IF NOT EXISTS idx_ombi_requests_user ON ombi_requests(requested_by);
            CREATE INDEX IF NOT EXISTS idx_ombi_requests_type ON ombi_requests(media_type);

            -- Append-only per-media-item event timeline. arr_grabs itself is a current-state
            -- table — canonical entity reconciliation in save_arr_grab deliberately collapses
            -- every grab/import/delete/re-grab for the same movie/series onto one row (by
            -- movie_id/series_id/download_id/scene_name) so the Pipeline/Fetchers/Archive views
            -- show one card per item, not one per event. That collapsing means arr_grabs alone
            -- can never show that an item was grabbed, imported, then trumped and re-fetched —
            -- only whatever the last event happened to leave behind. This table is the fix:
            -- every write to arr_grabs (via save_arr_grab or mark_grab_status) also appends one
            -- row here keyed by the same canonical grab_id, so the full lineage survives.
            CREATE TABLE IF NOT EXISTS arr_grab_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                grab_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                status TEXT NOT NULL,
                release_title TEXT,
                scene_name TEXT,
                quality TEXT,
                size_bytes INTEGER,
                indexer TEXT,
                download_client TEXT,
                download_id TEXT,
                payload_json TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_arr_grab_history_grab_id ON arr_grab_history(grab_id, created_at);

            -- One row per minute, one-minute-average of the in-memory 1s bandwidth samples
            -- (see transmission::pool::FetcherPool's 5-minute ring buffer) — lets the
            -- Dashboard's bandwidth chart offer 3h/12h/24h/72h ranges without keeping tens of
            -- thousands of 1s samples in memory. Pruned to the last 72h on every insert.
            CREATE TABLE IF NOT EXISTS bandwidth_history_downsampled (
                time INTEGER PRIMARY KEY,
                download_speed INTEGER NOT NULL,
                upload_speed INTEGER NOT NULL
            );
        ")?;

        // Migration safety checks for existing databases
        let _ = conn.execute("ALTER TABLE users ADD COLUMN totp_secret TEXT", []);
        let _ = conn.execute("ALTER TABLE users ADD COLUMN totp_enabled INTEGER NOT NULL DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN title TEXT", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN year INTEGER", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN overview TEXT", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN poster_url TEXT", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN genres TEXT", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN quality TEXT", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN size_bytes INTEGER", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN imdb_id TEXT", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN tmdb_id INTEGER", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN tvdb_id INTEGER", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN runtime_mins INTEGER", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN rating REAL", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN mattermost_post_id TEXT", []);
        let _ = conn.execute("ALTER TABLE ombi_requests ADD COLUMN mattermost_post_id TEXT", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN artist_id INTEGER", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN album_id INTEGER", []);
        let _ = conn.execute("ALTER TABLE arr_grabs ADD COLUMN zone_id TEXT", []);
        let _ = conn.execute("ALTER TABLE users ADD COLUMN token_version INTEGER NOT NULL DEFAULT 1", []);
        let _ = conn.execute("ALTER TABLE users ADD COLUMN totp_last_step INTEGER NOT NULL DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE plex_scrobbles ADD COLUMN source_node_name TEXT", []);
        let _ = conn.execute("ALTER TABLE circuit_breakers ADD COLUMN state TEXT NOT NULL DEFAULT 'tripped'", []);
        let _ = conn.execute("ALTER TABLE circuit_breakers ADD COLUMN recovery_started_at INTEGER", []);
        let _ = conn.execute("ALTER TABLE circuit_breakers ADD COLUMN consecutive_successes INTEGER NOT NULL DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE circuit_breakers ADD COLUMN backoff_secs INTEGER NOT NULL DEFAULT 0", []);

        // Retroactively heal previously misclassified audio grabs (e.g. grabbed via Sonarr before Lidarr routing)
        let _ = conn.execute(
            "UPDATE arr_grabs
             SET item_type = 'music'
             WHERE (item_type = 'series' OR item_type = 'tv' OR item_type = '')
               AND (title = 'Unknown Series' OR title = '' OR title IS NULL)
               AND (scene_name LIKE '%[V0]%' OR scene_name LIKE '%FLAC%' OR scene_name LIKE '%MP3%' OR scene_name LIKE '%Lossless%' OR scene_name LIKE '%320kbps%')",
            [],
        );
        let _ = conn.execute(
            "UPDATE arr_grabs
             SET title = scene_name
             WHERE item_type = 'music'
               AND (title = 'Unknown Series' OR title = '' OR title IS NULL)",
            [],
        );

        // Retroactively heal grabs where an import occurred under a duplicate/generic title record
        let _ = conn.execute(
            "UPDATE arr_grabs
             SET status = 'imported'
             WHERE status = 'fetched'
               AND (
                 id IN (SELECT DISTINCT grab_id FROM arr_grab_history WHERE event_type = 'Download' OR status = 'imported')
                 OR scene_name IN (SELECT DISTINCT scene_name FROM arr_grabs WHERE status = 'imported' OR event_type = 'Download')
                 OR (series_id IS NOT NULL AND season_number IS NOT NULL AND episode_numbers IS NOT NULL AND (series_id, season_number, episode_numbers) IN (SELECT series_id, season_number, episode_numbers FROM arr_grabs WHERE status = 'imported'))
               )",
            [],
        );

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            arr_grabs_cache: Arc::new(parking_lot::RwLock::new(None)),
            db_path: Arc::new(parking_lot::RwLock::new(Some(db_path_buf))),
            event_bus: Arc::new(parking_lot::RwLock::new(None)),
        })
    }

    /// Wires up the process-wide event bus so `log_event`/`save_arr_grab` publish onto the
    /// `events`/`pipeline` WebSocket topics as they write, instead of the frontend having to
    /// poll for changes. Safe to call at most once, right after both `Database::init` and the
    /// bus itself exist (see main.rs); every clone of this `Database` shares the same bus via
    /// the underlying `Arc`.
    pub fn set_event_bus(&self, bus: crate::events::EventBus) {
        *self.event_bus.write() = Some(bus);
    }

    fn publish_event(&self, topic: &str, payload: &impl serde::Serialize) {
        if let Some(ref bus) = *self.event_bus.read() {
            crate::events::publish(bus, topic, payload);
        }
    }

    /// Live database encryption key rotation using SQLCipher `PRAGMA rekey`
    pub fn rekey(&self, new_key: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.pragma_update(None, "rekey", new_key)?;
        if let Some(ref path) = *self.db_path.read() {
            let key_path = path.with_extension("key");
            if let Err(e) = std::fs::write(&key_path, new_key) {
                tracing::warn!("Could not write updated database key to {}: {}", key_path.display(), e);
            } else {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
                }
                tracing::info!("Updated database encryption key file at {}", key_path.display());
            }
        }
        Ok(())
    }

    // --- Users ---
    pub fn get_user_count(&self) -> anyhow::Result<usize> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM users")?;
        let count: usize = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    }

    pub fn has_users(&self) -> anyhow::Result<bool> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
        Ok(count > 0)
    }

    pub fn create_user(&self, username: &str, password_hash: &str, is_admin: bool) -> anyhow::Result<UserRecord> {
        let conn = self.conn.lock();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        conn.execute(
            "INSERT INTO users (id, username, password_hash, is_admin, totp_enabled, token_version, totp_last_step, created_at) VALUES (?1, ?2, ?3, ?4, 0, 1, 0, ?5)",
            params![id, username, password_hash, is_admin as i32, now_str],
        )?;

        Ok(UserRecord {
            id,
            username: username.to_string(),
            password_hash: password_hash.to_string(),
            is_admin,
            totp_enabled: false,
            totp_secret: None,
            token_version: 1,
            totp_last_step: 0,
            created_at: now,
        })
    }

    pub fn get_user_by_username(&self, username: &str) -> anyhow::Result<Option<UserRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, username, password_hash, is_admin, created_at, totp_enabled, totp_secret, token_version, totp_last_step FROM users WHERE username = ?1")?;
        let user = stmt.query_row(params![username], |row| {
            let created_str: String = row.get(4)?;
            let created_at = DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            Ok(UserRecord {
                id: row.get(0)?,
                username: row.get(1)?,
                password_hash: row.get(2)?,
                is_admin: row.get::<_, i32>(3)? != 0,
                created_at,
                totp_enabled: row.get::<_, i32>(5)? != 0,
                totp_secret: row.get(6)?,
                token_version: row.get(7).unwrap_or(1),
                totp_last_step: row.get(8).unwrap_or(0),
            })
        }).optional()?;

        Ok(user)
    }

    pub fn get_user_by_id(&self, id: &str) -> anyhow::Result<Option<UserRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, username, password_hash, is_admin, created_at, totp_enabled, totp_secret, token_version, totp_last_step FROM users WHERE id = ?1")?;
        let user = stmt.query_row(params![id], |row| {
            let created_str: String = row.get(4)?;
            let created_at = DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            Ok(UserRecord {
                id: row.get(0)?,
                username: row.get(1)?,
                password_hash: row.get(2)?,
                is_admin: row.get::<_, i32>(3)? != 0,
                created_at,
                totp_enabled: row.get::<_, i32>(5)? != 0,
                totp_secret: row.get(6)?,
                token_version: row.get(7).unwrap_or(1),
                totp_last_step: row.get(8).unwrap_or(0),
            })
        }).optional()?;

        Ok(user)
    }

    pub fn increment_user_token_version(&self, id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE users SET token_version = token_version + 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn update_user_password(&self, id: &str, password_hash: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE users SET password_hash = ?1, token_version = token_version + 1 WHERE id = ?2",
            params![password_hash, id],
        )?;
        Ok(())
    }

    pub fn update_user_profile(&self, id: &str, new_username: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE users SET username = ?1 WHERE id = ?2",
            params![new_username, id],
        )?;
        Ok(())
    }

    pub fn set_totp_secret(&self, id: &str, secret: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE users SET totp_secret = ?1, totp_last_step = 0 WHERE id = ?2",
            params![secret, id],
        )?;
        Ok(())
    }

    /// Records the TOTP time-step that was just successfully consumed, rejecting the update
    /// (returns Ok(false)) if a step at or after it has already been recorded — replay protection.
    pub fn consume_totp_step(&self, id: &str, step: i64) -> anyhow::Result<bool> {
        let conn = self.conn.lock();
        let affected = conn.execute(
            "UPDATE users SET totp_last_step = ?1 WHERE id = ?2 AND totp_last_step < ?1",
            params![step, id],
        )?;
        Ok(affected > 0)
    }

    // --- Async wrappers for the request-hot auth path ---
    //
    // Every one of these runs a synchronous SQLite call behind a parking_lot mutex; called
    // directly from an async handler that would hold up the executing Tokio worker thread for
    // the call's duration. The auth check in particular runs on *every* authenticated request,
    // so it's the one place this actually matters — wrap it in spawn_blocking so a burst of
    // concurrent requests can't stall each other behind the DB lock on the async runtime.

    pub async fn get_user_by_id_async(&self, id: &str) -> anyhow::Result<Option<UserRecord>> {
        let db = self.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || db.get_user_by_id(&id)).await?
    }

    pub async fn get_user_by_username_async(&self, username: &str) -> anyhow::Result<Option<UserRecord>> {
        let db = self.clone();
        let username = username.to_string();
        tokio::task::spawn_blocking(move || db.get_user_by_username(&username)).await?
    }

    pub async fn verify_and_touch_token_async(&self, token_hash: &str) -> anyhow::Result<Option<ApiTokenRecord>> {
        let db = self.clone();
        let token_hash = token_hash.to_string();
        tokio::task::spawn_blocking(move || db.verify_and_touch_token(&token_hash)).await?
    }

    pub async fn consume_totp_step_async(&self, id: &str, step: i64) -> anyhow::Result<bool> {
        let db = self.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || db.consume_totp_step(&id, step)).await?
    }

    pub fn enable_totp(&self, id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE users SET totp_enabled = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn disable_totp(&self, id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE users SET totp_enabled = 0, totp_secret = NULL, totp_last_step = 0 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    // --- API Tokens ---
    pub fn create_api_token(&self, name: &str, token_hash: &str, scopes: &[String], expires_at: Option<DateTime<Utc>>) -> anyhow::Result<ApiTokenRecord> {
        let conn = self.conn.lock();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let scopes_str = scopes.join(",");

        conn.execute(
            "INSERT INTO api_tokens (id, name, token_hash, scopes, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                name,
                token_hash,
                scopes_str,
                now.to_rfc3339(),
                expires_at.map(|e| e.to_rfc3339())
            ],
        )?;

        Ok(ApiTokenRecord {
            id,
            name: name.to_string(),
            token_hash: token_hash.to_string(),
            scopes: scopes.to_vec(),
            created_at: now,
            expires_at,
            last_used_at: None,
        })
    }

    pub fn list_api_tokens(&self) -> anyhow::Result<Vec<ApiTokenRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, name, token_hash, scopes, created_at, expires_at, last_used_at FROM api_tokens ORDER BY created_at DESC")?;
        let rows = stmt.query_map([], |row| {
            let scopes_str: String = row.get(3)?;
            let created_str: String = row.get(4)?;
            let expires_str: Option<String> = row.get(5)?;
            let last_used_str: Option<String> = row.get(6)?;

            let scopes = scopes_str.split(',').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
            let created_at = DateTime::parse_from_rfc3339(&created_str).map(|dt| dt.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now());
            let expires_at = expires_str.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc)));
            let last_used_at = last_used_str.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc)));

            Ok(ApiTokenRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                token_hash: row.get(2)?,
                scopes,
                created_at,
                expires_at,
                last_used_at,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_api_token(&self, id: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock();
        let affected = conn.execute("DELETE FROM api_tokens WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    pub fn verify_and_touch_token(&self, token_hash: &str) -> anyhow::Result<Option<ApiTokenRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, name, token_hash, scopes, created_at, expires_at, last_used_at FROM api_tokens WHERE token_hash = ?1")?;
        let token = stmt.query_row(params![token_hash], |row| {
            let scopes_str: String = row.get(3)?;
            let created_str: String = row.get(4)?;
            let expires_str: Option<String> = row.get(5)?;
            let last_used_str: Option<String> = row.get(6)?;

            let scopes = scopes_str.split(',').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
            let created_at = DateTime::parse_from_rfc3339(&created_str).map(|dt| dt.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now());
            let expires_at = expires_str.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc)));
            let last_used_at = last_used_str.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc)));

            Ok(ApiTokenRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                token_hash: row.get(2)?,
                scopes,
                created_at,
                expires_at,
                last_used_at,
            })
        }).optional()?;

        if let Some(ref tok) = token {
            if let Some(exp) = tok.expires_at {
                if exp < Utc::now() {
                    return Ok(None);
                }
            }
            let now_str = Utc::now().to_rfc3339();
            conn.execute("UPDATE api_tokens SET last_used_at = ?1 WHERE id = ?2", params![now_str, tok.id])?;
        }

        Ok(token)
    }

    // --- Arr Grabs & Lifecycle Cache ---
    pub fn save_arr_grab(&self, record: &ArrGrabRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock();

        // ── Canonical Entity Reconciliation ──
        // Check if an existing record already represents this media item to prevent split cards
        let mut target_id = record.id.clone();

        // 1. Check by download_id (hash string)
        if let Some(ref did) = record.download_id {
            if !did.trim().is_empty() {
                let existing: Option<String> = conn.query_row(
                    "SELECT id FROM arr_grabs WHERE LOWER(download_id) = LOWER(?1) OR id = ?1 LIMIT 1",
                    params![did],
                    |row| row.get(0),
                ).optional().unwrap_or(None);
                if let Some(eid) = existing {
                    target_id = eid;
                }
            }
        }

        // 2. Check by movie_id or series_id + season
        if target_id == record.id {
            if let Some(mid) = record.movie_id {
                let existing: Option<String> = conn.query_row(
                    "SELECT id FROM arr_grabs WHERE movie_id = ?1 LIMIT 1",
                    params![mid],
                    |row| row.get(0),
                ).optional().unwrap_or(None);
                if let Some(eid) = existing {
                    target_id = eid;
                }
            } else if let Some(sid) = record.series_id {
                let existing: Option<String> = conn.query_row(
                    "SELECT id FROM arr_grabs WHERE series_id = ?1 AND (season_number IS ?2 OR season_number IS NULL) LIMIT 1",
                    params![sid, record.season_number],
                    |row| row.get(0),
                ).optional().unwrap_or(None);
                if let Some(eid) = existing {
                    target_id = eid;
                }
            }
        }

        // 3. Check by scene_name or release_title
        if target_id == record.id && !record.scene_name.is_empty() {
            let existing: Option<String> = conn.query_row(
                "SELECT id FROM arr_grabs WHERE scene_name = ?1 OR release_title = ?1 LIMIT 1",
                params![record.scene_name],
                |row| row.get(0),
            ).optional().unwrap_or(None);
            if let Some(eid) = existing {
                target_id = eid;
            }
        }

        conn.execute(
            "INSERT INTO arr_grabs (
                id, scene_name, release_title, event_type, item_type,
                series_id, movie_id, season_number, episode_numbers, episode_ids,
                indexer, download_client, download_id, status, re_searched,
                re_search_count, title, year, overview, poster_url, genres,
                quality, size_bytes, imdb_id, tmdb_id, tvdb_id, runtime_mins, rating,
                mattermost_post_id, payload_json, created_at, updated_at, artist_id, album_id, zone_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35)
            ON CONFLICT(id) DO UPDATE SET
                event_type = excluded.event_type,
                status = excluded.status,
                scene_name = CASE WHEN excluded.scene_name != '' AND excluded.scene_name != excluded.title THEN excluded.scene_name ELSE arr_grabs.scene_name END,
                release_title = CASE WHEN excluded.release_title != '' AND excluded.release_title != excluded.title THEN excluded.release_title ELSE arr_grabs.release_title END,
                download_id = COALESCE(excluded.download_id, arr_grabs.download_id),
                download_client = COALESCE(excluded.download_client, arr_grabs.download_client),
                indexer = COALESCE(excluded.indexer, arr_grabs.indexer),
                season_number = COALESCE(excluded.season_number, arr_grabs.season_number),
                episode_numbers = COALESCE(excluded.episode_numbers, arr_grabs.episode_numbers),
                title = COALESCE(excluded.title, arr_grabs.title),
                year = COALESCE(excluded.year, arr_grabs.year),
                overview = COALESCE(excluded.overview, arr_grabs.overview),
                poster_url = COALESCE(excluded.poster_url, arr_grabs.poster_url),
                genres = COALESCE(excluded.genres, arr_grabs.genres),
                quality = COALESCE(excluded.quality, arr_grabs.quality),
                size_bytes = COALESCE(excluded.size_bytes, arr_grabs.size_bytes),
                imdb_id = COALESCE(excluded.imdb_id, arr_grabs.imdb_id),
                tmdb_id = COALESCE(excluded.tmdb_id, arr_grabs.tmdb_id),
                tvdb_id = COALESCE(excluded.tvdb_id, arr_grabs.tvdb_id),
                runtime_mins = COALESCE(excluded.runtime_mins, arr_grabs.runtime_mins),
                rating = COALESCE(excluded.rating, arr_grabs.rating),
                mattermost_post_id = COALESCE(excluded.mattermost_post_id, arr_grabs.mattermost_post_id),
                artist_id = COALESCE(excluded.artist_id, arr_grabs.artist_id),
                album_id = COALESCE(excluded.album_id, arr_grabs.album_id),
                zone_id = COALESCE(excluded.zone_id, arr_grabs.zone_id),
                updated_at = excluded.updated_at,
                payload_json = excluded.payload_json",
            params![
                target_id,
                record.scene_name,
                record.release_title,
                record.event_type,
                record.item_type,
                record.series_id,
                record.movie_id,
                record.season_number,
                record.episode_numbers,
                record.episode_ids,
                record.indexer,
                record.download_client,
                record.download_id,
                record.status,
                record.re_searched as i32,
                record.re_search_count,
                record.title,
                record.year,
                record.overview,
                record.poster_url,
                record.genres,
                record.quality,
                record.size_bytes,
                record.imdb_id,
                record.tmdb_id,
                record.tvdb_id,
                record.runtime_mins,
                record.rating,
                record.mattermost_post_id,
                record.payload_json,
                record.created_at.to_rfc3339(),
                record.updated_at.to_rfc3339(),
                record.artist_id,
                record.album_id,
                record.zone_id
            ],
        )?;
        // Append to the permanent per-item timeline — see arr_grab_history's schema comment for
        // why this exists alongside the collapsing upsert above rather than instead of it.
        Self::insert_grab_history_row(
            &conn,
            &target_id,
            &record.event_type,
            &record.status,
            Some(&record.release_title),
            Some(&record.scene_name),
            record.quality.as_deref(),
            record.size_bytes,
            record.indexer.as_deref(),
            record.download_client.as_deref(),
            record.download_id.as_deref(),
            Some(record.payload_json.as_str()),
        )?;

        drop(conn);
        *self.arr_grabs_cache.write() = None;

        // Push the new/updated grab straight to subscribed WebSocket clients (Dashboard's and
        // PipelineBrowser's "Conduit's Scent Trail" widgets) instead of making them poll every 5s.
        let mut published = record.clone();
        published.id = target_id;
        self.publish_event(crate::events::TOPIC_PIPELINE, &published);

        Ok(())
    }

    /// Shared by `save_arr_grab` and `mark_grab_status` — both hold `self.conn`'s lock already
    /// when they need this, so it takes an already-locked `Connection` rather than `&self` to
    /// avoid re-entering a non-reentrant mutex.
    #[allow(clippy::too_many_arguments)]
    fn insert_grab_history_row(
        conn: &rusqlite::Connection,
        grab_id: &str,
        event_type: &str,
        status: &str,
        release_title: Option<&str>,
        scene_name: Option<&str>,
        quality: Option<&str>,
        size_bytes: Option<i64>,
        indexer: Option<&str>,
        download_client: Option<&str>,
        download_id: Option<&str>,
        payload_json: Option<&str>,
    ) -> anyhow::Result<()> {
        conn.execute(
            "INSERT INTO arr_grab_history (
                grab_id, event_type, status, release_title, scene_name, quality, size_bytes,
                indexer, download_client, download_id, payload_json, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                grab_id, event_type, status, release_title, scene_name, quality, size_bytes,
                indexer, download_client, download_id, payload_json, Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    /// Full timeline for one canonical grab id, oldest first — see `arr_grab_history`.
    pub fn list_grab_history(&self, grab_id: &str) -> anyhow::Result<Vec<ArrGrabHistoryEntry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, grab_id, event_type, status, release_title, scene_name, quality, size_bytes,
                    indexer, download_client, download_id, payload_json, created_at
             FROM arr_grab_history WHERE grab_id = ?1 ORDER BY created_at ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![grab_id], |row| {
            Ok(ArrGrabHistoryEntry {
                id: row.get(0)?,
                grab_id: row.get(1)?,
                event_type: row.get(2)?,
                status: row.get(3)?,
                release_title: row.get(4)?,
                scene_name: row.get(5)?,
                quality: row.get(6)?,
                size_bytes: row.get(7)?,
                indexer: row.get(8)?,
                download_client: row.get(9)?,
                download_id: row.get(10)?,
                payload_json: row.get(11)?,
                created_at: row.get(12)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    fn map_arr_grab_row(row: &rusqlite::Row) -> rusqlite::Result<ArrGrabRecord> {
        let created_str: String = row.get(30)?;
        let updated_str: String = row.get(31)?;
        let created_at = DateTime::parse_from_rfc3339(&created_str).map(|dt| dt.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now());
        let updated_at = DateTime::parse_from_rfc3339(&updated_str).map(|dt| dt.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now());

        Ok(ArrGrabRecord {
            id: row.get(0)?,
            scene_name: row.get(1)?,
            release_title: row.get(2)?,
            event_type: row.get(3)?,
            item_type: row.get(4)?,
            series_id: row.get(5)?,
            movie_id: row.get(6)?,
            season_number: row.get(7)?,
            episode_numbers: row.get(8)?,
            episode_ids: row.get(9)?,
            indexer: row.get(10)?,
            download_client: row.get(11)?,
            download_id: row.get(12)?,
            status: row.get(13)?,
            re_searched: row.get::<_, i32>(14)? != 0,
            re_search_count: row.get(15)?,
            title: row.get(16)?,
            year: row.get(17)?,
            overview: row.get(18)?,
            poster_url: row.get(19)?,
            genres: row.get(20)?,
            quality: row.get(21)?,
            size_bytes: row.get(22)?,
            imdb_id: row.get(23)?,
            tmdb_id: row.get(24)?,
            tvdb_id: row.get(25)?,
            runtime_mins: row.get(26)?,
            rating: row.get(27)?,
            mattermost_post_id: row.get(28)?,
            payload_json: row.get(29)?,
            created_at,
            updated_at,
            artist_id: row.get(32)?,
            album_id: row.get(33)?,
            zone_id: row.get(34)?,
        })
    }

    pub fn list_arr_grabs(&self, limit: usize) -> anyhow::Result<Vec<ArrGrabRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, scene_name, release_title, event_type, item_type,
                    series_id, movie_id, season_number, episode_numbers, episode_ids,
                    indexer, download_client, download_id, status, re_searched,
                    re_search_count, title, year, overview, poster_url, genres,
                    quality, size_bytes, imdb_id, tmdb_id, tvdb_id, runtime_mins, rating,
                    mattermost_post_id, payload_json, created_at, updated_at, artist_id, album_id, zone_id
             FROM arr_grabs
             ORDER BY created_at DESC LIMIT ?1"
        )?;

        let rows = stmt.query_map(params![limit as i64], Self::map_arr_grab_row)?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn list_pipeline_items(&self, limit: usize, app_filter: Option<&str>, status_filter: Option<&str>, search: Option<&str>, zone_filter: Option<&str>) -> anyhow::Result<Vec<ArrGrabRecord>> {
        let conn = self.conn.lock();
        let mut sql = "SELECT id, scene_name, release_title, event_type, item_type,
                              series_id, movie_id, season_number, episode_numbers, episode_ids,
                              indexer, download_client, download_id, status, re_searched,
                              re_search_count, title, year, overview, poster_url, genres,
                              quality, size_bytes, imdb_id, tmdb_id, tvdb_id, runtime_mins, rating,
                              mattermost_post_id, payload_json, created_at, updated_at, artist_id, album_id, zone_id
                       FROM arr_grabs WHERE 1=1".to_string();

        let mut param_values: Vec<rusqlite::types::Value> = Vec::new();

        if let Some(app) = app_filter {
            if app != "all" && !app.is_empty() {
                if app == "radarr" || app == "movie" {
                    sql.push_str(" AND item_type = 'movie'");
                } else if app == "sonarr" || app == "series" || app == "tv" {
                    sql.push_str(" AND (item_type = 'series' OR item_type = 'tv')");
                } else if app == "lidarr" || app == "music" {
                    sql.push_str(" AND item_type = 'music'");
                }
            }
        }

        if let Some(st) = status_filter {
            let st_clean = st.trim().to_lowercase();
            if !st_clean.is_empty() && st_clean != "all" {
                match st_clean.as_str() {
                    "deleted" => {
                        sql.push_str(" AND (status = 'deleted' OR event_type IN ('MovieDelete', 'SeriesDelete', 'ArtistDelete', 'AlbumDelete', 'MovieFileDelete', 'EpisodeFileDelete', 'TrackFileDelete') OR id IN (SELECT DISTINCT grab_id FROM arr_grab_history WHERE event_type IN ('MovieDelete', 'SeriesDelete', 'ArtistDelete', 'AlbumDelete', 'MovieFileDelete', 'EpisodeFileDelete', 'TrackFileDelete', 'delete', 'deleted') OR status = 'deleted'))");
                    }
                    "replaced" => {
                        sql.push_str(" AND (status IN ('replaced', 're-searched') OR re_searched = 1 OR re_search_count > 0 OR id IN (SELECT DISTINCT grab_id FROM arr_grab_history WHERE event_type IN ('ReSearch', 're-search', 're-searched', 'replaced', 'trumped') OR status IN ('re-searched', 'replaced')))");
                    }
                    "imported" => {
                        sql.push_str(" AND status IN ('imported', 'downloaded')");
                    }
                    "fetched" => {
                        sql.push_str(" AND status IN ('fetched', 'grabbed', 'downloading')");
                    }
                    other => {
                        sql.push_str(" AND status = ?");
                        param_values.push(other.to_string().into());
                    }
                }
            }
        }

        if let Some(z) = zone_filter {
            if z != "all" && !z.is_empty() {
                sql.push_str(" AND zone_id = ?");
                param_values.push(z.to_string().into());
            }
        }

        if let Some(s) = search {
            let s_trim = s.trim();
            if !s_trim.is_empty() {
                sql.push_str(" AND (title LIKE ? OR scene_name LIKE ? OR release_title LIKE ? OR indexer LIKE ?)");
                let pattern = format!("%{}%", s_trim);
                param_values.push(pattern.clone().into());
                param_values.push(pattern.clone().into());
                param_values.push(pattern.clone().into());
                param_values.push(pattern.into());
            }
        }

        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        param_values.push((limit as i64).into());

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(param_values), Self::map_arr_grab_row)?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_arr_grab_by_id(&self, id: &str) -> anyhow::Result<Option<ArrGrabRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, scene_name, release_title, event_type, item_type,
                    series_id, movie_id, season_number, episode_numbers, episode_ids,
                    indexer, download_client, download_id, status, re_searched,
                    re_search_count, title, year, overview, poster_url, genres,
                    quality, size_bytes, imdb_id, tmdb_id, tvdb_id, runtime_mins, rating,
                    mattermost_post_id, payload_json, created_at, updated_at, artist_id, album_id, zone_id
             FROM arr_grabs WHERE id = ?1",
        )?;

        let grab = stmt.query_row(params![id], Self::map_arr_grab_row).optional()?;
        Ok(grab)
    }

    pub fn delete_arr_grab(&self, id: &str) -> anyhow::Result<bool> {
        let affected = {
            let conn = self.conn.lock();
            conn.execute("DELETE FROM arr_grabs WHERE id = ?1", params![id])?
        };
        if affected > 0 {
            *self.arr_grabs_cache.write() = None;
        }
        Ok(affected > 0)
    }

    pub fn purge_arr_grabs(&self, app_filter: Option<&str>) -> anyhow::Result<usize> {
        let affected = {
            let conn = self.conn.lock();
            if let Some(app) = app_filter {
                if app == "radarr" || app == "movie" {
                    conn.execute("DELETE FROM arr_grabs WHERE item_type = 'movie'", [])?
                } else if app == "sonarr" || app == "series" || app == "tv" {
                    conn.execute("DELETE FROM arr_grabs WHERE item_type = 'series' OR item_type = 'tv'", [])?
                } else if app == "lidarr" || app == "music" {
                    conn.execute("DELETE FROM arr_grabs WHERE item_type = 'music'", [])?
                } else {
                    conn.execute("DELETE FROM arr_grabs", [])?
                }
            } else {
                conn.execute("DELETE FROM arr_grabs", [])?
            }
        };
        if affected > 0 {
            *self.arr_grabs_cache.write() = None;
        }
        Ok(affected)
    }

    pub fn get_arr_grabs_lookup_map(&self) -> std::collections::HashMap<String, ArrGrabRecord> {
        if let Some(cached) = self.arr_grabs_cache.read().as_ref() {
            return cached.clone();
        }

        // Prevent lock-order inversion deadlock (conn.lock vs arr_grabs_cache.write):
        // Query database while holding conn.lock, release conn.lock, then populate arr_grabs_cache.
        let map = {
            let conn = self.conn.lock();
            let mut map = std::collections::HashMap::new();

            let mut stmt = match conn.prepare(
                "SELECT id, scene_name, release_title, event_type, item_type,
                        series_id, movie_id, season_number, episode_numbers, episode_ids,
                        indexer, download_client, download_id, status, re_searched,
                        re_search_count, title, year, overview, poster_url, genres,
                        quality, size_bytes, imdb_id, tmdb_id, tvdb_id, runtime_mins, rating,
                        mattermost_post_id, payload_json, created_at, updated_at, artist_id, album_id, zone_id
                 FROM arr_grabs
                 ORDER BY created_at ASC"
            ) {
                Ok(s) => s,
                Err(_) => return map,
            };

            let rows = match stmt.query_map([], Self::map_arr_grab_row) {
                Ok(r) => r,
                Err(_) => return map,
            };

            for row in rows.flatten() {
                if let Some(ref did) = row.download_id {
                    let did_clean = did.trim().to_lowercase();
                    if !did_clean.is_empty() {
                        map.insert(did_clean.clone(), row.clone());

                        // If download_id has the form "<client>_<filename>_<timestamp>", index the inner filename
                        if did_clean.contains('_') {
                            let parts: Vec<&str> = did_clean.split('_').collect();
                            if parts.len() >= 3 {
                                let inner_name = parts[1..parts.len() - 1].join("_");
                                map.insert(inner_name.clone(), row.clone());
                                for ext in &[".mkv", ".mp4", ".avi", ".ts", ".flac", ".mp3"] {
                                    if let Some(stem) = inner_name.strip_suffix(ext) {
                                        map.insert(stem.to_string(), row.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                let scene_clean = row.scene_name.trim().to_lowercase();
                if !scene_clean.is_empty() {
                    map.insert(scene_clean.clone(), row.clone());
                    for ext in &[".mkv", ".mp4", ".avi", ".ts", ".flac", ".mp3"] {
                        if let Some(stem) = scene_clean.strip_suffix(ext) {
                            map.insert(stem.to_string(), row.clone());
                        }
                    }
                }
                let rel_clean = row.release_title.trim().to_lowercase();
                if !rel_clean.is_empty() {
                    map.insert(rel_clean.clone(), row.clone());
                    for ext in &[".mkv", ".mp4", ".avi", ".ts", ".flac", ".mp3"] {
                        if let Some(stem) = rel_clean.strip_suffix(ext) {
                            map.insert(stem.to_string(), row.clone());
                        }
                    }
                }
            }
            map
        };

        *self.arr_grabs_cache.write() = Some(map.clone());
        map
    }

    pub fn find_arr_grab_by_hash_or_name(&self, hash_or_download_id: &str, torrent_name: &str) -> anyhow::Result<Option<ArrGrabRecord>> {
        let conn = self.conn.lock();
        let hash_clean = hash_or_download_id.trim();

        // 1. Try exact download_id / hash match using index
        if !hash_clean.is_empty() {
            let mut stmt = conn.prepare(
                "SELECT id, scene_name, release_title, event_type, item_type,
                        series_id, movie_id, season_number, episode_numbers, episode_ids,
                        indexer, download_client, download_id, status, re_searched,
                        re_search_count, title, year, overview, poster_url, genres,
                        quality, size_bytes, imdb_id, tmdb_id, tvdb_id, runtime_mins, rating,
                        mattermost_post_id, payload_json, created_at, updated_at, artist_id, album_id, zone_id
                 FROM arr_grabs
                 WHERE download_id = ?1 COLLATE NOCASE
                 ORDER BY created_at DESC LIMIT 1"
            )?;

            let grab = stmt.query_row(params![hash_clean], Self::map_arr_grab_row).optional()?;
            if grab.is_some() {
                return Ok(grab);
            }
        }

        // 2. Try exact name match on indexed fields, with filename/stem fallback
        let clean_name = torrent_name.trim();
        if !clean_name.is_empty() {
            let mut candidates = Vec::new();
            candidates.push(clean_name.to_string());

            // If name is a full path, extract basename and stem
            if clean_name.contains('/') || clean_name.contains('\\') {
                let p = std::path::Path::new(clean_name);
                if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
                    if !candidates.contains(&file_name.to_string()) {
                        candidates.push(file_name.to_string());
                    }
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        if !candidates.contains(&stem.to_string()) {
                            candidates.push(stem.to_string());
                        }
                    }
                }
            } else {
                // Strip common video/audio extensions if present
                for ext in &[".mkv", ".mp4", ".avi", ".ts", ".flac", ".mp3"] {
                    if let Some(stem) = clean_name.strip_suffix(ext) {
                        if !candidates.contains(&stem.to_string()) {
                            candidates.push(stem.to_string());
                        }
                    }
                }
            }

            for candidate in candidates {
                let mut stmt = conn.prepare(
                    "SELECT id, scene_name, release_title, event_type, item_type,
                            series_id, movie_id, season_number, episode_numbers, episode_ids,
                            indexer, download_client, download_id, status, re_searched,
                            re_search_count, title, year, overview, poster_url, genres,
                            quality, size_bytes, imdb_id, tmdb_id, tvdb_id, runtime_mins, rating,
                            mattermost_post_id, payload_json, created_at, updated_at, artist_id, album_id, zone_id
                     FROM arr_grabs
                     WHERE scene_name = ?1 COLLATE NOCASE OR release_title = ?1 COLLATE NOCASE OR title = ?1 COLLATE NOCASE
                     ORDER BY created_at DESC LIMIT 1"
                )?;

                let grab = stmt.query_row(params![candidate], Self::map_arr_grab_row).optional()?;
                if grab.is_some() {
                    return Ok(grab);
                }
            }
        }

        Ok(None)
    }

    pub fn find_arr_grab_by_series_episode(
        &self,
        series_id: Option<i64>,
        season_number: Option<i64>,
        episode_number: Option<i64>,
        episode_id: Option<i64>,
    ) -> anyhow::Result<Option<ArrGrabRecord>> {
        let conn = self.conn.lock();
        if let Some(s_id) = series_id {
            if let Some(ep_id) = episode_id {
                let ep_pattern = format!("%{}%", ep_id);
                let mut stmt = conn.prepare(
                    "SELECT id, scene_name, release_title, event_type, item_type,
                            series_id, movie_id, season_number, episode_numbers, episode_ids,
                            indexer, download_client, download_id, status, re_searched,
                            re_search_count, title, year, overview, poster_url, genres,
                            quality, size_bytes, imdb_id, tmdb_id, tvdb_id, runtime_mins, rating,
                            mattermost_post_id, payload_json, created_at, updated_at, artist_id, album_id, zone_id
                     FROM arr_grabs
                     WHERE series_id = ?1 AND episode_ids LIKE ?2
                     ORDER BY created_at DESC LIMIT 1"
                )?;
                let grab = stmt.query_row(params![s_id, ep_pattern], Self::map_arr_grab_row).optional()?;
                if grab.is_some() {
                    return Ok(grab);
                }
            }

            if let (Some(s_num), Some(ep_num)) = (season_number, episode_number) {
                let ep_pattern = format!("%{}%", ep_num);
                let mut stmt = conn.prepare(
                    "SELECT id, scene_name, release_title, event_type, item_type,
                            series_id, movie_id, season_number, episode_numbers, episode_ids,
                            indexer, download_client, download_id, status, re_searched,
                            re_search_count, title, year, overview, poster_url, genres,
                            quality, size_bytes, imdb_id, tmdb_id, tvdb_id, runtime_mins, rating,
                            mattermost_post_id, payload_json, created_at, updated_at, artist_id, album_id, zone_id
                     FROM arr_grabs
                     WHERE series_id = ?1 AND season_number = ?2 AND (episode_numbers LIKE ?3 OR episode_numbers IS NULL)
                     ORDER BY created_at DESC LIMIT 1"
                )?;
                let grab = stmt.query_row(params![s_id, s_num, ep_pattern], Self::map_arr_grab_row).optional()?;
                if grab.is_some() {
                    return Ok(grab);
                }
            }
        }
        Ok(None)
    }

    pub fn find_arr_grab_by_ids_or_title(
        &self,
        movie_id: Option<i64>,
        series_id: Option<i64>,
        imdb_id: Option<&str>,
        tmdb_id: Option<i64>,
        tvdb_id: Option<i64>,
        title: Option<&str>,
    ) -> anyhow::Result<Option<ArrGrabRecord>> {
        let conn = self.conn.lock();
        let title_clean = title.unwrap_or("").trim().to_string();

        let mut stmt = conn.prepare(
            "SELECT id, scene_name, release_title, event_type, item_type,
                    series_id, movie_id, season_number, episode_numbers, episode_ids,
                    indexer, download_client, download_id, status, re_searched,
                    re_search_count, title, year, overview, poster_url, genres,
                    quality, size_bytes, imdb_id, tmdb_id, tvdb_id, runtime_mins, rating,
                    mattermost_post_id, payload_json, created_at, updated_at, artist_id, album_id, zone_id
             FROM arr_grabs
             WHERE (movie_id IS NOT NULL AND ?1 IS NOT NULL AND movie_id = ?1)
                OR (series_id IS NOT NULL AND ?2 IS NOT NULL AND series_id = ?2)
                OR (imdb_id IS NOT NULL AND ?3 IS NOT NULL AND imdb_id != '' AND ?3 != '' AND LOWER(imdb_id) = LOWER(?3))
                OR (tmdb_id IS NOT NULL AND ?4 IS NOT NULL AND tmdb_id = ?4)
                OR (tvdb_id IS NOT NULL AND ?5 IS NOT NULL AND tvdb_id = ?5)
                OR (?6 != '' AND (LOWER(title) = LOWER(?6) OR ?6 LIKE '%' || title || '%' OR title LIKE '%' || ?6 || '%'))
             ORDER BY created_at DESC LIMIT 1"
        )?;

        let grab = stmt.query_row(
            params![movie_id, series_id, imdb_id, tmdb_id, tvdb_id, title_clean],
            Self::map_arr_grab_row,
        ).optional()?;
        Ok(grab)
    }

    pub fn mark_grab_status(&self, id: &str, status: &str, re_search_increment: bool) -> anyhow::Result<()> {
        struct ReleaseSnapshot {
            release_title: String,
            scene_name: String,
            quality: Option<String>,
            size_bytes: Option<i64>,
            indexer: Option<String>,
            download_client: Option<String>,
            download_id: Option<String>,
        }

        let conn = self.conn.lock();
        let now_str = Utc::now().to_rfc3339();

        // Snapshot the release detail that's about to become "the previous one" before this
        // status change — the UPDATE below doesn't touch these fields, but capturing them now
        // (rather than leaving the history row blank) is what makes "which release was this"
        // legible later, once a subsequent re-grab overwrites arr_grabs' own current-state row.
        let release_snapshot: Option<ReleaseSnapshot> = conn.query_row(
            "SELECT release_title, scene_name, quality, size_bytes, indexer, download_client, download_id FROM arr_grabs WHERE id = ?1",
            params![id],
            |row| Ok(ReleaseSnapshot {
                release_title: row.get(0)?,
                scene_name: row.get(1)?,
                quality: row.get(2)?,
                size_bytes: row.get(3)?,
                indexer: row.get(4)?,
                download_client: row.get(5)?,
                download_id: row.get(6)?,
            }),
        ).optional()?;

        if re_search_increment {
            conn.execute(
                "UPDATE arr_grabs SET status = ?1, re_searched = 1, re_search_count = re_search_count + 1, updated_at = ?2 WHERE id = ?3",
                params![status, now_str, id],
            )?;
        } else {
            conn.execute(
                "UPDATE arr_grabs SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status, now_str, id],
            )?;
        }

        let event_type = if re_search_increment { "ReSearch" } else { "StatusChange" };
        if let Some(s) = release_snapshot {
            Self::insert_grab_history_row(
                &conn, id, event_type, status,
                Some(&s.release_title), Some(&s.scene_name), s.quality.as_deref(), s.size_bytes,
                s.indexer.as_deref(), s.download_client.as_deref(), s.download_id.as_deref(), None,
            )?;
        } else {
            Self::insert_grab_history_row(&conn, id, event_type, status, None, None, None, None, None, None, None, None)?;
        }

        Ok(())
    }

    pub fn search_arr_grabs(&self, query: Option<&str>, status: Option<&str>, zone_filter: Option<&str>, limit: usize) -> anyhow::Result<Vec<ArrGrabRecord>> {
        let conn = self.conn.lock();
        let mut sql = "SELECT id, scene_name, release_title, event_type, item_type,
                              series_id, movie_id, season_number, episode_numbers, episode_ids,
                              indexer, download_client, download_id, status, re_searched,
                              re_search_count, title, year, overview, poster_url, genres,
                              quality, size_bytes, imdb_id, tmdb_id, tvdb_id, runtime_mins, rating,
                              mattermost_post_id, payload_json, created_at, updated_at, artist_id, album_id, zone_id
                       FROM arr_grabs WHERE 1=1".to_string();

        let mut param_vals: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(q) = query {
            if !q.trim().is_empty() {
                sql.push_str(" AND (title LIKE ? OR scene_name LIKE ? OR release_title LIKE ? OR indexer LIKE ? OR id LIKE ? OR download_id LIKE ?)");
                let pattern = format!("%{}%", q.trim());
                param_vals.push(Box::new(pattern.clone()));
                param_vals.push(Box::new(pattern.clone()));
                param_vals.push(Box::new(pattern.clone()));
                param_vals.push(Box::new(pattern.clone()));
                param_vals.push(Box::new(pattern.clone()));
                param_vals.push(Box::new(pattern));
            }
        }

        if let Some(st) = status {
            let st_clean = st.trim().to_lowercase();
            if !st_clean.is_empty() && st_clean != "all" {
                match st_clean.as_str() {
                    "deleted" => {
                        sql.push_str(" AND (status = 'deleted' OR event_type IN ('MovieDelete', 'SeriesDelete', 'ArtistDelete', 'AlbumDelete', 'MovieFileDelete', 'EpisodeFileDelete', 'TrackFileDelete') OR id IN (SELECT DISTINCT grab_id FROM arr_grab_history WHERE event_type IN ('MovieDelete', 'SeriesDelete', 'ArtistDelete', 'AlbumDelete', 'MovieFileDelete', 'EpisodeFileDelete', 'TrackFileDelete', 'delete', 'deleted') OR status = 'deleted'))");
                    }
                    "replaced" => {
                        sql.push_str(" AND (status IN ('replaced', 're-searched') OR re_searched = 1 OR re_search_count > 0 OR id IN (SELECT DISTINCT grab_id FROM arr_grab_history WHERE event_type IN ('ReSearch', 're-search', 're-searched', 'replaced', 'trumped') OR status IN ('re-searched', 'replaced')))");
                    }
                    "imported" => {
                        sql.push_str(" AND status IN ('imported', 'downloaded')");
                    }
                    "fetched" => {
                        sql.push_str(" AND status IN ('fetched', 'grabbed', 'downloading')");
                    }
                    other => {
                        sql.push_str(" AND status = ?");
                        param_vals.push(Box::new(other.to_string()));
                    }
                }
            }
        }

        if let Some(z) = zone_filter {
            if z != "all" && !z.is_empty() {
                sql.push_str(" AND zone_id = ?");
                param_vals.push(Box::new(z.to_string()));
            }
        }

        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        param_vals.push(Box::new(limit as i64));

        let mut stmt = conn.prepare(&sql)?;
        let params_slice: Vec<&dyn rusqlite::ToSql> = param_vals.iter().map(|b| b.as_ref()).collect();

        let rows = stmt.query_map(params_slice.as_slice(), Self::map_arr_grab_row)?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // --- Bandwidth History (Downsampled) ---
    /// Inserts one minute-averaged bandwidth sample and prunes anything older than 72h in the
    /// same call — the table never holds more than ~4320 rows, so this is cheap every time.
    pub fn insert_bandwidth_point(&self, time_ms: i64, download_speed: i64, upload_speed: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO bandwidth_history_downsampled (time, download_speed, upload_speed) VALUES (?1, ?2, ?3)",
            params![time_ms, download_speed, upload_speed],
        )?;
        let cutoff = time_ms - 72 * 3600 * 1000;
        conn.execute("DELETE FROM bandwidth_history_downsampled WHERE time < ?1", params![cutoff])?;
        Ok(())
    }

    /// Returns downsampled points from the last `since_ms` milliseconds, oldest first.
    pub fn list_bandwidth_history(&self, since_ms: i64) -> anyhow::Result<Vec<crate::fetcher::BandwidthPoint>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT time, download_speed, upload_speed FROM bandwidth_history_downsampled WHERE time >= ?1 ORDER BY time ASC",
        )?;
        let rows = stmt.query_map(params![since_ms], |row| {
            Ok(crate::fetcher::BandwidthPoint {
                time: row.get(0)?,
                download_speed: row.get(1)?,
                upload_speed: row.get(2)?,
                top_torrents: Vec::new(),
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // --- Event Logs ---
    pub fn log_event(&self, event_type: &str, level: &str, message: &str, details_json: Option<&str>) -> anyhow::Result<()> {
        let created_at = Utc::now();
        let id = {
            let conn = self.conn.lock();
            conn.execute(
                "INSERT INTO event_logs (event_type, level, message, details_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![event_type, level, message, details_json, created_at.to_rfc3339()],
            )?;
            conn.last_insert_rowid()
        };

        // Push the new row straight to any subscribed WebSocket clients instead of making them
        // poll — this was never a latency problem (SQLite reads are fast), just a "why poll for
        // something we can push the instant it happens" one.
        self.publish_event(crate::events::TOPIC_EVENTS, &EventLogRecord {
            id,
            event_type: event_type.to_string(),
            level: level.to_string(),
            message: message.to_string(),
            details_json: details_json.map(|s| s.to_string()),
            created_at,
        });

        Ok(())
    }

    pub fn get_event_logs(&self, limit: usize) -> anyhow::Result<Vec<EventLogRecord>> {
        self.search_event_logs(None, None, None, limit)
    }

    pub fn search_event_logs(
        &self,
        level: Option<&str>,
        event_type: Option<&str>,
        query: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<EventLogRecord>> {
        let conn = self.conn.lock();
        let mut sql = "SELECT id, event_type, level, message, details_json, created_at FROM event_logs WHERE 1=1".to_string();
        let mut param_vals: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(lvl) = level {
            match lvl.trim().to_lowercase().as_str() {
                "errors_warnings" | "errors_and_warnings" | "warn_error" => {
                    sql.push_str(" AND level IN ('error', 'warn', 'warning')");
                }
                "error" => {
                    sql.push_str(" AND level = 'error'");
                }
                "warn" | "warning" => {
                    sql.push_str(" AND level IN ('warn', 'warning')");
                }
                "info" => {
                    sql.push_str(" AND level = 'info'");
                }
                "debug" => {
                    sql.push_str(" AND level = 'debug'");
                }
                _ => {} // "all" or unrecognized
            }
        }

        if let Some(et) = event_type {
            if !et.trim().is_empty() && et != "all" {
                sql.push_str(" AND event_type LIKE ?");
                param_vals.push(Box::new(format!("%{}%", et.trim())));
            }
        }

        if let Some(q) = query {
            if !q.trim().is_empty() {
                sql.push_str(" AND (message LIKE ? OR event_type LIKE ? OR details_json LIKE ?)");
                let pattern = format!("%{}%", q.trim());
                param_vals.push(Box::new(pattern.clone()));
                param_vals.push(Box::new(pattern.clone()));
                param_vals.push(Box::new(pattern));
            }
        }

        sql.push_str(" ORDER BY id DESC LIMIT ?");
        param_vals.push(Box::new(limit as i64));

        let mut stmt = conn.prepare(&sql)?;
        let params_slice: Vec<&dyn rusqlite::ToSql> = param_vals.iter().map(|b| b.as_ref()).collect();

        let rows = stmt.query_map(params_slice.as_slice(), |row| {
            let created_str: String = row.get(5)?;
            let created_at = DateTime::parse_from_rfc3339(&created_str).map(|dt| dt.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now());
            Ok(EventLogRecord {
                id: row.get(0)?,
                event_type: row.get(1)?,
                level: row.get(2)?,
                message: row.get(3)?,
                details_json: row.get(4)?,
                created_at,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // --- Tracker Circuit Breakers State Persistence ---
    pub fn save_circuit_breaker(&self, breaker: &crate::fetcher::ActiveCircuitBreaker) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        let paused_json = serde_json::to_string(&breaker.paused_torrents).unwrap_or_else(|_| "[]".to_string());
        let now = Utc::now().to_rfc3339();
        let state = if breaker.state.is_empty() { "tripped" } else { &breaker.state };
        conn.execute(
            "INSERT INTO circuit_breakers (
                tracker_host, canary_compound_id, canary_name, paused_torrents_json,
                failing_error, tripped_at, created_at, updated_at,
                state, recovery_started_at, consecutive_successes, backoff_secs
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(tracker_host) DO UPDATE SET
                canary_compound_id = excluded.canary_compound_id,
                canary_name = excluded.canary_name,
                paused_torrents_json = excluded.paused_torrents_json,
                failing_error = excluded.failing_error,
                tripped_at = excluded.tripped_at,
                updated_at = excluded.updated_at,
                state = excluded.state,
                recovery_started_at = excluded.recovery_started_at,
                consecutive_successes = excluded.consecutive_successes,
                backoff_secs = excluded.backoff_secs",
            params![
                breaker.tracker_host,
                breaker.canary_compound_id,
                breaker.canary_name,
                paused_json,
                breaker.failing_error,
                breaker.tripped_at,
                now,
                now,
                state,
                breaker.recovery_started_at,
                breaker.consecutive_successes,
                breaker.backoff_secs,
            ],
        )?;
        Ok(())
    }

    pub fn remove_circuit_breaker(&self, tracker_host: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM circuit_breakers WHERE tracker_host = ?1",
            params![tracker_host],
        )?;
        Ok(())
    }

    pub fn list_circuit_breakers(&self) -> anyhow::Result<Vec<crate::fetcher::ActiveCircuitBreaker>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT tracker_host, canary_compound_id, canary_name, paused_torrents_json, failing_error, tripped_at,
                    state, recovery_started_at, consecutive_successes, backoff_secs
             FROM circuit_breakers ORDER BY tripped_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let tracker_host: String = row.get(0)?;
            let canary_compound_id: String = row.get(1)?;
            let canary_name: String = row.get(2)?;
            let paused_json: String = row.get(3)?;
            let failing_error: String = row.get(4)?;
            let tripped_at: i64 = row.get(5)?;
            let state: String = row.get(6)?;
            let recovery_started_at: Option<i64> = row.get(7)?;
            let consecutive_successes: u32 = row.get(8)?;
            let backoff_secs: i64 = row.get(9)?;
            let paused_torrents: Vec<String> = serde_json::from_str(&paused_json).unwrap_or_default();

            Ok(crate::fetcher::ActiveCircuitBreaker {
                tracker_host,
                canary_compound_id,
                canary_name,
                paused_torrents,
                failing_error,
                tripped_at,
                state,
                recovery_started_at,
                consecutive_successes,
                backoff_secs,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // --- Plex Scrobbles & Watch History ---
    pub fn save_plex_scrobble(&self, scrobble: &PlexScrobbleRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO plex_scrobbles (
                id, event, user_name, media_type, title, series_title,
                season_number, episode_number, year, imdb_id, tmdb_id, tvdb_id,
                rating_key, duration_ms, view_offset_ms, trakt_synced, raw_json, created_at,
                source_node_name
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                scrobble.id,
                scrobble.event,
                scrobble.user_name,
                scrobble.media_type,
                scrobble.title,
                scrobble.series_title,
                scrobble.season_number,
                scrobble.episode_number,
                scrobble.year,
                scrobble.imdb_id,
                scrobble.tmdb_id,
                scrobble.tvdb_id,
                scrobble.rating_key,
                scrobble.duration_ms,
                scrobble.view_offset_ms,
                if scrobble.trakt_synced { 1 } else { 0 },
                scrobble.raw_json,
                scrobble.created_at.to_rfc3339(),
                scrobble.source_node_name,
            ],
        )?;
        Ok(())
    }

    pub fn list_plex_scrobbles(
        &self,
        limit: usize,
        offset: usize,
        event_filter: Option<&str>,
        user_filter: Option<&str>,
    ) -> anyhow::Result<(Vec<PlexScrobbleRecord>, usize)> {
        let conn = self.conn.lock();
        let mut conditions = Vec::new();
        let mut params_vec: Vec<rusqlite::types::Value> = Vec::new();

        if let Some(e) = event_filter {
            if !e.is_empty() && e != "all" {
                conditions.push("event = ?".to_string());
                params_vec.push(e.to_string().into());
            }
        }
        if let Some(u) = user_filter {
            if !u.is_empty() {
                conditions.push("user_name LIKE ?".to_string());
                params_vec.push(format!("%{}%", u).into());
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let count_query = format!("SELECT COUNT(*) FROM plex_scrobbles {}", where_clause);
        let mut count_stmt = conn.prepare(&count_query)?;
        let count_params = rusqlite::params_from_iter(params_vec.iter());
        let total: usize = count_stmt.query_row(count_params, |r| r.get(0))?;

        let query = format!(
            "SELECT id, event, user_name, media_type, title, series_title,
                    season_number, episode_number, year, imdb_id, tmdb_id, tvdb_id,
                    rating_key, duration_ms, view_offset_ms, trakt_synced, raw_json, created_at,
                    source_node_name
             FROM plex_scrobbles {}
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
            where_clause
        );

        let mut query_params = params_vec;
        query_params.push((limit as i64).into());
        query_params.push((offset as i64).into());

        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(query_params.iter()), Self::map_plex_scrobble_row)?;

        let mut items = Vec::new();
        for r in rows {
            items.push(r?);
        }
        Ok((items, total))
    }

    fn map_plex_scrobble_row(row: &rusqlite::Row) -> rusqlite::Result<PlexScrobbleRecord> {
        let created_str: String = row.get(17)?;
        let created_at = DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let trakt_synced_int: i32 = row.get(15)?;

        Ok(PlexScrobbleRecord {
            id: row.get(0)?,
            event: row.get(1)?,
            user_name: row.get(2)?,
            media_type: row.get(3)?,
            title: row.get(4)?,
            series_title: row.get(5)?,
            season_number: row.get(6)?,
            episode_number: row.get(7)?,
            year: row.get(8)?,
            imdb_id: row.get(9)?,
            tmdb_id: row.get(10)?,
            tvdb_id: row.get(11)?,
            rating_key: row.get(12)?,
            duration_ms: row.get(13)?,
            view_offset_ms: row.get(14)?,
            trakt_synced: trakt_synced_int == 1,
            raw_json: row.get(16)?,
            created_at,
            source_node_name: row.get(18)?,
        })
    }

    /// Unsynced scrobbles originating from a `sync_watch_status=true` Plex node, oldest first —
    /// the actual eligible set for `engines::watch_sync`'s push-to-Trakt pass. Scrobbles from
    /// non-participating (or unmatched-origin) servers are excluded here but still visible via
    /// `list_plex_scrobbles` for the UI.
    pub fn list_unsynced_scrobbles_from_nodes(&self, node_names: &[String], limit: usize) -> anyhow::Result<Vec<PlexScrobbleRecord>> {
        if node_names.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock();
        let placeholders = node_names.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT id, event, user_name, media_type, title, series_title,
                    season_number, episode_number, year, imdb_id, tmdb_id, tvdb_id,
                    rating_key, duration_ms, view_offset_ms, trakt_synced, raw_json, created_at,
                    source_node_name
             FROM plex_scrobbles
             WHERE trakt_synced = 0 AND source_node_name IN ({})
             ORDER BY created_at ASC LIMIT ?",
            placeholders
        );
        let mut params_vec: Vec<rusqlite::types::Value> = node_names.iter().map(|n| n.clone().into()).collect();
        params_vec.push((limit as i64).into());

        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec.iter()), Self::map_plex_scrobble_row)?;
        let mut items = Vec::new();
        for r in rows {
            items.push(r?);
        }
        Ok(items)
    }

    // --- Generic sync-state key/value store ---
    pub fn get_sync_state(&self, key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock();
        conn.query_row("SELECT value FROM sync_state WHERE key = ?1", params![key], |row| row.get(0))
            .optional()
            .map_err(Into::into)
    }

    pub fn set_sync_state(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO sync_state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn mark_plex_scrobble_trakt_synced(&self, id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE plex_scrobbles SET trakt_synced = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    // --- Ombi Media Requests ---
    pub fn save_ombi_request(&self, request: &OmbiRequestRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO ombi_requests (
                id, event_type, requested_by, media_type, title, year,
                overview, poster_url, imdb_id, tmdb_id, tvdb_id, status,
                mattermost_post_id, raw_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(id) DO UPDATE SET
                event_type = excluded.event_type,
                status = excluded.status,
                overview = COALESCE(excluded.overview, ombi_requests.overview),
                poster_url = COALESCE(excluded.poster_url, ombi_requests.poster_url),
                mattermost_post_id = COALESCE(excluded.mattermost_post_id, ombi_requests.mattermost_post_id),
                updated_at = excluded.updated_at",
            params![
                request.id,
                request.event_type,
                request.requested_by,
                request.media_type,
                request.title,
                request.year,
                request.overview,
                request.poster_url,
                request.imdb_id,
                request.tmdb_id,
                request.tvdb_id,
                request.status,
                request.mattermost_post_id,
                request.raw_json,
                request.created_at.to_rfc3339(),
                now,
            ],
        )?;
        Ok(())
    }

    pub fn find_ombi_request_by_id_or_title(&self, id: &str, title: &str) -> anyhow::Result<Option<OmbiRequestRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, event_type, requested_by, media_type, title, year,
                    overview, poster_url, imdb_id, tmdb_id, tvdb_id, status,
                    raw_json, created_at, updated_at, mattermost_post_id
             FROM ombi_requests
             WHERE (id = ?1 AND ?1 != '') OR (lower(title) = lower(?2) AND ?2 != '')
             ORDER BY created_at DESC LIMIT 1"
        )?;

        let mut rows = stmt.query(params![id, title])?;
        if let Some(row) = rows.next()? {
            let created_str: String = row.get(13)?;
            let created_at = DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let updated_str: String = row.get(14)?;
            let updated_at = DateTime::parse_from_rfc3339(&updated_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            Ok(Some(OmbiRequestRecord {
                id: row.get(0)?,
                event_type: row.get(1)?,
                requested_by: row.get(2)?,
                media_type: row.get(3)?,
                title: row.get(4)?,
                year: row.get(5)?,
                overview: row.get(6)?,
                poster_url: row.get(7)?,
                imdb_id: row.get(8)?,
                tmdb_id: row.get(9)?,
                tvdb_id: row.get(10)?,
                status: row.get(11)?,
                raw_json: row.get(12)?,
                created_at,
                updated_at,
                mattermost_post_id: row.get(15)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn find_ombi_request_by_title_or_ids(
        &self,
        title: &str,
        imdb_id: Option<&str>,
        tmdb_id: Option<i64>,
        tvdb_id: Option<i64>,
    ) -> anyhow::Result<Option<OmbiRequestRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, event_type, requested_by, media_type, title, year,
                    overview, poster_url, imdb_id, tmdb_id, tvdb_id, status,
                    raw_json, created_at, updated_at, mattermost_post_id
             FROM ombi_requests
             WHERE (lower(title) = lower(?1) AND ?1 != '')
                OR (imdb_id = ?2 AND ?2 IS NOT NULL AND ?2 != '')
                OR (tmdb_id = ?3 AND ?3 IS NOT NULL)
                OR (tvdb_id = ?4 AND ?4 IS NOT NULL)
             ORDER BY created_at DESC LIMIT 1"
        )?;

        let mut rows = stmt.query(params![title, imdb_id, tmdb_id, tvdb_id])?;
        if let Some(row) = rows.next()? {
            let created_str: String = row.get(13)?;
            let created_at = DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let updated_str: String = row.get(14)?;
            let updated_at = DateTime::parse_from_rfc3339(&updated_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            Ok(Some(OmbiRequestRecord {
                id: row.get(0)?,
                event_type: row.get(1)?,
                requested_by: row.get(2)?,
                media_type: row.get(3)?,
                title: row.get(4)?,
                year: row.get(5)?,
                overview: row.get(6)?,
                poster_url: row.get(7)?,
                imdb_id: row.get(8)?,
                tmdb_id: row.get(9)?,
                tvdb_id: row.get(10)?,
                status: row.get(11)?,
                raw_json: row.get(12)?,
                created_at,
                updated_at,
                mattermost_post_id: row.get(15)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_ombi_requests(
        &self,
        limit: usize,
        offset: usize,
        type_filter: Option<&str>,
        status_filter: Option<&str>,
    ) -> anyhow::Result<(Vec<OmbiRequestRecord>, usize)> {
        let conn = self.conn.lock();
        let mut conditions = Vec::new();
        let mut params_vec: Vec<rusqlite::types::Value> = Vec::new();

        if let Some(t) = type_filter {
            if !t.is_empty() && t != "all" {
                conditions.push("media_type = ?".to_string());
                params_vec.push(t.to_string().into());
            }
        }
        if let Some(s) = status_filter {
            if !s.is_empty() && s != "all" {
                conditions.push("status = ?".to_string());
                params_vec.push(s.to_string().into());
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let count_query = format!("SELECT COUNT(*) FROM ombi_requests {}", where_clause);
        let mut count_stmt = conn.prepare(&count_query)?;
        let count_params = rusqlite::params_from_iter(params_vec.iter());
        let total: usize = count_stmt.query_row(count_params, |r| r.get(0))?;

        let query = format!(
            "SELECT id, event_type, requested_by, media_type, title, year,
                    overview, poster_url, imdb_id, tmdb_id, tvdb_id, status,
                    raw_json, created_at, updated_at, mattermost_post_id
             FROM ombi_requests {}
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
            where_clause
        );

        let mut query_params = params_vec;
        query_params.push((limit as i64).into());
        query_params.push((offset as i64).into());

        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
            let created_str: String = row.get(13)?;
            let created_at = DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let updated_str: String = row.get(14)?;
            let updated_at = DateTime::parse_from_rfc3339(&updated_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            Ok(OmbiRequestRecord {
                id: row.get(0)?,
                event_type: row.get(1)?,
                requested_by: row.get(2)?,
                media_type: row.get(3)?,
                title: row.get(4)?,
                year: row.get(5)?,
                overview: row.get(6)?,
                poster_url: row.get(7)?,
                imdb_id: row.get(8)?,
                tmdb_id: row.get(9)?,
                tvdb_id: row.get(10)?,
                status: row.get(11)?,
                raw_json: row.get(12)?,
                created_at,
                updated_at,
                mattermost_post_id: row.get(15)?,
            })
        })?;

        let mut items = Vec::new();
        for r in rows {
            items.push(r?);
        }
        Ok((items, total))
    }
}
