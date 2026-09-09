// src/engines/arr_sync.rs
use crate::config::{ArrNodeConfig, ConfigManager};
use crate::db::Database;
use crate::engines::registry::{heartbeat_sleep, TaskHandle};
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;
use tracing::{error, info, warn};

pub async fn run_arr_sync_loop(
    config_mgr: ConfigManager,
    _db: Database,
    handle: TaskHandle,
) {
    info!("Starting Sonarr & Radarr Multi-Node Sync Engine");

    loop {
        let config = config_mgr.get().await;

        // 1. Sonarr Primary -> Replicas Sync
        if config.sonarr.enabled {
            sync_sonarr_instances(&config).await;
        }

        // 2. Radarr Primary -> Replicas Sync
        if config.radarr.enabled {
            sync_radarr_instances(&config).await;
        }

        // 3. Lidarr Primary -> Replicas Sync
        if config.lidarr.enabled {
            sync_lidarr_instances(&config).await;
        }

        let interval_mins = config
            .sonarr
            .sync_interval_mins
            .min(config.radarr.sync_interval_mins)
            .min(config.lidarr.sync_interval_mins)
            .max(5);
        heartbeat_sleep(&handle, Duration::from_secs(interval_mins * 60)).await;
    }
}

fn build_sync_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default()
}

pub async fn sync_sonarr_instances(config: &crate::config::AppConfig) {
    if let Some(ref primary) = config.sonarr.primary {
        // One client shared across every replica in this cycle, instead of a fresh
        // (connection-pool-less) client per replica.
        let http = build_sync_client();
        for replica in &config.sonarr.replicas {
            if let Err(e) = sync_sonarr_master_to_slave(&http, primary, replica).await {
                warn!("Failed to sync Sonarr from {} to {}: {}", primary.name, replica.name, e);
            }
        }
    }
}

pub async fn sync_radarr_instances(config: &crate::config::AppConfig) {
    if let Some(ref primary) = config.radarr.primary {
        let http = build_sync_client();
        for replica in &config.radarr.replicas {
            if let Err(e) = sync_radarr_master_to_slave(&http, primary, replica).await {
                warn!("Failed to sync Radarr from {} to {}: {}", primary.name, replica.name, e);
            }
        }
    }
}

pub async fn sync_lidarr_instances(config: &crate::config::AppConfig) {
    if let Some(ref primary) = config.lidarr.primary {
        let http = build_sync_client();
        for replica in &config.lidarr.replicas {
            if let Err(e) = sync_lidarr_master_to_slave(&http, primary, replica).await {
                warn!("Failed to sync Lidarr from {} to {}: {}", primary.name, replica.name, e);
            }
        }
    }
}

async fn sync_sonarr_master_to_slave(http: &reqwest::Client, master: &ArrNodeConfig, slave: &ArrNodeConfig) -> anyhow::Result<()> {
    let master_url = format!("{}/api/v{}/series?apikey={}", master.base_url.trim_end_matches('/'), master.version, master.api_key);
    let slave_url = format!("{}/api/v{}/series?apikey={}", slave.base_url.trim_end_matches('/'), slave.version, slave.api_key);

    let master_series: Vec<Value> = http.get(&master_url).send().await?.json().await?;
    let slave_series: Vec<Value> = http.get(&slave_url).send().await?.json().await?;

    let slave_tvdb_ids: HashSet<i64> = slave_series
        .iter()
        .filter_map(|s| s["tvdbId"].as_i64())
        .collect();

    for mut series in master_series {
        if let Some(tvdb_id) = series["tvdbId"].as_i64() {
            if !slave_tvdb_ids.contains(&tvdb_id) {
                info!("Propagating TV Show '{}' (TVDB: {}) to slave {}", series["title"].as_str().unwrap_or_default(), tvdb_id, slave.name);

                if let Some(ref bp) = slave.base_path {
                    series["rootFolderPath"] = serde_json::json!(bp);
                }
                series["monitored"] = serde_json::json!(true);

                let post_res = http.post(&slave_url).json(&series).send().await;
                if let Err(err) = post_res {
                    error!("Error adding TV show to slave {}: {}", slave.name, err);
                }
            }
        }
    }

    Ok(())
}

async fn sync_radarr_master_to_slave(http: &reqwest::Client, master: &ArrNodeConfig, slave: &ArrNodeConfig) -> anyhow::Result<()> {
    let master_url = format!("{}/api/v{}/movie?apikey={}", master.base_url.trim_end_matches('/'), master.version, master.api_key);
    let slave_url = format!("{}/api/v{}/movie?apikey={}", slave.base_url.trim_end_matches('/'), slave.version, slave.api_key);

    let master_movies: Vec<Value> = http.get(&master_url).send().await?.json().await?;
    let slave_movies: Vec<Value> = http.get(&slave_url).send().await?.json().await?;

    let slave_tmdb_ids: HashSet<i64> = slave_movies
        .iter()
        .filter_map(|m| m["tmdbId"].as_i64())
        .collect();

    for mut movie in master_movies {
        if let Some(tmdb_id) = movie["tmdbId"].as_i64() {
            if !slave_tmdb_ids.contains(&tmdb_id) {
                info!("Propagating Movie '{}' (TMDB: {}) to slave {}", movie["title"].as_str().unwrap_or_default(), tmdb_id, slave.name);

                if let Some(ref bp) = slave.base_path {
                    movie["rootFolderPath"] = serde_json::json!(bp);
                }
                movie["monitored"] = serde_json::json!(true);

                let post_res = http.post(&slave_url).json(&movie).send().await;
                if let Err(err) = post_res {
                    error!("Error adding movie to slave {}: {}", slave.name, err);
                }
            }
        }
    }

    Ok(())
}

async fn sync_lidarr_master_to_slave(http: &reqwest::Client, master: &ArrNodeConfig, slave: &ArrNodeConfig) -> anyhow::Result<()> {
    let master_api_ver = if master.version == 3 { 1 } else { master.version };
    let slave_api_ver = if slave.version == 3 { 1 } else { slave.version };

    let master_url = format!("{}/api/v{}/artist?apikey={}", master.base_url.trim_end_matches('/'), master_api_ver, master.api_key);
    let slave_url = format!("{}/api/v{}/artist?apikey={}", slave.base_url.trim_end_matches('/'), slave_api_ver, slave.api_key);

    let master_artists: Vec<Value> = http.get(&master_url).send().await?.json().await?;
    let slave_artists: Vec<Value> = http.get(&slave_url).send().await?.json().await?;

    let slave_mbids: HashSet<String> = slave_artists
        .iter()
        .filter_map(|a| a["foreignArtistId"].as_str().or_else(|| a["mbId"].as_str()).map(|s| s.to_string()))
        .collect();

    for mut artist in master_artists {
        let maybe_mbid = artist["foreignArtistId"].as_str().or_else(|| artist["mbId"].as_str()).map(|s| s.to_string());
        if let Some(mbid) = maybe_mbid {
            if !slave_mbids.contains(&mbid) {
                info!("Propagating Artist '{}' (MBID: {}) to slave {}", artist["artistName"].as_str().unwrap_or_default(), mbid, slave.name);

                if let Some(ref bp) = slave.base_path {
                    artist["rootFolderPath"] = serde_json::json!(bp);
                }
                artist["monitored"] = serde_json::json!(true);

                let post_res = http.post(&slave_url).json(&artist).send().await;
                if let Err(err) = post_res {
                    error!("Error adding artist to slave {}: {}", slave.name, err);
                }
            }
        }
    }

    Ok(())
}
