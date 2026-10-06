// Two torrents for the same movie (1080p + 4K) must stay two distinct grabs end to end.
use chrono::Utc;
use conduit::db::{ArrGrabRecord, Database};
use tempfile::NamedTempFile;

const HASH_1080: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_4K: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn grab(id: &str, release: &str, hash: &str, quality: &str) -> ArrGrabRecord {
    let now = Utc::now();
    ArrGrabRecord {
        id: id.to_string(),
        scene_name: release.to_string(),
        release_title: release.to_string(),
        event_type: "Grab".to_string(),
        item_type: "movie".to_string(),
        series_id: None,
        movie_id: Some(42),
        artist_id: None,
        album_id: None,
        zone_id: None,
        season_number: None,
        episode_numbers: None,
        episode_ids: None,
        indexer: None,
        download_client: Some("node-a".to_string()),
        download_id: Some(hash.to_string()),
        status: "fetched".to_string(),
        re_searched: false,
        re_search_count: 0,
        title: Some("Some Movie".to_string()),
        year: Some(2020),
        overview: None,
        poster_url: None,
        genres: None,
        quality: Some(quality.to_string()),
        size_bytes: None,
        imdb_id: None,
        tmdb_id: Some(7),
        tvdb_id: None,
        runtime_mins: None,
        rating: None,
        mattermost_post_id: None,
        payload_json: "{}".to_string(),
        created_at: now,
        updated_at: now,
    }
}

fn setup() -> (Database, NamedTempFile) {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::init(temp.path(), Some("secret_db_pass")).unwrap();
    db.save_arr_grab(&grab(HASH_1080, "Some.Movie.2020.1080p.BluRay", HASH_1080, "1080p")).unwrap();
    db.save_arr_grab(&grab(HASH_4K, "Some.Movie.2020.2160p.UHD", HASH_4K, "2160p")).unwrap();
    (db, temp)
}

#[test]
fn each_torrent_resolves_to_its_own_grab() {
    let (db, _t) = setup();
    let map = db.get_arr_grabs_lookup_map();
    let a = Database::match_grab_in_map(&map, HASH_1080, "Some.Movie.2020.1080p.BluRay").unwrap();
    let b = Database::match_grab_in_map(&map, HASH_4K, "Some.Movie.2020.2160p.UHD").unwrap();
    assert_eq!(a.quality.as_deref(), Some("1080p"));
    assert_eq!(b.quality.as_deref(), Some("2160p"));
}

#[test]
fn name_match_never_steals_a_grab_bound_to_another_hash() {
    let (db, _t) = setup();
    let map = db.get_arr_grabs_lookup_map();
    // A third torrent (unknown hash) whose name equals the 1080p release must not adopt that grab.
    let other = "cccccccccccccccccccccccccccccccccccccccc";
    assert!(Database::match_grab_in_map(&map, other, "Some.Movie.2020.1080p.BluRay").is_none());
    assert!(db.find_arr_grab_by_hash_or_name(other, "Some.Movie.2020.1080p.BluRay").unwrap().is_none());
    // ...but without a hash (webhook path-only lookups) name matching still works.
    assert!(db.find_arr_grab_by_hash_or_name("", "Some.Movie.2020.1080p.BluRay").unwrap().is_some());
}

#[test]
fn media_level_lookup_returns_every_concurrent_grab() {
    let (db, _t) = setup();
    let all = db.find_arr_grabs_for_media(Some(42), None, None, None, Some("Some Movie")).unwrap();
    assert_eq!(all.len(), 2);
    // With no ids, title must match exactly (no "Alien" vs "Aliens" style substring hits).
    assert!(db.find_arr_grabs_for_media(None, None, None, None, Some("Some Mov")).unwrap().is_empty());
    assert_eq!(db.find_arr_grabs_for_media(None, None, None, None, Some("some movie")).unwrap().len(), 2);
}
