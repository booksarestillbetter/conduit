// src/engines/ip_asn_updater.rs
use crate::config::ConfigManager;
use crate::engines::registry::{heartbeat_sleep, TaskHandle};
use crate::ip_asn::{self, IpAsnCache};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

const SOURCE_URL: &str = "https://iptoasn.com/data/ip2asn-combined.tsv.gz";
const REFRESH_INTERVAL: Duration = Duration::from_secs(7 * 24 * 3600);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 3600);
const CACHE_FILENAME: &str = "ip2asn-combined.tsv.gz";

/// Downloads and parses iptoasn.com's free IP-to-ASN database on a weekly cadence (checked every
/// 6h, only re-downloaded once the cached copy is >7 days old), keeping the parsed result in
/// `cache` for on-demand peer-list enrichment (see `ip_asn::lookup`, wired into
/// `torrent_routes::get_torrent`). A no-op loop (just idles, releasing any loaded database) while
/// `config.ip_asn.enabled` is false — this is opt-in given the ~50-60MB memory cost.
pub async fn run_ip_asn_updater_loop(config_mgr: ConfigManager, cache: IpAsnCache, handle: TaskHandle) {
    info!("Starting IP-to-ASN Enrichment Updater (iptoasn.com, weekly refresh)");

    loop {
        let config = config_mgr.get().await;
        if !config.ip_asn.enabled {
            if cache.read().is_some() {
                *cache.write() = None;
                info!("IP-to-ASN enrichment disabled — released in-memory database");
            }
            handle.tick();
            heartbeat_sleep(&handle, CHECK_INTERVAL).await;
            continue;
        }

        let cache_path = PathBuf::from(&config.system.data_dir).join(CACHE_FILENAME);
        let needs_download = match tokio::fs::metadata(&cache_path).await {
            Ok(meta) => meta
                .modified()
                .ok()
                .and_then(|m| m.elapsed().ok())
                .map(|age| age > REFRESH_INTERVAL)
                .unwrap_or(true),
            Err(_) => true,
        };
        let was_loaded = cache.read().is_some();

        if needs_download {
            match download(&cache_path).await {
                Ok(()) => info!("Downloaded fresh IP-to-ASN database from {}", SOURCE_URL),
                Err(e) => warn!(
                    "Failed to download IP-to-ASN database: {} (will retry next cycle, using any existing cached copy)",
                    e
                ),
            }
        }

        if needs_download || !was_loaded {
            match tokio::fs::read(&cache_path).await {
                Ok(bytes) => match ip_asn::parse_tsv_gz(&bytes) {
                    Ok(db) => {
                        info!("Loaded IP-to-ASN database: {} ranges", db.len());
                        *cache.write() = Some(db);
                        handle.tick();
                    }
                    Err(e) => {
                        warn!("Failed to parse cached IP-to-ASN database: {}", e);
                        handle.tick_err(e.to_string());
                    }
                },
                Err(e) => {
                    warn!("No IP-to-ASN database available yet: {}", e);
                    handle.tick_err(e.to_string());
                }
            }
        } else {
            handle.tick();
        }

        heartbeat_sleep(&handle, CHECK_INTERVAL).await;
    }
}

async fn download(dest: &PathBuf) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    let bytes = client
        .get(SOURCE_URL)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    tokio::fs::write(dest, &bytes).await?;
    Ok(())
}
