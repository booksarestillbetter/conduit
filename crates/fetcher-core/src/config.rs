use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum RetrieverClientType {
    #[default]
    #[serde(alias = "fetcher", alias = "tr")]
    Transmission,
    #[serde(alias = "qbit", alias = "qbittorrent", alias = "qtorrent")]
    QBittorrent,
    #[serde(alias = "deluge")]
    Deluge,
    #[serde(alias = "synapse", alias = "rf_engine", alias = "synapsed")]
    Synapse,
}

impl std::fmt::Display for RetrieverClientType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transmission => write!(f, "transmission"),
            Self::QBittorrent => write!(f, "qbittorrent"),
            Self::Deluge => write!(f, "deluge"),
            Self::Synapse => write!(f, "synapse"),
        }
    }
}

fn default_true() -> bool { true }
fn default_tr_port() -> u16 { 9091 }
fn default_tr_rpc_path() -> String { "/transmission/rpc".to_string() }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct FetcherNodeConfig {
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub client_type: RetrieverClientType,
    #[serde(default = "default_tr_port")]
    pub port: u16,
    #[serde(default = "default_tr_rpc_path")]
    pub rpc_path: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub use_ssl: bool,
    #[serde(default = "default_true")]
    pub verify_tls: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub fetcher_only: bool,
    #[serde(default)]
    pub tv_pre: Option<String>,
    #[serde(default)]
    pub movie_pre: Option<String>,
    #[serde(default)]
    pub music_pre: Option<String>,
    #[serde(default)]
    pub auto_purge_min_space_gb: Option<u64>,
    #[serde(default)]
    pub auto_purge_ratio: Option<f64>,
    #[serde(default)]
    pub auto_purge_age_days: Option<u64>,
    #[serde(default)]
    pub auto_purge_seeds: Option<u64>,
    #[serde(default)]
    pub auto_purge_match_count: Option<u32>,
    #[serde(default = "default_true")]
    pub auto_purge_enabled: bool,
    #[serde(default)]
    pub media_dir_overrides: HashMap<String, String>,
}

impl FetcherNodeConfig {
    pub fn default_port_for_type(client_type: RetrieverClientType) -> u16 {
        match client_type {
            RetrieverClientType::Transmission => 9091,
            RetrieverClientType::QBittorrent => 8080,
            RetrieverClientType::Deluge => 8112,
            RetrieverClientType::Synapse => 50051,
        }
    }

    pub fn default_rpc_path_for_type(client_type: RetrieverClientType) -> &'static str {
        match client_type {
            RetrieverClientType::Transmission => "/transmission/rpc",
            RetrieverClientType::QBittorrent => "/api/v2",
            RetrieverClientType::Deluge => "/json",
            RetrieverClientType::Synapse => "",
        }
    }
}
