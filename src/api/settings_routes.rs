// src/api/settings_routes.rs
use crate::auth::RequireAdmin;
use crate::config::{
    create_backup_bundle, restore_backup_bundle, AppConfig, BackupBundle, BackupExportRequest,
    BackupRestoreRequest, ConfigManager,
};
use crate::db::{Database, RekeyDbRequest};
use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;

#[utoipa::path(
    get,
    path = "/api/settings",
    tag = "Settings",
    summary = "Get Conduit application settings",
    description = "Retrieves full application configuration (nodes, sync paths, Arr sync, Plex/Trakt, notifications, pipeline rules).",
    responses(
        (status = 200, description = "Current Conduit configuration", body = AppConfig),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn get_settings(
    _admin: RequireAdmin,
    State(config_mgr): State<ConfigManager>,
) -> Json<AppConfig> {
    Json(config_mgr.get().await)
}

#[utoipa::path(
    put,
    path = "/api/settings",
    tag = "Settings",
    summary = "Update Conduit application settings",
    description = "Saves and persists modified application configuration.",
    request_body = AppConfig,
    responses(
        (status = 200, description = "Settings updated successfully"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn update_settings(
    _admin: RequireAdmin,
    State(config_mgr): State<ConfigManager>,
    Json(mut new_config): Json<AppConfig>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // A settings page opened before a Trakt token refresh must not put the old tokens back.
    crate::trakt_client::keep_newer_tokens(&config_mgr.get().await.trakt, &mut new_config.trakt);
    config_mgr.update(new_config).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({"message": "Settings updated successfully"})))
}

#[utoipa::path(
    post,
    path = "/api/settings/backup",
    tag = "Settings",
    summary = "Export configuration backup bundle",
    description = "Creates an encrypted (AES-256-GCM) or plaintext backup bundle of the application configuration.",
    request_body = BackupExportRequest,
    responses(
        (status = 200, description = "Backup bundle created", body = BackupBundle),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn export_backup(
    _admin: RequireAdmin,
    State(config_mgr): State<ConfigManager>,
    Json(req): Json<BackupExportRequest>,
) -> Result<Json<BackupBundle>, (StatusCode, Json<serde_json::Value>)> {
    let passphrase = req.passphrase.as_deref().unwrap_or("").trim();
    if passphrase.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "A passphrase of at least 8 characters is required to export a backup — it contains every configured API key, credential, and secret in plaintext once decrypted."})),
        ));
    }

    let cfg = config_mgr.get().await;
    let bundle = create_backup_bundle(&cfg, Some(passphrase))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(bundle))
}

#[utoipa::path(
    post,
    path = "/api/settings/restore",
    tag = "Settings",
    summary = "Restore configuration from backup bundle",
    description = "Restores and decrypts an application configuration backup bundle.",
    request_body = BackupRestoreRequest,
    responses(
        (status = 200, description = "Configuration restored successfully"),
        (status = 400, description = "Invalid backup or incorrect passphrase"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn restore_backup(
    _admin: RequireAdmin,
    State(config_mgr): State<ConfigManager>,
    Json(req): Json<BackupRestoreRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let restored_config = restore_backup_bundle(&req.bundle, req.passphrase.as_deref())
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))))?;

    config_mgr.update(restored_config).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({"message": "Settings restored successfully"})))
}

#[utoipa::path(
    post,
    path = "/api/settings/rekey-db",
    tag = "Settings",
    summary = "Rotate database encryption key",
    description = "Rotates the live SQLite database encryption key using SQLCipher PRAGMA rekey.",
    request_body = RekeyDbRequest,
    responses(
        (status = 200, description = "Database rekeyed successfully"),
        (status = 400, description = "Invalid key"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn rekey_database(
    _admin: RequireAdmin,
    State(db): State<Database>,
    Json(req): Json<RekeyDbRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if req.new_key.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "New encryption key cannot be empty"}))));
    }

    db.rekey(&req.new_key)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Rekey failed: {}", e)}))))?;

    Ok(Json(json!({"message": "Database rekeyed successfully with new encryption key"})))
}
