// src/engines/file_sync.rs
use crate::config::ConfigManager;
use crate::db::Database;
use crate::engines::registry::{heartbeat_sleep, TaskHandle};
use crate::notify::NotificationManager;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, error, info, warn};
use walkdir::WalkDir;

/// A same-directory, hidden staging name for `target`. Copies/moves land here first and are
/// only renamed into `target` once complete — `rename` within the same directory is atomic, so
/// a killed process or interrupted copy can never leave a partial file at `target` for the
/// `target.exists()` "already synced" check to be fooled by.
fn staging_path_for(target: &Path) -> PathBuf {
    let file_name = target.file_name().and_then(|f| f.to_str()).unwrap_or("file");
    target.with_file_name(format!(".conduit-sync-{}", file_name))
}

/// Renames a completed staging copy into place. On failure the staging file is deliberately
/// left where it is rather than deleted — for the `delete_source_after_move` (`mv`) path the
/// original source is already gone at this point, so removing the staging copy too would lose
/// the file outright; leaving it lets an operator recover it from the hidden `.conduit-sync-*` name.
async fn finalize_staged_copy(staging: &Path, target: &Path) -> std::io::Result<()> {
    tokio::fs::rename(staging, target).await
}

pub async fn run_file_sync_loop(
    config_mgr: ConfigManager,
    db: Database,
    handle: TaskHandle,
) {
    info!("Starting Remote Sync & Folder Ingestion Watcher Engine");

    loop {
        let config = config_mgr.get().await;
        if config.file_sync.enabled {
            let mut total_synced = 0usize;

            // 1. Process custom Remote Sync / Resilio Ingestion Folder Mappings
            for mapping in &config.file_sync.mappings {
                if mapping.watch_dir.trim().is_empty() || mapping.post_dir.trim().is_empty() {
                    continue;
                }

                if let Ok(count) = sync_mapping_directory(
                    mapping,
                    &config.file_sync.sync_cmd,
                    &db,
                ).await {
                    total_synced += count;
                }
            }

            // 2. Backward compatibility: Process per-node legacy pre/post definitions if any
            for (node_name, node_cfg) in &config.nodes {
                if !node_cfg.enabled || node_cfg.fetcher_only {
                    continue;
                }

                if let (Some(pre), Some(post)) = (&node_cfg.tv_pre, &config.file_sync.tv_post) {
                    if let Ok(count) = sync_directory_files(pre, post, &config.file_sync.sync_cmd, "TV", node_name, &db).await {
                        total_synced += count;
                    }
                }

                if let (Some(pre), Some(post)) = (&node_cfg.movie_pre, &config.file_sync.movie_post) {
                    if let Ok(count) = sync_directory_files(pre, post, &config.file_sync.sync_cmd, "Movie", node_name, &db).await {
                        total_synced += count;
                    }
                }

                if let (Some(pre), Some(post)) = (&node_cfg.music_pre, &config.file_sync.music_post) {
                    if let Ok(count) = sync_directory_files(pre, post, &config.file_sync.sync_cmd, "Music", node_name, &db).await {
                        total_synced += count;
                    }
                }

                // 3. Queue Cleaner
                if let Some(days) = config.file_sync.clean_queue_days {
                    if days > 0 {
                        clean_staging_queues(node_cfg, days).await;
                    }
                }
            }

            // If files were synced, notify & trigger Plex library refresh
            if total_synced > 0 {
                info!("Completed remote sync ingestion batch: {} files transferred to Arr intake", total_synced);
                
                // Category filtering (and whether any target even wants "sync" events) is now
                // per-target inside dispatch itself — no legacy notify_on_sync pre-check needed.
                NotificationManager::dispatch(
                    &config.notifications,
                    "sync.completed",
                    None,
                    "Remote Sync Ingestion Complete",
                    &format!("Successfully ingested {} remote media files into *arr intake queues.", total_synced),
                    Some("#10B981"), // Green
                ).await;

                if config.plex.enabled && config.plex.refresh_on_sync {
                    for plex_node in &config.plex.nodes {
                        let token = plex_node.token_override.as_deref().unwrap_or(&config.plex.token);
                        let _ = refresh_plex_sections(&plex_node.url, token).await;
                    }
                }
            }
        }

        let interval = config.file_sync.interval_secs.max(10);
        heartbeat_sleep(&handle, Duration::from_secs(interval)).await;
    }
}

async fn sync_mapping_directory(
    mapping: &crate::config::RemoteSyncFolderMapping,
    default_cmd: &str,
    db: &Database,
) -> anyhow::Result<usize> {
    let watch_path = Path::new(&mapping.watch_dir);
    let post_path = Path::new(&mapping.post_dir);

    if !watch_path.exists() || !post_path.exists() {
        return Ok(0);
    }

    let mut count = 0usize;
    let now = std::time::SystemTime::now();

    for entry in WalkDir::new(watch_path)
        .min_depth(1)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let file_path = entry.path();
        let file_name = file_path.file_name().and_then(|f| f.to_str()).unwrap_or_default();

        // Skip temporary Resilio / Syncthing files, partial downloads, and dotfiles
        if file_name.starts_with('.')
            || file_name.ends_with(".bts")
            || file_name.ends_with(".sync")
            || file_name.ends_with(".part")
            || file_name.ends_with(".tmp")
            || file_name.starts_with("!sync")
            || file_path.to_string_lossy().contains(".sync")
        {
            continue;
        }

        // Check settle time (ensure remote sync finished writing)
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = now.duration_since(modified) {
                    if elapsed.as_secs() < mapping.settle_time_secs {
                        debug!("File '{}' still settling ({}s < {}s), skipping this cycle", file_name, elapsed.as_secs(), mapping.settle_time_secs);
                        continue;
                    }
                }
            }
        }

        // Relative path preservation
        if let Ok(rel_path) = file_path.strip_prefix(watch_path) {
            let target_dest = post_path.join(rel_path);
            if target_dest.exists() {
                continue; // Already exists at destination
            }

            if let Some(target_parent) = target_dest.parent() {
                let _ = tokio::fs::create_dir_all(target_parent).await;
            }

            info!("Ingesting remote sync file [{}] {} -> {}", mapping.name, file_path.display(), target_dest.display());

            let staging_dest = staging_path_for(&target_dest);
            let status = if mapping.delete_source_after_move {
                Command::new("mv")
                    .arg(file_path)
                    .arg(&staging_dest)
                    .status()
                    .await
            } else {
                let cmd_parts: Vec<&str> = default_cmd.split_whitespace().collect();
                if cmd_parts.is_empty() {
                    Command::new("cp")
                        .args(["-al", "-v"])
                        .arg(file_path)
                        .arg(&staging_dest)
                        .status()
                        .await
                } else {
                    Command::new(cmd_parts[0])
                        .args(&cmd_parts[1..])
                        .arg(file_path)
                        .arg(&staging_dest)
                        .status()
                        .await
                }
            };

            match status {
                Ok(s) if s.success() => {
                    if let Err(e) = finalize_staged_copy(&staging_dest, &target_dest).await {
                        error!("Ingested '{}' but failed to finalize into place at {}: {}", file_name, target_dest.display(), e);
                        continue;
                    }
                    count += 1;
                    let _ = db.log_event(
                        "remote_sync",
                        "info",
                        &format!("Ingested remote sync file '{}' -> '{}' for {}", file_name, target_dest.display(), mapping.name),
                        None,
                    );
                }
                Ok(s) => {
                    let _ = tokio::fs::remove_file(&staging_dest).await;
                    warn!("Ingestion command exited with non-zero code {}: {}", s, file_path.display());
                }
                Err(e) => {
                    let _ = tokio::fs::remove_file(&staging_dest).await;
                    error!("Failed to execute ingestion command for {}: {}", file_path.display(), e);
                }
            }
        }
    }

    if mapping.delete_source_after_move {
        cleanup_empty_dirs(watch_path).await;
    }

    Ok(count)
}

async fn cleanup_empty_dirs(root: &Path) {
    let dirs: Vec<PathBuf> = WalkDir::new(root)
        .min_depth(1)
        .max_depth(5)
        .contents_first(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .map(|e| e.into_path())
        .collect();

    for dir in dirs {
        let _ = tokio::fs::remove_dir(&dir).await;
    }
}

async fn sync_directory_files(
    source_dir: &str,
    dest_dir: &str,
    sync_cmd: &str,
    media_type: &str,
    node: &str,
    db: &Database,
) -> anyhow::Result<usize> {
    let source_path = Path::new(source_dir);
    let dest_path = Path::new(dest_dir);

    if !source_path.exists() || !dest_path.exists() {
        return Ok(0);
    }

    let mut count = 0usize;

    for entry in WalkDir::new(source_path)
        .min_depth(1)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let file_path = entry.path();
        let file_name = file_path.file_name().and_then(|f| f.to_str()).unwrap_or_default();

        // Skip partial downloads, torrent sync temp files, and hidden files
        if file_name.starts_with('.') || file_name.ends_with(".bts") || file_name.ends_with(".part") || file_path.to_string_lossy().contains(".sync") {
            continue;
        }

        // Relative path preservation
        if let Ok(rel_path) = file_path.strip_prefix(source_path) {
            let target_dest = dest_path.join(rel_path);
            if target_dest.exists() {
                continue; // Already exists at destination
            }

            if let Some(target_parent) = target_dest.parent() {
                let _ = tokio::fs::create_dir_all(target_parent).await;
            }

            debug!("Syncing [{}] {} -> {}", media_type, file_path.display(), target_dest.display());

            // Run sync command directly without invoking a shell, staging into a hidden
            // same-directory name so a killed transfer never leaves a partial file at
            // target_dest for the exists()-check above to mistake for "already synced".
            let staging_dest = staging_path_for(&target_dest);
            let cmd_parts: Vec<&str> = sync_cmd.split_whitespace().collect();
            let status = if cmd_parts.is_empty() {
                Command::new("cp")
                    .args(["-al", "-v"])
                    .arg(file_path)
                    .arg(&staging_dest)
                    .status()
                    .await
            } else {
                Command::new(cmd_parts[0])
                    .args(&cmd_parts[1..])
                    .arg(file_path)
                    .arg(&staging_dest)
                    .status()
                    .await
            };

            match status {
                Ok(s) if s.success() => {
                    if let Err(e) = finalize_staged_copy(&staging_dest, &target_dest).await {
                        error!("Synced '{}' but failed to finalize into place at {}: {}", file_name, target_dest.display(), e);
                        continue;
                    }
                    count += 1;
                    let _ = db.log_event(
                        "file_sync",
                        "info",
                        &format!("Synced {} file '{}' from node {}", media_type, file_name, node),
                        None,
                    );
                }
                Ok(s) => {
                    let _ = tokio::fs::remove_file(&staging_dest).await;
                    warn!("Sync command exited with non-zero code {}: {}", s, file_path.display());
                }
                Err(e) => {
                    let _ = tokio::fs::remove_file(&staging_dest).await;
                    error!("Failed to execute sync command for {}: {}", file_path.display(), e);
                }
            }
        }
    }

    Ok(count)
}

async fn clean_staging_queues(node_cfg: &crate::config::FetcherNodeConfig, max_age_days: u64) {
    let pre_dirs = [&node_cfg.tv_pre, &node_cfg.movie_pre, &node_cfg.music_pre];
    for dir in pre_dirs.into_iter().flatten() {
        let trimmed = dir.trim_matches('/');
        // Prevent dangerous root-level deletes
        if trimmed.len() < 3 || dir == "/" || dir == "/root" || dir == "/etc" || dir == "/usr" || dir == "/bin" {
            continue;
        }
        let path = Path::new(dir);
        if path.exists() && path.is_dir() {
            let max_age_str = format!("+{}", max_age_days);
            let _ = Command::new("find")
                .arg(path)
                .args(["-mindepth", "1", "-mtime", &max_age_str, "-not", "-name", ".*", "-exec", "rm", "-rf", "{}", ";"])
                .status()
                .await;
        }
    }
}

pub async fn refresh_plex_sections(plex_url: &str, token: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    let url = format!("{}/library/sections/all/refresh?X-Plex-Token={}", plex_url.trim_end_matches('/'), token);
    let _ = client.get(&url).send().await?;
    info!("Triggered Plex library sections refresh at {}", plex_url);
    Ok(())
}
