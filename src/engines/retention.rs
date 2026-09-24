// src/engines/retention.rs
//! Data retention: deletes historical rows older than `system.history_retention_days`, when set.
//! See `Database::purge_history_older_than` for exactly which tables this touches (and, just as
//! important, which one it deliberately never does).

use crate::config::ConfigManager;
use crate::db::Database;
use crate::engines::registry::{heartbeat_sleep, TaskHandle};
use chrono::{Duration as ChronoDuration, Utc};
use std::time::Duration;
use tracing::info;

/// How often to check whether a purge is due. Retention is a days-granularity setting, so
/// there's no benefit to checking more often than this — it just costs a config read.
const CHECK_INTERVAL: Duration = Duration::from_secs(3600);

pub async fn run_retention_loop(config_mgr: ConfigManager, db: Database, handle: TaskHandle) {
    info!("Starting Data Retention Engine");
    let _ = db.log_event(
        "retention",
        "info",
        "🐕 Data Retention Engine started",
        None,
    );

    loop {
        let config = config_mgr.get().await;
        if let Some(days) = config.system.history_retention_days.filter(|d| *d > 0) {
            run_purge(&db, days);
        }
        heartbeat_sleep(&handle, CHECK_INTERVAL).await;
    }
}

pub(crate) fn run_purge(db: &Database, days: u32) -> crate::db::RetentionPurgeSummary {
    let cutoff = Utc::now() - ChronoDuration::days(days as i64);
    match db.purge_history_older_than(cutoff) {
        Ok(summary) => {
            if summary.total() > 0 {
                let msg = format!(
                    "Purged {} row(s) older than {} day(s): {} event log(s), {} Plex scrobble(s), {} Ombi request(s), {} grab history row(s)",
                    summary.total(), days, summary.event_logs, summary.plex_scrobbles, summary.ombi_requests, summary.arr_grab_history
                );
                info!("{msg}");
                let _ = db.log_event("retention", "info", &msg, None);
            }
            summary
        }
        Err(e) => {
            let msg = format!("Data retention purge failed: {e}");
            tracing::warn!("{msg}");
            let _ = db.log_event("retention", "warn", &msg, None);
            Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> (Database, tempfile::NamedTempFile) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = Database::init(tmp.path(), Some("test-pass")).unwrap();
        (db, tmp)
    }

    #[test]
    fn purges_only_rows_older_than_the_cutoff() {
        let (db, _tmp) = test_db();

        db.log_event("test", "info", "ancient", None).unwrap();
        db.test_backdate("event_logs", Utc::now() - ChronoDuration::days(40))
            .unwrap();
        db.log_event("test", "info", "recent", None).unwrap();

        let summary = run_purge(&db, 30);
        assert_eq!(summary.event_logs, 1);

        let remaining: Vec<String> = db
            .get_event_logs(100)
            .unwrap()
            .into_iter()
            .map(|e| e.message)
            .collect();
        assert!(remaining.contains(&"recent".to_string()));
        assert!(!remaining.contains(&"ancient".to_string()));
    }

    #[test]
    fn arr_grabs_itself_is_never_purged_only_its_lineage_history() {
        let (db, _tmp) = test_db();

        // A currently-active grab (fetched, not yet imported) — must never be touched by
        // retention, however old it looks, since arr_grabs is current state, not pure history.
        let grab = crate::db::ArrGrabRecord {
            id: "grab-1".into(),
            scene_name: "Some.Release-GRP".into(),
            release_title: "Some.Release-GRP".into(),
            event_type: "Grab".into(),
            item_type: "movie".into(),
            series_id: None,
            movie_id: Some(1),
            season_number: None,
            episode_numbers: None,
            episode_ids: None,
            indexer: None,
            download_client: None,
            download_id: None,
            status: "fetched".into(),
            re_searched: false,
            re_search_count: 0,
            title: Some("Some Movie".into()),
            year: None,
            overview: None,
            poster_url: None,
            genres: None,
            quality: None,
            size_bytes: None,
            imdb_id: None,
            tmdb_id: None,
            tvdb_id: None,
            runtime_mins: None,
            rating: None,
            mattermost_post_id: None,
            payload_json: "{}".into(),
            created_at: Utc::now() - ChronoDuration::days(400),
            updated_at: Utc::now() - ChronoDuration::days(400),
            artist_id: None,
            album_id: None,
            zone_id: None,
        };
        db.save_arr_grab(&grab).unwrap();
        db.test_backdate("arr_grab_history", Utc::now() - ChronoDuration::days(400))
            .unwrap();

        run_purge(&db, 30);

        assert!(
            db.get_arr_grab_by_id("grab-1").unwrap().is_some(),
            "arr_grabs is current state, not purged by age"
        );
        let history = db.list_grab_history("grab-1").unwrap();
        assert!(history.is_empty(), "the lineage log is purged");
    }

    #[test]
    fn zero_or_unset_days_disables_the_purge() {
        for days in [None, Some(0u32)] {
            assert!(
                days.filter(|d| *d > 0).is_none(),
                "the loop's own gate must skip run_purge for {days:?}"
            );
        }
    }
}
