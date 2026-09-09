// src/api/auth_routes.rs
use crate::auth::{
    create_jwt, generate_totp_secret, get_otpauth_url, hash_api_token, hash_password,
    verify_totp_code_at, CreatePairTokenResponse, MobilePairRequest, MobilePairResponse,
    RateLimiter, RequireAdmin, RequireAuth, WsTicketStore,
};
use crate::config::{ConfigManager, FetcherNodeConfig};
use crate::db::{ApiTokenRecord, Database, UserRecord};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

/// Returns " Secure;" when the request reached us over HTTPS (directly, or via a reverse proxy
/// that sets X-Forwarded-Proto), so session cookies aren't marked Secure on a plain-HTTP-only
/// deployment — which would otherwise make the browser silently refuse to send them back.
fn secure_cookie_attr(headers: &HeaderMap) -> &'static str {
    let is_https = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("https"))
        .unwrap_or(false)
        || headers
            .get("x-forwarded-ssl")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("on"))
            .unwrap_or(false);

    if is_https { " Secure;" } else { "" }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupStatusResponse {
    /// True if no admin user exists and setup wizard is needed
    pub setup_needed: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetupRequest {
    /// Initial administrator username
    pub admin_username: String,
    /// Initial administrator password (minimum 4 characters)
    pub admin_password: String,
    /// Optional first fetcher daemon node to connect
    pub initial_node: Option<FetcherNodeConfig>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Administrator or user username
    pub username: String,
    /// User password
    pub password: String,
    /// Optional 6-digit TOTP two-factor authentication code
    pub totp_code: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthResponse {
    /// JWT Bearer access token
    pub token: String,
    /// Authenticated user record
    pub user: UserRecord,
    /// True if 2FA code is required to complete authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_2fa: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateProfileRequest {
    /// Updated account username
    pub username: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    /// Existing password
    pub current_password: String,
    /// New password (minimum 4 characters)
    pub new_password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Setup2FaResponse {
    /// Base32 secret key for manual entry
    pub secret: String,
    /// otpauth:// URI for scanning with Authenticator apps
    pub otpauth_url: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct Verify2FaRequest {
    /// 6-digit code from authenticator app
    pub code: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct Disable2FaRequest {
    /// User password to confirm 2FA deactivation
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTokenRequest {
    /// Human-readable label for the token (e.g. "HomeAssistant", "CLI")
    pub name: String,
    /// Scopes granted to this token (e.g. ["torrents:read", "torrents:write", "*"])
    pub scopes: Vec<String>,
    /// Optional expiration duration in days
    pub expires_days: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateTokenResponse {
    /// Metadata record of the created token
    pub token_record: ApiTokenRecord,
    /// Plaintext token key (cnd_...) - only shown once upon generation
    pub raw_token: String,
}

#[utoipa::path(
    get,
    path = "/api/auth/setup-status",
    tag = "Auth",
    summary = "Check first-run setup status",
    description = "Returns whether the Conduit first-run onboarding setup wizard is required (i.e. no users exist).",
    responses(
        (status = 200, description = "Setup status response", body = SetupStatusResponse)
    )
)]
pub async fn get_setup_status(State(db): State<Database>) -> Result<Json<SetupStatusResponse>, StatusCode> {
    let has_users = db.has_users().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(SetupStatusResponse {
        setup_needed: !has_users,
    }))
}

#[utoipa::path(
    post,
    path = "/api/auth/setup",
    tag = "Auth",
    summary = "Complete first-run setup wizard",
    description = "Initializes the root administrator account and optionally registers the first fetcher daemon node.",
    request_body = SetupRequest,
    responses(
        (status = 200, description = "Setup completed successfully and session initialized", body = AuthResponse),
        (status = 400, description = "Setup has already been completed or invalid credentials provided")
    )
)]
pub async fn complete_setup(
    State(db): State<Database>,
    State(config_mgr): State<ConfigManager>,
    headers_in: HeaderMap,
    Json(payload): Json<SetupRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if db.has_users().unwrap_or(false) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Setup has already been completed"}))));
    }

    if payload.admin_username.trim().is_empty() || payload.admin_password.len() < 12 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Username cannot be empty and password must be at least 12 characters"}))));
    }

    let password_hash = hash_password(&payload.admin_password)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let user = db.create_user(payload.admin_username.trim(), &password_hash, true)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    if let Some(node) = payload.initial_node {
        let mut cfg = config_mgr.get().await;
        cfg.nodes.insert(node.name.clone(), node);
        let _ = config_mgr.update(cfg).await;
    }

    let config = config_mgr.get().await;
    let token = create_jwt(&user.id, &user.username, user.is_admin, user.token_version, &config.system.jwt_secret, 30)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        format!("conduit_session={}; Path=/; HttpOnly; SameSite=Strict;{} Max-Age={}", token, secure_cookie_attr(&headers_in), 30 * 86400)
            .parse()
            .unwrap(),
    );

    Ok((headers, Json(AuthResponse { token, user, requires_2fa: None })))
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "Auth",
    summary = "User login",
    description = "Authenticates username and password against Argon2id hash. If 2FA is active, verifies TOTP code before returning session token.",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Authentication successful or 2FA required", body = AuthResponse),
        (status = 401, description = "Invalid username, password, or 2FA passcode")
    )
)]
pub async fn login(
    State(db): State<Database>,
    State(config_mgr): State<ConfigManager>,
    State(rate_limiter): State<RateLimiter>,
    headers_in: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if let Err(retry_secs) = rate_limiter.check(&payload.username) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": format!("Too many failed attempts. Try again in {} seconds.", retry_secs)})),
        ));
    }

    let user_opt = db.get_user_by_username_async(&payload.username).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let user = match user_opt {
        Some(u) => u,
        None => {
            // Constant-time dummy verification to defeat username enumeration timing attacks
            let _ = crate::auth::verify_password_async(payload.password.clone(), crate::auth::DUMMY_ARGON2_HASH.to_string()).await;
            rate_limiter.record_failure(&payload.username);
            return Err((StatusCode::UNAUTHORIZED, Json(json!({"error": "Invalid username or password"}))));
        }
    };

    if !crate::auth::verify_password_async(payload.password.clone(), user.password_hash.clone()).await {
        rate_limiter.record_failure(&payload.username);
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"error": "Invalid username or password"}))));
    }

    // Two-factor authentication verification if enabled
    if user.totp_enabled {
        match payload.totp_code {
            None => {
                return Ok((
                    HeaderMap::new(),
                    Json(json!({
                        "requires_2fa": true,
                        "message": "Two-factor authentication code required"
                    })),
                ));
            }
            Some(ref code) => {
                let secret = user.totp_secret.as_deref().unwrap_or("");
                match verify_totp_code_at(secret, code, user.totp_last_step) {
                    Some(step) if db.consume_totp_step(&user.id, step as i64).unwrap_or(false) => {}
                    _ => {
                        rate_limiter.record_failure(&payload.username);
                        return Err((StatusCode::UNAUTHORIZED, Json(json!({"error": "Invalid or already-used two-factor authentication passcode"}))));
                    }
                }
            }
        }
    }

    rate_limiter.record_success(&payload.username);

    let config = config_mgr.get().await;
    let token = create_jwt(&user.id, &user.username, user.is_admin, user.token_version, &config.system.jwt_secret, 30)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        format!("conduit_session={}; Path=/; HttpOnly; SameSite=Strict;{} Max-Age={}", token, secure_cookie_attr(&headers_in), 30 * 86400)
            .parse()
            .unwrap(),
    );

    Ok((headers, Json(json!({
        "token": token,
        "user": user,
        "requires_2fa": false
    }))))
}

#[utoipa::path(
    post,
    path = "/api/auth/logout",
    tag = "Auth",
    summary = "User logout",
    description = "Clears the active session cookie and revokes active token version.",
    responses(
        (status = 200, description = "Session terminated successfully")
    )
)]
pub async fn logout(
    auth: crate::auth::OptionalAuth,
    State(db): State<Database>,
    headers_in: HeaderMap,
) -> impl IntoResponse {
    if let Some(a) = auth.0 {
        let _ = db.increment_user_token_version(&a.user_id);
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        format!("conduit_session=; Path=/; HttpOnly; SameSite=Strict;{} Max-Age=0", secure_cookie_attr(&headers_in))
            .parse()
            .unwrap(),
    );
    (headers, Json(json!({"message": "Logged out successfully"})))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WsTicketResponse {
    /// Single-use ticket, valid for 30 seconds, to authenticate the WebSocket upgrade at
    /// /api/ws?ticket=... — used instead of the JWT itself so the long-lived session token
    /// never has to appear in a URL (proxy logs, browser history).
    pub ticket: String,
}

#[utoipa::path(
    post,
    path = "/api/auth/ws-ticket",
    tag = "Auth",
    summary = "Issue a one-time WebSocket auth ticket",
    description = "Exchanges the caller's existing auth (JWT/cookie/API token) for a short-lived, single-use ticket used to authenticate the /api/ws upgrade without putting the session token itself in a URL.",
    responses(
        (status = 200, description = "Ticket issued", body = WsTicketResponse),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn issue_ws_ticket(
    auth: RequireAuth,
    State(tickets): State<WsTicketStore>,
) -> Json<WsTicketResponse> {
    Json(WsTicketResponse { ticket: tickets.issue(&auth.0.user_id) })
}

#[utoipa::path(
    get,
    path = "/api/auth/me",
    tag = "Auth",
    summary = "Get current authenticated profile",
    description = "Returns profile details for the currently authenticated session.",
    responses(
        (status = 200, description = "Current user profile", body = UserRecord),
        (status = 401, description = "Unauthenticated")
    )
)]
pub async fn get_me(
    auth: RequireAuth,
    State(db): State<Database>,
) -> Result<Json<UserRecord>, StatusCode> {
    if let Ok(Some(user)) = db.get_user_by_id(&auth.0.user_id) {
        Ok(Json(user))
    } else {
        Ok(Json(UserRecord {
            id: auth.0.user_id,
            username: auth.0.username,
            password_hash: String::new(),
            is_admin: auth.0.is_admin,
            totp_enabled: false,
            totp_secret: None,
            token_version: 1,
            totp_last_step: 0,
            created_at: Utc::now(),
        }))
    }
}

#[utoipa::path(
    post,
    path = "/api/auth/profile",
    tag = "Auth",
    summary = "Update user profile",
    description = "Updates the current user's profile username.",
    request_body = UpdateProfileRequest,
    responses(
        (status = 200, description = "Profile updated successfully"),
        (status = 400, description = "Invalid username"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn update_profile(
    auth: RequireAuth,
    State(db): State<Database>,
    Json(payload): Json<UpdateProfileRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let username_clean = payload.username.trim();
    if username_clean.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Username cannot be empty"}))));
    }

    db.update_user_profile(&auth.0.user_id, username_clean)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({"success": true, "message": "Profile updated successfully"})))
}

#[utoipa::path(
    post,
    path = "/api/auth/change-password",
    tag = "Auth",
    summary = "Change account password",
    description = "Verifies the current password and hashes the new password with Argon2id.",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed successfully"),
        (status = 400, description = "Invalid password length or incorrect current password"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn change_password(
    auth: RequireAuth,
    State(db): State<Database>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user = db.get_user_by_id(&auth.0.user_id)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Database error"}))))?
        .ok_or((StatusCode::UNAUTHORIZED, Json(json!({"error": "User not found"}))))?;

    if !crate::auth::verify_password_async(payload.current_password, user.password_hash).await {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Current password is incorrect"}))));
    }

    if payload.new_password.len() < 12 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "New password must be at least 12 characters long"}))));
    }

    let new_hash = hash_password(&payload.new_password)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    db.update_user_password(&auth.0.user_id, &new_hash)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({"success": true, "message": "Password changed successfully"})))
}

#[utoipa::path(
    post,
    path = "/api/auth/2fa/setup",
    tag = "Auth",
    summary = "Initialize Two-Factor Authentication setup",
    description = "Generates a new RFC 6238 TOTP secret key and otpauth URL for Google Authenticator / 1Password.",
    responses(
        (status = 200, description = "2FA setup parameters", body = Setup2FaResponse),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn setup_2fa(
    auth: RequireAuth,
    State(db): State<Database>,
) -> Result<Json<Setup2FaResponse>, (StatusCode, Json<serde_json::Value>)> {
    let secret = generate_totp_secret();
    let otpauth_url = get_otpauth_url(&secret, &auth.0.username, "Conduit");

    db.set_totp_secret(&auth.0.user_id, &secret)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(Setup2FaResponse {
        secret,
        otpauth_url,
    }))
}

#[utoipa::path(
    post,
    path = "/api/auth/2fa/verify",
    tag = "Auth",
    summary = "Verify and enable Two-Factor Authentication",
    description = "Verifies the 6-digit TOTP code and marks 2FA as active for the user account.",
    request_body = Verify2FaRequest,
    responses(
        (status = 200, description = "2FA enabled successfully"),
        (status = 400, description = "Invalid code or secret missing"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn verify_and_enable_2fa(
    auth: RequireAuth,
    State(db): State<Database>,
    Json(payload): Json<Verify2FaRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user = db.get_user_by_id(&auth.0.user_id)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Database error"}))))?
        .ok_or((StatusCode::UNAUTHORIZED, Json(json!({"error": "User not found"}))))?;

    let secret = match user.totp_secret {
        Some(s) if !s.is_empty() => s,
        _ => return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "No 2FA setup in progress. Please click Setup first."})))),
    };

    let step = match verify_totp_code_at(&secret, &payload.code, user.totp_last_step) {
        Some(s) => s,
        None => return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid 6-digit verification code. Please check your authenticator clock and try again."})))),
    };

    db.enable_totp(&auth.0.user_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    let _ = db.consume_totp_step(&auth.0.user_id, step as i64);
    let _ = db.increment_user_token_version(&auth.0.user_id);

    Ok(Json(json!({"success": true, "message": "Two-Factor Authentication is now enabled on your account"})))
}

#[utoipa::path(
    post,
    path = "/api/auth/2fa/disable",
    tag = "Auth",
    summary = "Disable Two-Factor Authentication",
    description = "Verifies account password and deactivates 2FA on the account.",
    request_body = Disable2FaRequest,
    responses(
        (status = 200, description = "2FA disabled successfully"),
        (status = 400, description = "Incorrect password"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn disable_2fa(
    auth: RequireAuth,
    State(db): State<Database>,
    Json(payload): Json<Disable2FaRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user = db.get_user_by_id(&auth.0.user_id)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Database error"}))))?
        .ok_or((StatusCode::UNAUTHORIZED, Json(json!({"error": "User not found"}))))?;

    if !crate::auth::verify_password_async(payload.password, user.password_hash).await {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Password is incorrect"}))));
    }

    db.disable_totp(&auth.0.user_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    let _ = db.increment_user_token_version(&auth.0.user_id);

    Ok(Json(json!({"success": true, "message": "Two-Factor Authentication disabled successfully"})))
}

#[utoipa::path(
    get,
    path = "/api/auth/tokens",
    tag = "Auth",
    summary = "List all API tokens",
    description = "Lists all generated API tokens and their assigned scopes.",
    responses(
        (status = 200, description = "List of API tokens", body = [ApiTokenRecord]),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn list_tokens(
    _admin: RequireAdmin,
    State(db): State<Database>,
) -> Result<Json<Vec<ApiTokenRecord>>, StatusCode> {
    let tokens = db.list_api_tokens().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(tokens))
}

#[utoipa::path(
    post,
    path = "/api/auth/tokens",
    tag = "Auth",
    summary = "Generate scoped API token",
    description = "Generates a new secure API token (cnd_...) with designated access scopes.",
    request_body = CreateTokenRequest,
    responses(
        (status = 200, description = "API token created", body = CreateTokenResponse),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn create_token(
    _admin: RequireAdmin,
    State(db): State<Database>,
    Json(payload): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, (StatusCode, Json<serde_json::Value>)> {
    let mut raw_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut raw_bytes);
    let raw_token = format!("cnd_{}", base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, raw_bytes));
    let token_hash = hash_api_token(&raw_token);

    let expires_at = payload.expires_days.map(|d| Utc::now() + chrono::Duration::days(d));

    let record = db.create_api_token(&payload.name, &token_hash, &payload.scopes, expires_at)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(CreateTokenResponse {
        token_record: record,
        raw_token,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/auth/tokens/{id}",
    tag = "Auth",
    summary = "Revoke API token",
    description = "Deletes and immediately revokes an active API token.",
    params(
        ("id" = String, Path, description = "Unique ID of the token to revoke")
    ),
    responses(
        (status = 200, description = "Token revoked successfully"),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn delete_token(
    _admin: RequireAdmin,
    State(db): State<Database>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    db.delete_api_token(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({"message": "Token revoked successfully"})))
}

#[utoipa::path(
    post,
    path = "/api/auth/mobile/pair-token",
    tag = "Auth",
    summary = "Generate Mobile QR Pairing Token",
    description = "Generates a single-use, 3-minute QR pairing token for Flutter mobile app onboarding.",
    responses(
        (status = 200, description = "Pairing session created", body = CreatePairTokenResponse),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn create_mobile_pair_token(
    auth: RequireAuth,
    headers: HeaderMap,
    State(state): State<crate::auth::AppState>,
) -> Result<Json<crate::auth::CreatePairTokenResponse>, (StatusCode, Json<serde_json::Value>)> {
    let session = state.mobile_pairs.create_session(&auth.0.user_id, &auth.0.username, auth.0.is_admin);

    let config = state.config.get().await;
    let scheme = headers.get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(if config.system.ssl_enabled { "https" } else { "http" });

    let host = headers.get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let base_url = if !host.is_empty() && !host.starts_with("0.0.0.0") && !host.starts_with("127.0.0.1") {
        format!("{}://{}", scheme, host)
    } else {
        format!("{}://{}:{}", scheme, config.system.bind_addr, config.system.port)
    };

    let qr_payload = json!({
        "conduit_url": base_url,
        "pair_code": session.pair_code,
        "user_id": session.user_id,
        "username": session.username,
        "expires_at": session.expires_at.to_rfc3339(),
    }).to_string();

    Ok(Json(crate::auth::CreatePairTokenResponse {
        pair_code: session.pair_code,
        qr_payload,
        server_url: Some(base_url),
        expires_in_secs: 180,
    }))
}

#[utoipa::path(
    post,
    path = "/api/auth/mobile/pair",
    tag = "Auth",
    summary = "Redeem Mobile QR Pairing Token",
    description = "Exchanges a scanned QR pairing code for an authenticated session token for the Flutter mobile app.",
    request_body = MobilePairRequest,
    responses(
        (status = 200, description = "Mobile paired successfully", body = MobilePairResponse),
        (status = 400, description = "Invalid or expired pair code")
    )
)]
pub async fn redeem_mobile_pair(
    State(state): State<crate::auth::AppState>,
    Json(payload): Json<crate::auth::MobilePairRequest>,
) -> Result<Json<crate::auth::MobilePairResponse>, (StatusCode, Json<serde_json::Value>)> {
    let session = state.mobile_pairs.redeem_session(&payload.pair_code)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid or expired mobile pairing code"}))))?;

    let user = state.db.get_user_by_id(&session.user_id)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Database error"}))))?
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, Json(json!({"error": "User not found"}))))?;

    let config = state.config.get().await;
    // Issue a 90-day mobile session JWT
    let token = crate::auth::create_jwt(
        &user.id,
        &user.username,
        user.is_admin,
        user.token_version,
        &config.system.jwt_secret,
        90,
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let device_info = payload.device_name.as_deref().unwrap_or("Mobile App");
    let msg = format!("📱 Mobile device '{}' successfully paired for user '{}'", device_info, user.username);
    let _ = state.db.log_event("auth", "info", &msg, None);

    Ok(Json(crate::auth::MobilePairResponse {
        token,
        user,
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        ws_endpoint: "/api/ws".to_string(),
    }))
}

