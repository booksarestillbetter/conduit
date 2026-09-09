// src/notify/mod.rs
pub mod discord;
pub mod mattermost;
pub mod pushover;
pub mod webhook;

use crate::config::{NotificationChannel, NotificationConfig, NotificationTarget};
use std::future::Future;
use tracing::warn;

pub struct NotificationManager;

/// The fixed set of event categories a `NotificationTarget.categories` entry may name. An empty
/// `categories` list means "every category" (used by the legacy-config migration to preserve
/// pre-0.11 "every enabled channel gets every event" behavior exactly).
#[allow(dead_code)] // documented reference for the Settings UI / API consumers, not read internally
pub const CATEGORY_KEYS: &[&str] = &["grab", "download", "replacement", "health", "error", "autopurge", "sync", "scrobble"];

/// Maps a raw `event_type` string (e.g. "sonarr.grab", "health.alert") to one of `CATEGORY_KEYS`.
/// `torrent.error_fixed` — the pipeline's self-heal of an unregistered/trumped torrent (auto-
/// purge + re-search) — is deliberately classified as "replacement", not "error": it's routine
/// cleanup, the same story as `torrent.replaced`/`pipeline.researched`, not an actual problem.
/// `health.*` (from `engines::health_monitor`) previously fell through to an always-on catch-all
/// with no toggle at all — it's now its own real, filterable category.
fn categorize_event(event_type: &str) -> &'static str {
    match event_type {
        t if t.starts_with("arr.grab") || t.starts_with("sonarr.grab") || t.starts_with("radarr.grab") || t.starts_with("lidarr.grab")
            || t.ends_with(".added") || t.starts_with("ombi.") || t == "torrent.enriched" => "grab",
        t if t.starts_with("sync.download") || t.starts_with("download.staged") || t.contains("download") || t == "torrent.started" || t == "torrent.restarted" => "download",
        t if t.starts_with("torrent.replaced") || t.starts_with("pipeline.researched") || t == "torrent.error_fixed" || t.ends_with(".delete") || t == "torrent.deleted" || t == "torrent.removed" => "replacement",
        t if t.starts_with("health.") || t.ends_with(".health") || t.ends_with(".update") => "health",
        t if t.starts_with("torrent.error") || t.starts_with("error") || t.ends_with(".manual") || t.starts_with("plex.admin") => "error",
        t if t.starts_with("space.") => "autopurge",
        t if t.starts_with("arr.sync") || t.starts_with("plex.refresh") || t.starts_with("trakt.sync") || t.ends_with(".rename") || t == "torrent.location_changed" => "sync",
        t if t.starts_with("scrobble") || t.starts_with("plex.scrobble") || t.starts_with("media.scrobble") || t.starts_with("media.rate") => "scrobble",
        _ => "error",
    }
}

/// Whether `target` should receive an event of `category`, optionally tagged with `zone_id`
/// (the zone a Sonarr/Radarr/Lidarr webhook resolved, when known — see the 0.8.0 zones work).
/// A target with no `zone_id` is global and fires for every event, zoned or not; a zone-scoped
/// target only fires for that exact zone's events (never for zone-less ones, since those aren't
/// attributable to it).
pub fn target_applies(target: &NotificationTarget, category: &str, zone_id: Option<&str>) -> bool {
    if !target.enabled {
        return false;
    }
    if !target.categories.is_empty() && !target.categories.iter().any(|c| c == category) {
        return false;
    }
    match (target.zone_id.as_deref(), zone_id) {
        (None, _) => true,
        (Some(tz), Some(ez)) => tz == ez,
        (Some(_), None) => false,
    }
}

/// Notification failures used to vanish into `let _ = ...` with zero operator-visible signal.
/// This gives every channel one retry after a short delay (handles a transient blip without a
/// full retry queue) and always logs a warning on final failure instead of staying silent.
async fn send_with_retry<F, Fut, T>(channel: &str, attempt: F) -> Option<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    match attempt().await {
        Ok(v) => return Some(v),
        Err(e) => warn!("{} notification attempt 1 failed, retrying once: {}", channel, e),
    }

    tokio::time::sleep(std::time::Duration::from_millis(750)).await;

    match attempt().await {
        Ok(v) => Some(v),
        Err(e) => {
            warn!("{} notification failed after retry, giving up: {}", channel, e);
            None
        }
    }
}

impl NotificationManager {
    pub async fn dispatch(
        config: &NotificationConfig,
        event_type: &str,
        zone_id: Option<&str>,
        title: &str,
        message: &str,
        color_hex: Option<&str>,
    ) {
        let category = categorize_event(event_type);

        for target in config.targets.iter().filter(|t| target_applies(t, category, zone_id)) {
            match &target.channel {
                NotificationChannel::Mattermost(mm) => {
                    send_with_retry("Mattermost", || mattermost::send_mattermost_notification(mm, title, message, color_hex)).await;
                }
                NotificationChannel::Discord(dc) => {
                    send_with_retry("Discord", || discord::send_discord_notification(dc, title, message, color_hex)).await;
                }
                NotificationChannel::Pushover(po) => {
                    send_with_retry("Pushover", || pushover::send_pushover_notification(po, title, message)).await;
                }
                NotificationChannel::Webhook(wh) => {
                    send_with_retry("Webhook", || webhook::send_generic_webhook(wh, event_type, title, message, None)).await;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn dispatch_rich(
        config: &NotificationConfig,
        event_type: &str,
        zone_id: Option<&str>,
        title: &str,
        overview: &str,
        poster_url: Option<&str>,
        fields: Vec<(&str, &str, bool)>,
        color_hex: Option<&str>,
    ) {
        let _ = Self::dispatch_rich_or_update(
            config,
            event_type,
            zone_id,
            None,
            None,
            title,
            overview,
            poster_url,
            fields,
            color_hex,
        ).await;
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn dispatch_rich_or_update(
        config: &NotificationConfig,
        event_type: &str,
        zone_id: Option<&str>,
        existing_post_id: Option<&str>,
        root_id: Option<&str>,
        title: &str,
        overview: &str,
        poster_url: Option<&str>,
        fields: Vec<(&str, &str, bool)>,
        color_hex: Option<&str>,
    ) -> Option<String> {
        let category = categorize_event(event_type);
        let mut out_post_id: Option<String> = None;
        let mut mattermost_seen = false;

        for target in config.targets.iter().filter(|t| target_applies(t, category, zone_id)) {
            match &target.channel {
                NotificationChannel::Mattermost(mm) => {
                    // Living-card in-place updates need a single post-id per grab, tracked on
                    // the grab record itself — that only works for one Mattermost target. The
                    // first applicable one in list order gets the real update-in-place behavior;
                    // any additional Mattermost targets just get an independent fresh post.
                    if !mattermost_seen {
                        mattermost_seen = true;
                        let res_id = send_with_retry("Mattermost", || {
                            mattermost::dispatch_or_update_mattermost_card(
                                mm, existing_post_id, root_id, title, overview, poster_url, fields.clone(), color_hex,
                            )
                        }).await.flatten();
                        if res_id.is_some() {
                            out_post_id = res_id;
                        }
                    } else {
                        send_with_retry("Mattermost", || {
                            mattermost::send_mattermost_notification(mm, title, overview, color_hex)
                        }).await;
                    }
                }
                NotificationChannel::Discord(dc) => {
                    send_with_retry("Discord", || {
                        discord::send_discord_rich_notification(dc, title, overview, poster_url, fields.clone(), color_hex)
                    }).await;
                }
                NotificationChannel::Pushover(po) => {
                    send_with_retry("Pushover", || pushover::send_pushover_notification(po, title, overview)).await;
                }
                NotificationChannel::Webhook(wh) => {
                    send_with_retry("Webhook", || webhook::send_generic_webhook(wh, event_type, title, overview, Some(&fields))).await;
                }
            }
        }

        out_post_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DiscordConfig;

    fn discord_target(name: &str, categories: Vec<&str>, zone_id: Option<&str>) -> NotificationTarget {
        NotificationTarget {
            id: name.to_string(),
            name: name.to_string(),
            enabled: true,
            channel: NotificationChannel::Discord(DiscordConfig {
                enabled: true,
                webhook_url: "https://discord.example/hook".to_string(),
                username: "Conduit".to_string(),
                avatar_url: None,
            }),
            categories: categories.into_iter().map(|c| c.to_string()).collect(),
            zone_id: zone_id.map(|z| z.to_string()),
        }
    }

    #[test]
    fn categorizes_known_event_types() {
        assert_eq!(categorize_event("sonarr.grab"), "grab");
        assert_eq!(categorize_event("radarr.added"), "grab");
        assert_eq!(categorize_event("ombi.request"), "grab");
        assert_eq!(categorize_event("torrent.enriched"), "grab");
        assert_eq!(categorize_event("radarr.download"), "download");
        assert_eq!(categorize_event("torrent.started"), "download");
        // The pipeline's self-heal event is deliberately "replacement", not "error" -- routine
        // cleanup of a trumped/unregistered torrent, not an actual problem.
        assert_eq!(categorize_event("torrent.error_fixed"), "replacement");
        assert_eq!(categorize_event("torrent.replaced"), "replacement");
        assert_eq!(categorize_event("sonarr.delete"), "replacement");
        // health.* previously fell through to an always-on catch-all with no toggle at all.
        assert_eq!(categorize_event("health.alert"), "health");
        assert_eq!(categorize_event("health.recovery"), "health");
        assert_eq!(categorize_event("radarr.health"), "health");
        assert_eq!(categorize_event("sonarr.update"), "health");
        assert_eq!(categorize_event("space.autopurge"), "autopurge");
        assert_eq!(categorize_event("arr.sync"), "sync");
        assert_eq!(categorize_event("radarr.rename"), "sync");
        assert_eq!(categorize_event("media.scrobble"), "scrobble");
        assert_eq!(categorize_event("media.rate"), "scrobble");
        assert_eq!(categorize_event("radarr.manual"), "error");
        assert_eq!(categorize_event("something.unrecognized"), "error");
    }

    #[test]
    fn disabled_target_never_applies() {
        let mut t = discord_target("d1", vec![], None);
        t.enabled = false;
        assert!(!target_applies(&t, "health", None));
    }

    #[test]
    fn empty_categories_means_every_category() {
        let t = discord_target("d1", vec![], None);
        assert!(target_applies(&t, "health", None));
        assert!(target_applies(&t, "grab", None));
    }

    #[test]
    fn category_filter_is_respected() {
        let t = discord_target("d1", vec!["health"], None);
        assert!(target_applies(&t, "health", None));
        assert!(!target_applies(&t, "grab", None));
    }

    #[test]
    fn global_target_fires_for_every_zone_and_unzoned_events() {
        let t = discord_target("d1", vec![], None); // zone_id: None = global
        assert!(target_applies(&t, "grab", None));
        assert!(target_applies(&t, "grab", Some("4k")));
        assert!(target_applies(&t, "grab", Some("general")));
    }

    #[test]
    fn zone_scoped_target_only_fires_for_its_own_zone() {
        let t = discord_target("d1", vec![], Some("4k"));
        assert!(target_applies(&t, "grab", Some("4k")));
        assert!(!target_applies(&t, "grab", Some("general")));
        // No zone context on the event at all (e.g. a health check with no zone attribution) --
        // not attributable to this zone-scoped target, so it's skipped.
        assert!(!target_applies(&t, "grab", None));
    }
}
