// src/api/mod.rs
pub mod arr_routes;
pub mod auth_routes;
pub mod integration_routes;
pub mod metrics_routes;
pub mod node_routes;
pub mod op_error;
pub mod openapi;
pub mod plex_oauth_routes;
pub mod settings_routes;
pub mod sync_routes;
pub mod torrent_routes;
pub mod webhook_routes;
pub mod ws;

pub use openapi::ApiDoc;

use crate::auth::AppState;
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub fn build_api_router(state: AppState) -> Router {
    let openapi_doc = ApiDoc::openapi();

    Router::new()
        // Swagger UI at /swagger-ui
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi_doc))
        
        // Auth & Profile & 2FA
        .route("/api/auth/setup-status", get(auth_routes::get_setup_status))
        .route("/api/auth/setup", post(auth_routes::complete_setup))
        .route("/api/auth/login", post(auth_routes::login))
        .route("/api/auth/logout", post(auth_routes::logout))
        .route("/api/auth/me", get(auth_routes::get_me))
        .route("/api/auth/profile", post(auth_routes::update_profile))
        .route("/api/auth/change-password", post(auth_routes::change_password))
        .route("/api/auth/2fa/setup", post(auth_routes::setup_2fa))
        .route("/api/auth/2fa/verify", post(auth_routes::verify_and_enable_2fa))
        .route("/api/auth/2fa/disable", post(auth_routes::disable_2fa))
        .route("/api/auth/tokens", get(auth_routes::list_tokens).post(auth_routes::create_token))
        .route("/api/auth/tokens/{id}", delete(auth_routes::delete_token))
        .route("/api/auth/ws-ticket", post(auth_routes::issue_ws_ticket))
        .route("/api/auth/mobile/pair-token", post(auth_routes::create_mobile_pair_token))
        .route("/api/auth/mobile/pair", post(auth_routes::redeem_mobile_pair))

        // Fetchers / Torrents
        .route("/api/torrents", get(torrent_routes::list_torrents).post(torrent_routes::add_torrent))
        .route("/api/torrents/stats", get(torrent_routes::get_stats))
        .route("/api/torrents/bandwidth-history", get(torrent_routes::get_bandwidth_history))
        .route("/api/torrents/bulk", post(torrent_routes::bulk_action))
        .route("/api/torrents/queue-move", post(torrent_routes::queue_move_bulk))
        .route("/api/torrents/batch-replace-trackers", post(torrent_routes::batch_replace_trackers))
        .route("/api/torrents/migrate", post(torrent_routes::migrate_torrent))
        .route("/api/torrents/{compound_id}", get(torrent_routes::get_torrent).delete(torrent_routes::delete_torrent))
        .route("/api/torrents/{compound_id}/start", post(torrent_routes::start_torrent))
        .route("/api/torrents/{compound_id}/stop", post(torrent_routes::stop_torrent))
        .route("/api/torrents/{compound_id}/location", post(torrent_routes::set_location))
        .route("/api/torrents/{compound_id}/queue-move", post(torrent_routes::queue_move_torrent))
        .route("/api/torrents/{compound_id}/sequential-download", post(torrent_routes::set_sequential_download))
        .route("/api/torrents/{compound_id}/rename-path", post(torrent_routes::rename_torrent_path))
        .route("/api/torrents/{compound_id}/enrich", post(torrent_routes::enrich_torrent))
        .route("/api/torrents/enrich-all", post(torrent_routes::enrich_all_torrents))

        // Nodes & Fetcher Sessions
        .route("/api/nodes", get(node_routes::list_nodes))
        .route("/api/nodes/capabilities", get(node_routes::get_node_capabilities))
        .route("/api/nodes/turtle-mode", post(node_routes::set_turtle_mode_all))
        .route("/api/nodes/{name}/session", get(node_routes::get_node_session).put(node_routes::update_node_session))
        .route("/api/nodes/{name}/test-port", post(node_routes::test_node_port))
        .route("/api/nodes/{name}/turtle-mode", post(node_routes::set_turtle_mode))
        .route("/api/nodes/{name}/blocklist-update", post(node_routes::update_node_blocklist))

        // Settings & Backups
        .route("/api/settings", get(settings_routes::get_settings).put(settings_routes::update_settings))
        .route("/api/settings/backup", post(settings_routes::export_backup))
        .route("/api/settings/restore", post(settings_routes::restore_backup))
        .route("/api/settings/rekey-db", post(settings_routes::rekey_database))

        // Arr Webhooks & Master-Slave Controls (with standard path aliases)
        .route("/api/sonarr/inbound", post(arr_routes::sonarr_inbound))
        .route("/api/webhook/sonarr", post(arr_routes::sonarr_inbound))
        .route("/api/webhooks/sonarr", post(arr_routes::sonarr_inbound))
        .route("/webhook/sonarr", post(arr_routes::sonarr_inbound))
        .route("/api/radarr/inbound", post(arr_routes::radarr_inbound))
        .route("/api/webhook/radarr", post(arr_routes::radarr_inbound))
        .route("/api/webhooks/radarr", post(arr_routes::radarr_inbound))
        .route("/webhook/radarr", post(arr_routes::radarr_inbound))
        .route("/api/lidarr/inbound", post(arr_routes::lidarr_inbound))
        .route("/api/webhook/lidarr", post(arr_routes::lidarr_inbound))
        .route("/api/webhooks/lidarr", post(arr_routes::lidarr_inbound))
        .route("/webhook/lidarr", post(arr_routes::lidarr_inbound))
        .route("/api/arr/test-connection", post(arr_routes::test_arr_connection))
        .route("/api/arr/sync-now", post(arr_routes::trigger_arr_sync_now))
        .route("/api/arr/stats", get(arr_routes::get_arr_stats))
        .route("/api/arr/grabs", get(arr_routes::list_grabs))
        .route("/api/arr/pipeline", get(arr_routes::list_pipeline).delete(arr_routes::purge_pipeline))
        .route("/api/arr/pipeline/{id}", delete(arr_routes::delete_pipeline_item))
        .route("/api/arr/pipeline/{id}/history", get(arr_routes::get_grab_history))
        .route("/api/arr/pipeline/{id}/re-search", post(arr_routes::re_search_pipeline_item))
        .route("/api/arr/search/releases", get(arr_routes::search_releases))
        .route("/api/arr/search/grab", post(arr_routes::grab_release))

        // Integrations (Plex, Trakt, Notifications, Pipeline Regex, Webhooks)
        .route("/api/plex/inbound", post(webhook_routes::plex_inbound))
        .route("/api/webhook/plex", post(webhook_routes::plex_inbound))
        .route("/api/webhooks/plex", post(webhook_routes::plex_inbound))
        .route("/webhook/plex", post(webhook_routes::plex_inbound))
        .route("/api/plex/scrobbles", get(webhook_routes::list_plex_scrobbles))
        .route("/api/ombi/inbound", post(webhook_routes::ombi_inbound))
        .route("/api/webhook/ombi", post(webhook_routes::ombi_inbound))
        .route("/api/webhooks/ombi", post(webhook_routes::ombi_inbound))
        .route("/webhook/ombi", post(webhook_routes::ombi_inbound))
        .route("/api/ombi/requests", get(webhook_routes::list_ombi_requests))
        .route("/api/overseerr/inbound", post(webhook_routes::overseerr_inbound))
        .route("/api/webhook/overseerr", post(webhook_routes::overseerr_inbound))
        .route("/api/webhooks/overseerr", post(webhook_routes::overseerr_inbound))
        .route("/webhook/overseerr", post(webhook_routes::overseerr_inbound))
        .route("/api/bazarr/inbound", post(webhook_routes::bazarr_inbound))
        .route("/api/webhook/bazarr", post(webhook_routes::bazarr_inbound))
        .route("/api/webhooks/bazarr", post(webhook_routes::bazarr_inbound))
        .route("/webhook/bazarr", post(webhook_routes::bazarr_inbound))
        .route("/api/jellyfin/inbound", post(webhook_routes::jellyfin_inbound))
        .route("/api/webhook/jellyfin", post(webhook_routes::jellyfin_inbound))
        .route("/api/webhooks/jellyfin", post(webhook_routes::jellyfin_inbound))
        .route("/webhook/jellyfin", post(webhook_routes::jellyfin_inbound))
        .route("/api/plex/test-connection", post(integration_routes::test_plex_connection))
        .route("/api/plex/refresh", post(integration_routes::refresh_plex_sections))
        .route("/api/plex/oauth/pin", post(plex_oauth_routes::create_oauth_pin))
        .route("/api/plex/oauth/poll/{pin_id}", get(plex_oauth_routes::poll_oauth_pin))
        .route("/api/plex/oauth/add-server", post(plex_oauth_routes::add_oauth_plex_server))
        .route("/api/trakt/test-connection", post(integration_routes::test_trakt_connection))
        .route("/api/notifications/test", post(integration_routes::send_test_notification))
        .route("/api/pipeline/test-regex", post(integration_routes::test_pipeline_regex))

        // File Staging & Queue Routing
        .route("/api/sync/routes", get(sync_routes::get_queue_routes))
        .route("/api/sync/classify", post(sync_routes::classify_file))
        .route("/api/sync/notify-download", post(sync_routes::notify_download))
        .route("/api/sync/trackers", get(sync_routes::get_tracker_mappings).post(sync_routes::save_tracker_mappings))
        .route("/api/sync/media-types", get(sync_routes::get_media_types).post(sync_routes::save_media_types))
        .route("/api/nodes/{name}/media-overrides", put(sync_routes::update_node_media_overrides))
        .route("/api/sync/hook-script", get(sync_routes::get_hook_script))

        // Events & Health
        .route("/api/events", get(sync_routes::get_events))
        .route("/api/health", get(sync_routes::health_check))

        // Metrics & System
        .route("/metrics", get(metrics_routes::prometheus_metrics))
        .route("/api/system/stats", get(metrics_routes::get_system_stats))
        .route("/api/system/health", get(metrics_routes::get_system_health))
        .route("/api/system/circuit-breakers/{host}/trip", post(metrics_routes::force_trip_circuit_breaker))
        .route("/api/system/circuit-breakers/{host}/reset", post(metrics_routes::force_reset_circuit_breaker))
        .route("/api/system/engines", get(metrics_routes::get_engine_health))
        .route("/api/system/crash-log", get(metrics_routes::get_crash_log))

        // Realtime WebSocket
        .route("/api/ws", get(ws::ws_handler))

        .with_state(state)
}
