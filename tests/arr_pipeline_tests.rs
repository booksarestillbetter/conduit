// tests/arr_pipeline_tests.rs
use conduit::db::{ArrGrabRecord, Database};
use chrono::Utc;
use tempfile::NamedTempFile;

#[test]
fn test_arr_grab_lifecycle_and_lookup() {
    let temp_db = NamedTempFile::new().expect("Failed to create temp db");
    let db = Database::init(temp_db.path(), Some("secret_db_pass")).expect("DB init failed");

    let grab_id = "4a5b6c7d8e9f0123456789abcdef";
    let now = Utc::now();

    // 1. Sonarr Grab
    let record = ArrGrabRecord {
        id: grab_id.to_string(),
        scene_name: "The.Bear.S03E01.1080p.WEB.H264".to_string(),
        release_title: "The.Bear.S03E01.1080p.WEB.H264".to_string(),
        event_type: "Grab".to_string(),
        item_type: "series".to_string(),
        series_id: Some(15),
        movie_id: None,
        artist_id: None,
        album_id: None,
        zone_id: None,
        season_number: Some(3),
        episode_numbers: Some("[1]".to_string()),
        episode_ids: Some("[401]".to_string()),
        indexer: Some("PTP".to_string()),
        download_client: Some("Transmission-Munro".to_string()),
        download_id: Some(grab_id.to_string()),
        status: "fetched".to_string(),
        re_searched: false,
        re_search_count: 0,
        title: Some("The Bear".to_string()),
        year: Some(2024),
        overview: Some("Carmy strives for culinary perfection.".to_string()),
        poster_url: Some("https://example.com/bear.jpg".to_string()),
        genres: Some("Drama, Comedy".to_string()),
        quality: Some("1080p WEB".to_string()),
        size_bytes: Some(1500000000),
        imdb_id: Some("tt14452776".to_string()),
        tmdb_id: None,
        tvdb_id: Some(399999),
        runtime_mins: Some(35),
        rating: Some(8.6),
        mattermost_post_id: None,
        payload_json: "{}".to_string(),
        created_at: now,
        updated_at: now,
    };

    db.save_arr_grab(&record).expect("Failed to save grab");

    // 2. Lookup by info hash
    let found_by_hash = db.find_arr_grab_by_hash_or_name(grab_id, "DifferentName")
        .expect("Query failed")
        .expect("Should find by hash");
    assert_eq!(found_by_hash.series_id, Some(15));
    assert_eq!(found_by_hash.status, "fetched");

    // 3. Lookup by scene name
    let found_by_name = db.find_arr_grab_by_hash_or_name("non_existent_hash", "The.Bear.S03E01.1080p.WEB.H264")
        .expect("Query failed")
        .expect("Should find by name");
    assert_eq!(found_by_name.id, grab_id);

    // 4. Update status to imported (Download event)
    db.mark_grab_status(grab_id, "imported", false).expect("Status update failed");
    let imported_grab = db.get_arr_grab_by_id(grab_id).expect("Query failed").expect("Grab not found");
    assert_eq!(imported_grab.status, "imported");
    assert!(!imported_grab.re_searched);

    // 5. Torrent marked trumped/unregistered -> MediaReplacer marks replaced
    db.mark_grab_status(grab_id, "replaced", true).expect("MediaReplacer update failed");
    let replaced_grab = db.get_arr_grab_by_id(grab_id).expect("Query failed").expect("Grab not found");
    assert_eq!(replaced_grab.status, "replaced");
    assert!(replaced_grab.re_searched);
    assert_eq!(replaced_grab.re_search_count, 1);

    // 6. Full lineage: arr_grabs itself only ever shows current state ("replaced"), but
    // arr_grab_history must have preserved every step along the way, oldest first — this is
    // what lets the Ghost Archive/Pipeline UI show "grabbed -> imported -> replaced" instead of
    // just the final status with no record of what happened before it.
    let history = db.list_grab_history(grab_id).expect("History query failed");
    assert_eq!(history.len(), 3, "expected one history row per save_arr_grab/mark_grab_status call");
    assert_eq!(history[0].status, "fetched");
    assert_eq!(history[0].event_type, "Grab");
    assert_eq!(history[1].status, "imported");
    assert_eq!(history[1].event_type, "StatusChange");
    assert_eq!(history[2].status, "replaced");
    assert_eq!(history[2].event_type, "ReSearch");
    // The re-search step didn't touch release_title in arr_grabs, but the history row should
    // still carry it forward from the snapshot taken at mark_grab_status time.
    assert_eq!(history[2].release_title.as_deref(), Some("The.Bear.S03E01.1080p.WEB.H264"));

    // 7. Search filtering for 'replaced' in Ghost Archive
    let replaced_results = db.search_arr_grabs(None, Some("replaced"), None, 50).expect("Search failed");
    assert_eq!(replaced_results.len(), 1, "expected replaced grab to be found under replaced filter");
    assert_eq!(replaced_results[0].id, grab_id);

    // 8. Re-download (upgrade) updates status to imported, but history and search retains replaced & deleted history
    db.mark_grab_status(grab_id, "deleted", false).expect("Delete status failed");
    let deleted_results = db.search_arr_grabs(None, Some("deleted"), None, 50).expect("Search failed");
    assert_eq!(deleted_results.len(), 1, "expected grab to be found under deleted filter");

    // Upgrade is now imported
    db.mark_grab_status(grab_id, "imported", false).expect("Import status failed");
    let post_import_replaced = db.search_arr_grabs(None, Some("replaced"), None, 50).expect("Search failed");
    assert_eq!(post_import_replaced.len(), 1, "historical replacement still surfaces in Ghost Archive filter");
    let post_import_deleted = db.search_arr_grabs(None, Some("deleted"), None, 50).expect("Search failed");
    assert_eq!(post_import_deleted.len(), 1, "historical deletion still surfaces in Ghost Archive filter");
}

#[test]
fn test_sonarr_import_reconciliation_from_queue_source_path() {
    let temp_db = NamedTempFile::new().expect("Failed to create temp db");
    let db = Database::init(temp_db.path(), Some("secret_db_pass")).expect("DB init failed");

    let grab_id = "f0e1d2c3b4a5968778695a4b3c2d1e0f";
    let now = Utc::now();

    // 1. Initial Grab from Sonarr
    let grab_record = ArrGrabRecord {
        id: grab_id.to_string(),
        scene_name: "Lanterns.2026.S01E03.1080p.WEB.h264-ETHEL".to_string(),
        release_title: "Lanterns.2026.S01E03.1080p.WEB.h264-ETHEL".to_string(),
        event_type: "Grab".to_string(),
        item_type: "series".to_string(),
        series_id: Some(101),
        movie_id: None,
        artist_id: None,
        album_id: None,
        zone_id: None,
        season_number: Some(1),
        episode_numbers: Some("[3]".to_string()),
        episode_ids: Some("[503]".to_string()),
        indexer: Some("TorrentLeech".to_string()),
        download_client: Some("qBittorrent-Main".to_string()),
        download_id: Some(grab_id.to_string()),
        status: "fetched".to_string(),
        re_searched: false,
        re_search_count: 0,
        title: Some("Lanterns".to_string()),
        year: Some(2026),
        overview: Some("DC Green Lantern mystery thriller.".to_string()),
        poster_url: Some("https://example.com/lanterns.jpg".to_string()),
        genres: Some("Action, Sci-Fi".to_string()),
        quality: Some("1080p WEB".to_string()),
        size_bytes: Some(2500000000),
        imdb_id: Some("tt9988776".to_string()),
        tmdb_id: None,
        tvdb_id: Some(403889),
        runtime_mins: Some(52),
        rating: Some(8.9),
        mattermost_post_id: None,
        payload_json: "{}".to_string(),
        created_at: now,
        updated_at: now,
    };

    db.save_arr_grab(&grab_record).expect("Failed to save grab");

    // 2. Test lookup by full sourcePath (as passed by Sonarr Download / Episode Imported webhook)
    let source_path = "/queue/tvQueue/Lanterns.2026.S01E03.1080p.WEB.h264-ETHEL.mkv";
    let found_by_source = db.find_arr_grab_by_hash_or_name("", source_path)
        .expect("Query failed")
        .expect("Should match grab by full sourcePath");
    assert_eq!(found_by_source.id, grab_id);
    assert_eq!(found_by_source.release_title, "Lanterns.2026.S01E03.1080p.WEB.h264-ETHEL");

    // 3. Test lookup by series_id and episode_id / episode_number
    let found_by_ep = db.find_arr_grab_by_series_episode(Some(101), Some(1), Some(3), Some(503))
        .expect("Query failed")
        .expect("Should match grab by series and episode IDs");
    assert_eq!(found_by_ep.id, grab_id);

    // 4. Simulate Sonarr Download webhook saving reconciled record
    let imported_record = ArrGrabRecord {
        id: found_by_source.id, // Reconciled canonical ID
        scene_name: found_by_source.scene_name, // Preserved scene name
        release_title: found_by_source.release_title, // Preserved release title
        event_type: "Download".to_string(),
        item_type: "series".to_string(),
        series_id: Some(101),
        movie_id: None,
        artist_id: None,
        album_id: None,
        zone_id: None,
        season_number: Some(1),
        episode_numbers: Some("[3]".to_string()),
        episode_ids: Some("[503]".to_string()),
        indexer: found_by_source.indexer,
        download_client: found_by_source.download_client,
        download_id: found_by_source.download_id,
        status: "imported".to_string(),
        re_searched: false,
        re_search_count: 0,
        title: Some("Lanterns".to_string()),
        year: Some(2026),
        overview: Some("DC Green Lantern mystery thriller.".to_string()),
        poster_url: Some("https://example.com/lanterns.jpg".to_string()),
        genres: Some("Action, Sci-Fi".to_string()),
        quality: Some("1080p WEB".to_string()),
        size_bytes: Some(2500000000),
        imdb_id: Some("tt9988776".to_string()),
        tmdb_id: None,
        tvdb_id: Some(403889),
        runtime_mins: Some(52),
        rating: Some(8.9),
        mattermost_post_id: None,
        payload_json: "{}".to_string(),
        created_at: found_by_source.created_at,
        updated_at: Utc::now(),
    };

    db.save_arr_grab(&imported_record).expect("Failed to save imported grab");

    // 5. Verify current state in database
    let all_grabs = db.list_arr_grabs(10).expect("Failed to list grabs");
    assert_eq!(all_grabs.len(), 1, "Must not create duplicate row on import");
    assert_eq!(all_grabs[0].status, "imported");
    assert_eq!(all_grabs[0].release_title, "Lanterns.2026.S01E03.1080p.WEB.h264-ETHEL");

    // 6. Verify history timeline contains both Grab and Download events
    let history = db.list_grab_history(grab_id).expect("Failed to list grab history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].event_type, "Grab");
    assert_eq!(history[0].status, "fetched");
    assert_eq!(history[1].event_type, "Download");
    assert_eq!(history[1].status, "imported");
}
