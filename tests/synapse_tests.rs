// tests/synapse_tests.rs
use conduit::config::{FetcherNodeConfig, RetrieverClientType};
use conduit::downloader::create_client;
use std::collections::HashMap;

#[test]
fn test_synapse_client_type_serde_aliases() {
    let json_synapse = "\"synapse\"";
    let ct_synapse: RetrieverClientType = serde_json::from_str(json_synapse).unwrap();
    assert_eq!(ct_synapse, RetrieverClientType::Synapse);

    let json_rf_engine = "\"rf_engine\"";
    let ct_rf: RetrieverClientType = serde_json::from_str(json_rf_engine).unwrap();
    assert_eq!(ct_rf, RetrieverClientType::Synapse);

    let json_synapsed = "\"synapsed\"";
    let ct_synapsed: RetrieverClientType = serde_json::from_str(json_synapsed).unwrap();
    assert_eq!(ct_synapsed, RetrieverClientType::Synapse);
}

#[test]
fn test_synapse_node_config_defaults() {
    assert_eq!(FetcherNodeConfig::default_port_for_type(RetrieverClientType::Synapse), 50051);
    assert_eq!(FetcherNodeConfig::default_rpc_path_for_type(RetrieverClientType::Synapse), "");
}

#[tokio::test]
async fn test_create_synapse_client() {
    let config = FetcherNodeConfig {
        name: "synapse-fast-node".to_string(),
        host: "127.0.0.1".to_string(),
        client_type: RetrieverClientType::Synapse,
        port: 50051,
        rpc_path: "".to_string(),
        username: None,
        password: Some("secret-bearer-token".to_string()),
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

    let client = create_client(config);
    assert_eq!(client.node_name(), "synapse-fast-node");
    assert_eq!(client.client_type(), RetrieverClientType::Synapse);

    // Initial snapshot should report disconnected until stream lands
    let res = client.get_torrents(None).await;
    assert!(res.is_err(), "Expected disconnected error before stream connects");
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("not connected"), "Error should indicate not connected: {}", err_str);
}
