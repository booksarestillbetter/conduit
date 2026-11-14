// src/engines/hardlink_checker.rs
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, RwLock};
use tracing::{info, warn};
use utoipa::ToSchema;
use walkdir::WalkDir;

use crate::config::model::AppConfig;
use crate::config::ConfigManager;
use crate::db::Database;
use crate::engines::registry::{ensure_registered, EngineRegistry, TaskHandle};
use crate::notify::NotificationManager;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HardlinkIssue {
    pub pair_name: String,
    pub path: String,
    pub kind: String, // "cross_device_copy" | "unlinked_copy" | "broken_symlink" | "missing_dir"
    pub size_bytes: u64,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HardlinkPairStatus {
    pub name: String,
    pub source_dir: String,
    pub dest_dir: String,
    pub same_filesystem: bool,
    pub total_dest_files: u64,
    pub hardlinked_files: u64,
    pub duplicate_copy_files: u64,
    pub broken_symlinks: u64,
    pub wasted_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct HardlinkHealthReport {
    pub scanned_at: Option<DateTime<Utc>>,
    pub total_pairs: usize,
    pub total_dest_files: u64,
    pub hardlinked_files: u64,
    pub duplicate_copy_files: u64,
    pub broken_symlinks: u64,
    pub wasted_bytes: u64,
    pub pairs: Vec<HardlinkPairStatus>,
    pub issues: Vec<HardlinkIssue>,
}

#[derive(Clone, Default)]
pub struct HardlinkCache {
    inner: Arc<RwLock<Option<HardlinkHealthReport>>>,
}

impl HardlinkCache {
    pub async fn get(&self) -> Option<HardlinkHealthReport> {
        self.inner.read().await.clone()
    }

    pub async fn set(&self, report: HardlinkHealthReport) {
        *self.inner.write().await = Some(report);
    }
}

#[cfg(unix)]
fn dev_ino_nlink(meta: &std::fs::Metadata) -> (u64, u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (meta.dev(), meta.ino(), meta.nlink())
}

#[cfg(not(unix))]
fn dev_ino_nlink(_meta: &std::fs::Metadata) -> (u64, u64, u64) {
    (0, 0, 1)
}

const MAX_FILES_PER_DIR: usize = 50_000;
const MAX_ISSUES_IN_REPORT: usize = 100;

fn scan_pair(
    name: &str,
    source_dir: &str,
    dest_dir: &str,
    issues: &mut Vec<HardlinkIssue>,
) -> HardlinkPairStatus {
    let src_path = Path::new(source_dir);
    let dst_path = Path::new(dest_dir);

    let mut status = HardlinkPairStatus {
        name: name.to_string(),
        source_dir: source_dir.to_string(),
        dest_dir: dest_dir.to_string(),
        same_filesystem: true,
        total_dest_files: 0,
        hardlinked_files: 0,
        duplicate_copy_files: 0,
        broken_symlinks: 0,
        wasted_bytes: 0,
    };

    let Ok(src_root_meta) = std::fs::metadata(src_path) else {
        if issues.len() < MAX_ISSUES_IN_REPORT {
            issues.push(HardlinkIssue {
                pair_name: name.to_string(),
                path: source_dir.to_string(),
                kind: "missing_dir".to_string(),
                size_bytes: 0,
                detail: "Source directory does not exist or is not accessible".to_string(),
            });
        }
        return status;
    };

    let Ok(dst_root_meta) = std::fs::metadata(dst_path) else {
        if issues.len() < MAX_ISSUES_IN_REPORT {
            issues.push(HardlinkIssue {
                pair_name: name.to_string(),
                path: dest_dir.to_string(),
                kind: "missing_dir".to_string(),
                size_bytes: 0,
                detail: "Destination directory does not exist or is not accessible".to_string(),
            });
        }
        return status;
    };

    let (src_dev, _, _) = dev_ino_nlink(&src_root_meta);
    let (dst_dev, _, _) = dev_ino_nlink(&dst_root_meta);
    status.same_filesystem = src_dev == dst_dev;

    // Index source files by (dev, ino) and by (filename, size) so renamed hardlinks
    // match via inode, while unlinked/cross-device copies match via (filename, size).
    let mut src_inodes: HashSet<(u64, u64)> = HashSet::new();
    let mut src_names_sizes: HashMap<(String, u64), usize> = HashMap::new();

    for (idx, entry) in WalkDir::new(src_path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .enumerate()
    {
        if idx >= MAX_FILES_PER_DIR {
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            let (dev, ino, _) = dev_ino_nlink(&meta);
            src_inodes.insert((dev, ino));
            let fname = entry.file_name().to_string_lossy().to_string();
            *src_names_sizes.entry((fname, meta.len())).or_insert(0) += 1;
        }
    }

    for (idx, entry) in WalkDir::new(dst_path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .enumerate()
    {
        if idx >= MAX_FILES_PER_DIR {
            break;
        }
        if entry.file_type().is_symlink() {
            if std::fs::metadata(entry.path()).is_err() {
                status.broken_symlinks += 1;
                if issues.len() < MAX_ISSUES_IN_REPORT {
                    issues.push(HardlinkIssue {
                        pair_name: name.to_string(),
                        path: entry.path().display().to_string(),
                        kind: "broken_symlink".to_string(),
                        size_bytes: 0,
                        detail: "Dangling symlink in destination/staging directory".to_string(),
                    });
                }
            }
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        status.total_dest_files += 1;
        let (dev, ino, nlink) = dev_ino_nlink(&meta);

        if src_inodes.contains(&(dev, ino)) {
            status.hardlinked_files += 1;
            continue;
        }

        let fname = entry.file_name().to_string_lossy().to_string();
        let size = meta.len();
        if nlink <= 1 && src_names_sizes.contains_key(&(fname, size)) {
            status.duplicate_copy_files += 1;
            status.wasted_bytes += size;
            if issues.len() < MAX_ISSUES_IN_REPORT {
                let kind = if !status.same_filesystem {
                    "cross_device_copy"
                } else {
                    "unlinked_copy"
                };
                let detail = if !status.same_filesystem {
                    "Source and destination are on different filesystems; file was copied instead of hardlinked".to_string()
                } else {
                    "File has nlink=1 and duplicates a file in source directory without sharing an inode".to_string()
                };
                issues.push(HardlinkIssue {
                    pair_name: name.to_string(),
                    path: entry.path().display().to_string(),
                    kind: kind.to_string(),
                    size_bytes: size,
                    detail,
                });
            }
        }
    }

    status
}

pub async fn scan_hardlink_health(config: &AppConfig) -> HardlinkHealthReport {
    let mut pairs_to_scan: Vec<(String, String, String)> = Vec::new();
    for m in &config.file_sync.mappings {
        if !m.watch_dir.is_empty() && !m.post_dir.is_empty() {
            pairs_to_scan.push((m.name.clone(), m.watch_dir.clone(), m.post_dir.clone()));
        }
    }
    for p in &config.hardlink_checker.extra_pairs {
        if !p.source_dir.is_empty() && !p.dest_dir.is_empty() {
            pairs_to_scan.push((p.name.clone(), p.source_dir.clone(), p.dest_dir.clone()));
        }
    }

    tokio::task::spawn_blocking(move || {
        let mut report = HardlinkHealthReport {
            scanned_at: Some(Utc::now()),
            total_pairs: pairs_to_scan.len(),
            ..Default::default()
        };

        for (name, src, dst) in pairs_to_scan {
            let pair_status = scan_pair(&name, &src, &dst, &mut report.issues);
            report.total_dest_files += pair_status.total_dest_files;
            report.hardlinked_files += pair_status.hardlinked_files;
            report.duplicate_copy_files += pair_status.duplicate_copy_files;
            report.broken_symlinks += pair_status.broken_symlinks;
            report.wasted_bytes += pair_status.wasted_bytes;
            report.pairs.push(pair_status);
        }

        report
    })
    .await
    .unwrap_or_default()
}

pub async fn run_hardlink_checker_loop(
    config_mgr: ConfigManager,
    db: Database,
    _notifiers: Arc<NotificationManager>,
    cache: HardlinkCache,
    registry: EngineRegistry,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut last_alert_wasted_mb: u64 = 0;
    ensure_registered(&registry, "hardlink_checker");
    let handle = TaskHandle::new(registry, "hardlink_checker");

    loop {
        let cfg = config_mgr.get().await;
        if cfg.hardlink_checker.enabled {
            let report = scan_hardlink_health(&cfg).await;
            let wasted_mb = report.wasted_bytes / (1024 * 1024);
            let dup_count = report.duplicate_copy_files;
            cache.set(report).await;

            handle.tick();

            let threshold_mb = cfg.hardlink_checker.alert_wasted_mb_threshold;
            if wasted_mb >= threshold_mb && threshold_mb > 0 && wasted_mb > last_alert_wasted_mb {
                last_alert_wasted_mb = wasted_mb;
                let msg = format!(
                    "Hardlink health warning: {} unlinked duplicate file(s) wasting {} MB across staging/sync directories",
                    dup_count, wasted_mb
                );
                warn!("{}", msg);
                let _ = db.log_event("hardlink_checker", "warn", &msg, None);
                NotificationManager::dispatch(
                    &cfg.notifications,
                    "system",
                    None,
                    "Hardlink Health Warning",
                    &msg,
                    Some("#f59e0b"),
                )
                .await;
            } else if wasted_mb < threshold_mb {
                last_alert_wasted_mb = 0;
            }
        } else {
            info!("Hardlink checker disabled");
            handle.tick();
        }

        let interval = cfg.hardlink_checker.check_interval_secs.max(60);
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(interval)) => {}
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::HardlinkPairConfig;

    #[tokio::test]
    async fn test_scan_hardlink_health_detects_hardlinks_and_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("downloads");
        let dst = dir.path().join("library");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();

        let f1 = src.join("linked.mkv");
        std::fs::write(&f1, b"1234567890").unwrap();
        std::fs::hard_link(&f1, dst.join("linked_renamed.mkv")).unwrap();

        let f2 = src.join("copied.mkv");
        std::fs::write(&f2, b"abcdefghij").unwrap();
        std::fs::copy(&f2, dst.join("copied.mkv")).unwrap();

        let mut cfg = AppConfig::default();
        cfg.hardlink_checker.extra_pairs.push(HardlinkPairConfig {
            name: "test-pair".to_string(),
            source_dir: src.display().to_string(),
            dest_dir: dst.display().to_string(),
        });

        let report = scan_hardlink_health(&cfg).await;
        assert_eq!(report.total_pairs, 1);
        assert_eq!(report.total_dest_files, 2);
        assert_eq!(report.hardlinked_files, 1);
        assert_eq!(report.duplicate_copy_files, 1);
        assert_eq!(report.wasted_bytes, 10);
        assert_eq!(report.issues.len(), 1);
    }
}
