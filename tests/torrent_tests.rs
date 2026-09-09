// tests/torrent_tests.rs
use conduit::fetcher::{Torrent, TorrentStatus, UnifiedTorrent};

#[test]
fn test_unified_torrent_conversion() {
    let raw = Torrent {
        id: 104,
        name: "Ubuntu.24.04.iso".to_string(),
        hash_string: "abcdef1234567890".to_string(),
        status: 4, // Downloading
        rate_upload: 150000,
        rate_download: 5200000,
        uploaded_ever: 10000000,
        downloaded_ever: 50000000,
        upload_ratio: 0.2,
        total_size: 4500000000,
        size_when_done: 4500000000,
        left_until_done: 2000000000,
        percent_done: 0.555,
        eta: 360,
        eta_idle: None,
        error: 0,
        error_string: String::new(),
        peers_connected: 15,
        peers_sending_to_us: 10,
        peers_getting_from_us: 4,
        added_date: 1700000000,
        done_date: 0,
        download_dir: "/downloads/iso".to_string(),
        tracker_stats: vec![],
        files: None,
        peers: None,
        comment: None,
        creator: None,
        date_created: None,
        piece_count: None,
        piece_size: None,
        is_private: None,
        magnet_link: None,
        corrupt_ever: None,
        seconds_downloading: None,
        seconds_seeding: None,
        activity_date: None,
        queue_position: 1,
        sequential_download: true,
        pieces: None,
        availability: None,
    };

    let unified = UnifiedTorrent::from_torrent("idyll", raw);
    assert_eq!(unified.compound_id, "idyll:104");
    assert_eq!(unified.node, "idyll");
    assert_eq!(unified.id, 104);
    assert_eq!(unified.status, TorrentStatus::Downloading);
    assert_eq!(unified.percent_done, 0.555);
    assert_eq!(unified.queue_position, 1);
    assert!(unified.sequential_download);
}
