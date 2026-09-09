// src/auth/middleware.rs
use super::jwt::verify_jwt;
use super::hash_api_token;
use super::rate_limit::RateLimiter;
use super::ws_ticket::WsTicketStore;
use crate::config::ConfigManager;
use crate::db::Database;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AuthContext {
    pub user_id: String,
    pub username: String,
    pub is_admin: bool,
    pub scopes: Vec<String>, // If authenticated via API token, lists permitted scopes
    pub is_api_token: bool,
}

#[allow(dead_code)]
impl AuthContext {
    pub fn has_scope(&self, required_scope: &str) -> bool {
        if self.is_admin {
            return true;
        }
        if !self.is_api_token {
            return true; // Web UI logged-in users have full access
        }
        self.scopes.iter().any(|s| s == "*" || s == required_scope)
    }
}

#[derive(Clone)]
pub struct RequireAuth(pub AuthContext);
#[derive(Clone)]
pub struct OptionalAuth(pub Option<AuthContext>);
#[derive(Clone)]
#[allow(dead_code)]
pub struct RequireAdmin(pub AuthContext);

#[derive(Clone)]
pub struct AppState {
    pub config: ConfigManager,
    pub db: Database,
    pub fetcher_pool: Arc<crate::fetcher::FetcherPool>,
    pub notifiers: Arc<crate::notify::NotificationManager>,
    pub rate_limiter: RateLimiter,
    pub ws_tickets: WsTicketStore,
    pub mobile_pairs: crate::auth::mobile::MobilePairingStore,
    pub arr_stats_cache: crate::engines::arr_stats_poller::ArrStatsCache,
    pub engine_registry: crate::engines::registry::EngineRegistry,
    pub event_bus: crate::events::EventBus,
    pub ip_asn_db: crate::ip_asn::IpAsnCache,
}

impl FromRef<AppState> for ConfigManager {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}

impl FromRef<AppState> for Database {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}

impl FromRef<AppState> for Arc<crate::fetcher::FetcherPool> {
    fn from_ref(state: &AppState) -> Self {
        state.fetcher_pool.clone()
    }
}

impl FromRef<AppState> for Arc<crate::notify::NotificationManager> {
    fn from_ref(state: &AppState) -> Self {
        state.notifiers.clone()
    }
}

impl FromRef<AppState> for RateLimiter {
    fn from_ref(state: &AppState) -> Self {
        state.rate_limiter.clone()
    }
}

impl FromRef<AppState> for WsTicketStore {
    fn from_ref(state: &AppState) -> Self {
        state.ws_tickets.clone()
    }
}

impl FromRef<AppState> for crate::ip_asn::IpAsnCache {
    fn from_ref(state: &AppState) -> Self {
        state.ip_asn_db.clone()
    }
}

impl<S> FromRequestParts<S> for RequireAuth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let config = app_state.config.get().await;
        let db = app_state.db;

        // 1. Check Authorization header
        if let Some(auth_header) = parts.headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()) {
            if let Some(token) = auth_header.strip_prefix("Bearer ") {
                let token = token.trim();
                
                // Try JWT first
                if let Ok(claims) = verify_jwt(token, &config.system.jwt_secret) {
                    if let Ok(Some(user_rec)) = db.get_user_by_id_async(&claims.sub).await {
                        if claims.token_version == user_rec.token_version {
                            return Ok(RequireAuth(AuthContext {
                                user_id: claims.sub,
                                username: claims.username,
                                is_admin: user_rec.is_admin,
                                scopes: vec!["*".to_string()],
                                is_api_token: false,
                            }));
                        }
                    }
                }

                // Try API Token hash lookup in DB
                let token_hash = hash_api_token(token);
                if let Ok(Some(tok_rec)) = db.verify_and_touch_token_async(&token_hash).await {
                    return Ok(RequireAuth(AuthContext {
                        user_id: tok_rec.id,
                        username: tok_rec.name,
                        is_admin: tok_rec.scopes.contains(&"admin".to_string()) || tok_rec.scopes.contains(&"*".to_string()),
                        scopes: tok_rec.scopes,
                        is_api_token: true,
                    }));
                }
            } else if let Some(basic_b64) = auth_header.strip_prefix("Basic ") {
                // Support Basic Auth (username:password or username:password:totp_code when 2FA enabled)
                if let Ok(decoded) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, basic_b64.trim()) {
                    if let Ok(creds) = String::from_utf8(decoded) {
                        if let Some((user, pass_and_totp)) = creds.split_once(':') {
                            let limiter = &app_state.rate_limiter;
                            if limiter.check(user).is_ok() {
                                if let Ok(Some(user_rec)) = db.get_user_by_username_async(user).await {
                                    let mut authenticated = false;
                                    if user_rec.totp_enabled {
                                        // When 2FA is active, require password:totp_code
                                        if let Some((pass, totp_code)) = pass_and_totp.rsplit_once(':') {
                                            let secret = user_rec.totp_secret.as_deref().unwrap_or("");
                                            if super::verify_password(pass, &user_rec.password_hash) {
                                                if let Some(step) = super::totp::verify_totp_code_at(secret, totp_code, user_rec.totp_last_step) {
                                                    if db.consume_totp_step_async(&user_rec.id, step as i64).await.unwrap_or(false) {
                                                        authenticated = true;
                                                    }
                                                }
                                            }
                                        }
                                    } else if super::verify_password(pass_and_totp, &user_rec.password_hash) {
                                        authenticated = true;
                                    }

                                    if authenticated {
                                        limiter.record_success(user);
                                        return Ok(RequireAuth(AuthContext {
                                            user_id: user_rec.id,
                                            username: user_rec.username,
                                            is_admin: user_rec.is_admin,
                                            scopes: vec!["*".to_string()],
                                            is_api_token: false,
                                        }));
                                    }
                                }
                                limiter.record_failure(user);
                            }
                        }
                    }
                }
            }
        }

        // 2. Check X-Api-Key header (for fetcher scripts, copy2queue, etc.)
        if let Some(api_key_header) = parts.headers.get("X-Api-Key")
            .or_else(|| parts.headers.get("x-api-key"))
            .or_else(|| parts.headers.get("X-API-KEY"))
            .and_then(|h| h.to_str().ok())
        {
            let token_hash = hash_api_token(api_key_header.trim());
            if let Ok(Some(tok_rec)) = db.verify_and_touch_token_async(&token_hash).await {
                return Ok(RequireAuth(AuthContext {
                    user_id: tok_rec.id,
                    username: tok_rec.name,
                    is_admin: tok_rec.scopes.contains(&"admin".to_string()) || tok_rec.scopes.contains(&"*".to_string()),
                    scopes: tok_rec.scopes,
                    is_api_token: true,
                }));
            }
        }

        // 3. Check Cookie
        if let Some(cookie_header) = parts.headers.get(header::COOKIE).and_then(|h| h.to_str().ok()) {
            for cookie in cookie_header.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix("conduit_session=") {
                    if let Ok(claims) = verify_jwt(token, &config.system.jwt_secret) {
                        if let Ok(Some(user_rec)) = db.get_user_by_id_async(&claims.sub).await {
                            if claims.token_version == user_rec.token_version {
                                return Ok(RequireAuth(AuthContext {
                                    user_id: claims.sub,
                                    username: claims.username,
                                    is_admin: user_rec.is_admin,
                                    scopes: vec!["*".to_string()],
                                    is_api_token: false,
                                }));
                            }
                        }
                    }
                }
            }
        }

        Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "Unauthorized",
                "message": "Valid JWT, API Token, or session cookie required"
            })),
        ).into_response())
    }
}

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth = RequireAuth::from_request_parts(parts, state).await?;
        if !auth.0.is_admin {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "Forbidden",
                    "message": "Administrator privileges required"
                })),
            ).into_response());
        }
        Ok(RequireAdmin(auth.0))
    }
}

impl<S> FromRequestParts<S> for OptionalAuth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match RequireAuth::from_request_parts(parts, state).await {
            Ok(auth) => Ok(OptionalAuth(Some(auth.0))),
            Err(_) => Ok(OptionalAuth(None)),
        }
    }
}
