// tests/qbittorrent_tests.rs
use conduit::config::{FetcherNodeConfig, RetrieverClientType, ZoneConfig};
use conduit::downloader::{create_client, TorrentClientTrait};
use std::collections::HashMap;

#[test]
fn test_retriever_client_type_serde_aliases() {
    let json_qbit = "\"qbittorrent\"";
    let ct_qbit: RetrieverClientType = serde_json::from_str(json_qbit).unwrap();
    assert_eq!(ct_qbit, RetrieverClientType::QBittorrent);

    let json_qtorrent = "\"qtorrent\"";
    let ct_qtorrent: RetrieverClientType = serde_json::from_str(json_qtorrent).unwrap();
    assert_eq!(ct_qtorrent, RetrieverClientType::QBittorrent);

    let json_tr = "\"transmission\"";
    let ct_tr: RetrieverClientType = serde_json::from_str(json_tr).unwrap();
    assert_eq!(ct_tr, RetrieverClientType::Transmission);

    let json_deluge = "\"deluge\"";
    let ct_deluge: RetrieverClientType = serde_json::from_str(json_deluge).unwrap();
    assert_eq!(ct_deluge, RetrieverClientType::Deluge);
}

#[test]
fn test_fetcher_node_config_defaults() {
    assert_eq!(FetcherNodeConfig::default_port_for_type(RetrieverClientType::Transmission), 9091);
    assert_eq!(FetcherNodeConfig::default_port_for_type(RetrieverClientType::QBittorrent), 8080);
    assert_eq!(FetcherNodeConfig::default_port_for_type(RetrieverClientType::Deluge), 8112);

    assert_eq!(FetcherNodeConfig::default_rpc_path_for_type(RetrieverClientType::Transmission), "/transmission/rpc");
    assert_eq!(FetcherNodeConfig::default_rpc_path_for_type(RetrieverClientType::QBittorrent), "/api/v2");
    assert_eq!(FetcherNodeConfig::default_rpc_path_for_type(RetrieverClientType::Deluge), "/json");
}

#[test]
fn test_zone_config_fetcher_node_names_alias() {
    let json = r#"{
        "id": "general",
        "name": "General Library",
        "fetcher_node_names": ["qbit-main", "tr-backup"]
    }"#;
    let zone: ZoneConfig = serde_json::from_str(json).unwrap();
    assert_eq!(zone.id, "general");
    assert_eq!(zone.fetcher_node_names, vec!["qbit-main", "tr-backup"]);

    // Old configs saved before the fetcher_node_names rename used this key — must still parse.
    let legacy_json = r#"{
        "id": "legacy",
        "name": "Legacy Zone",
        "transmission_node_names": ["seedbox"]
    }"#;
    let legacy_zone: ZoneConfig = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(legacy_zone.fetcher_node_names, vec!["seedbox"]);
}

#[test]
fn test_create_qbittorrent_client() {
    let config = FetcherNodeConfig {
        name: "qbit-seedbox".to_string(),
        host: "127.0.0.1".to_string(),
        client_type: RetrieverClientType::QBittorrent,
        port: 8080,
        rpc_path: "/api/v2".to_string(),
        username: Some("admin".to_string()),
        password: Some("adminadmin".to_string()),
        use_ssl: false,
        verify_tls: true,
        enabled: true,
        fetcher_only: false,
        tv_pre: None,
        movie_pre: None,
        music_pre: None,
        auto_purge_min_space_gb: None,
        auto_purge_ratio: None,
        auto_purge_age_days: None,
        auto_purge_seeds: None,
        auto_purge_match_count: None,
        auto_purge_enabled: true,
        media_dir_overrides: HashMap::new(),
    };

    let client: std::sync::Arc<dyn TorrentClientTrait> = create_client(config);
    assert_eq!(client.node_name(), "qbit-seedbox");
    assert_eq!(client.client_type(), RetrieverClientType::QBittorrent);
}
