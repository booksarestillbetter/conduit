// src/engines/influx_pusher.rs
use crate::config::ConfigManager;
use crate::engines::registry::{heartbeat_sleep, TaskHandle};
use crate::fetcher::FetcherPool;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub async fn run_influx_pusher_loop(
    config_mgr: ConfigManager,
    pool: Arc<FetcherPool>,
    handle: TaskHandle,
) {
    info!("Starting InfluxDB v2 Metrics Pusher Engine");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    loop {
        let config = config_mgr.get().await;
        if config.influx.enabled && !config.influx.token.is_empty() {
            let stats = pool.get_aggregate_stats();

            let scheme = if config.influx.use_ssl { "https" } else { "http" };
            let url = format!(
                "{}://{}:{}/api/v2/write?org={}&bucket={}&precision=s",
                scheme, config.influx.host, config.influx.port, config.influx.org, config.influx.bucket
            );

            let mut lines = Vec::new();
            for node_stat in stats.nodes {
                let escaped_host = node_stat
                    .node
                    .replace('\\', "\\\\")
                    .replace(' ', "\\ ")
                    .replace(',', "\\,")
                    .replace('=', "\\=");
                lines.push(format!(
                    "transmission_node,host={},downloader=transmission download_speed={},upload_speed={},torrents_total={},free_space_bytes={}",
                    escaped_host,
                    node_stat.rate_download,
                    node_stat.rate_upload,
                    node_stat.total_torrents,
                    node_stat.free_space_bytes
                ));
            }

            if !lines.is_empty() {
                let payload = lines.join("\n");
                let res = client
                    .post(&url)
                    .header("Authorization", format!("Token {}", config.influx.token))
                    .body(payload)
                    .send()
                    .await;

                if let Err(e) = res {
                    warn!("Failed to push metrics to InfluxDB: {}", e);
                }
            }
        }

        heartbeat_sleep(&handle, Duration::from_secs(30)).await;
    }
}
