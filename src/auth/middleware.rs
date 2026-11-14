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
    pub role: String,
    pub scopes: Vec<String>,
    pub is_api_token: bool,
}

pub fn scopes_for_role(role: &str) -> Vec<String> {
    match crate::db::normalize_role(role) {
        "admin" => vec!["*".to_string()],
        "operator" => vec![
            "torrents:read".to_string(),
            "torrents:write".to_string(),
            "system:read".to_string(),
            "fetcher".to_string(),
            "sync".to_string(),
            "sync:write".to_string(),
            "arr:write".to_string(),
        ],
        "requester" => vec![
            "torrents:read".to_string(),
            "system:read".to_string(),
            "arr:write".to_string(),
        ],
        _ => vec![
            "torrents:read".to_string(),
            "system:read".to_string(),
        ],
    }
}

impl AuthContext {
    #[allow(dead_code)]
    pub fn has_scope(&self, required_scope: &str) -> bool {
        self.has_any_scope(&[required_scope])
    }

    pub fn has_any_scope(&self, required_scopes: &[&str]) -> bool {
        if self.is_admin {
            return true;
        }
        self.scopes
            .iter()
            .any(|s| s == "*" || required_scopes.contains(&s.as_str()))
    }
}

#[derive(Clone)]
pub struct RequireAuth(pub AuthContext);
#[derive(Clone)]
pub struct OptionalAuth(pub Option<AuthContext>);
#[derive(Clone)]
pub struct RequireAdmin(#[allow(dead_code)] pub AuthContext);
#[derive(Clone)]
pub struct RequireRead(#[allow(dead_code)] pub AuthContext);
#[derive(Clone)]
pub struct RequireTorrentsWrite(pub AuthContext);
#[derive(Clone)]
pub struct RequireSync(#[allow(dead_code)] pub AuthContext);
#[derive(Clone)]
pub struct RequireArrWrite(#[allow(dead_code)] pub AuthContext);

#[derive(Clone, Debug)]
pub struct ClientIp(pub String);

fn is_trusted_proxy_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local()
        }
    }
}

pub fn extract_client_ip(parts: &Parts) -> String {
    let peer_ip = parts
        .extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip());

    if let Some(ip) = peer_ip {
        if is_trusted_proxy_ip(ip) {
            if let Some(xff) = parts.headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
                // A trusted proxy appends the peer IP *it* saw to the end of the list, so the
                // client-supplied prefix is otherwise attacker-controlled. Walk from the right
                // and return the first entry that isn't itself another trusted-proxy hop.
                for candidate in xff.split(',').rev().map(str::trim) {
                    if let Ok(parsed) = candidate.parse::<std::net::IpAddr>() {
                        if !is_trusted_proxy_ip(parsed) {
                            return parsed.to_string();
                        }
                    }
                }
            }
            if let Some(xri) = parts.headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
                let trimmed = xri.trim();
                if trimmed.parse::<std::net::IpAddr>().is_ok() {
                    return trimmed.to_string();
                }
            }
        }
        return ip.to_string();
    }

    "unknown".to_string()
}

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(ClientIp(extract_client_ip(parts)))
    }
}

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
    pub hardlink_cache: crate::engines::hardlink_checker::HardlinkCache,
    pub oidc_states: crate::auth::oidc::OidcStateStore,
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
                                scopes: scopes_for_role(&user_rec.role),
                                role: user_rec.role,
                                is_api_token: false,
                            }));
                        }
                    }
                }

                // Try API Token hash lookup in DB
                let token_hash = hash_api_token(token);
                if let Ok(Some(tok_rec)) = db.verify_and_touch_token_async(&token_hash).await {
                    let tok_is_admin = tok_rec.scopes.contains(&"admin".to_string()) || tok_rec.scopes.contains(&"*".to_string());
                    return Ok(RequireAuth(AuthContext {
                        user_id: tok_rec.id,
                        username: tok_rec.name,
                        is_admin: tok_is_admin,
                        role: if tok_is_admin { "admin".to_string() } else { "token".to_string() },
                        scopes: tok_rec.scopes,
                        is_api_token: true,
                    }));
                }
            } else if let Some(basic_b64) = auth_header.strip_prefix("Basic ") {
                // Support Basic Auth (username:password or username:password:totp_code when 2FA enabled)
                if let Ok(decoded) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, basic_b64.trim()) {
                    if let Ok(creds) = String::from_utf8(decoded) {
                        if let Some((user, pass_and_totp)) = creds.split_once(':') {
                            let client_ip = extract_client_ip(parts);
                            let limiter = &app_state.rate_limiter;
                            if limiter.check_login(&client_ip, user).is_ok() {
                                if let Ok(Some(user_rec)) = db.get_user_by_username_async(user).await {
                                    let mut authenticated = false;
                                    if user_rec.totp_enabled {
                                        // When 2FA is active, require password:totp_code
                                        if let Some((pass, totp_code)) = pass_and_totp.rsplit_once(':') {
                                            let secret = user_rec.totp_secret.as_deref().unwrap_or("");
                                            if super::verify_password_async(pass.to_string(), user_rec.password_hash.clone()).await {
                                                if let Some(step) = super::totp::verify_totp_code_at(secret, totp_code, user_rec.totp_last_step) {
                                                    if db.consume_totp_step_async(&user_rec.id, step as i64).await.unwrap_or(false) {
                                                        authenticated = true;
                                                    }
                                                }
                                            }
                                        }
                                    } else if super::verify_password_async(pass_and_totp.to_string(), user_rec.password_hash.clone()).await {
                                        authenticated = true;
                                    }

                                    if authenticated {
                                        limiter.record_login_success(&client_ip, user);
                                        return Ok(RequireAuth(AuthContext {
                                            user_id: user_rec.id,
                                            username: user_rec.username,
                                            is_admin: user_rec.is_admin,
                                            scopes: scopes_for_role(&user_rec.role),
                                            role: user_rec.role,
                                            is_api_token: false,
                                        }));
                                    }
                                }
                                limiter.record_login_failure(&client_ip, user);
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
                let tok_is_admin = tok_rec.scopes.contains(&"admin".to_string()) || tok_rec.scopes.contains(&"*".to_string());
                return Ok(RequireAuth(AuthContext {
                    user_id: tok_rec.id,
                    username: tok_rec.name,
                    is_admin: tok_is_admin,
                    role: if tok_is_admin { "admin".to_string() } else { "token".to_string() },
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
                                    scopes: scopes_for_role(&user_rec.role),
                                    role: user_rec.role,
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

macro_rules! impl_scope_extractor {
    ($name:ident, $scopes:expr, $msg:expr) => {
        impl<S> FromRequestParts<S> for $name
        where
            S: Send + Sync,
            AppState: FromRef<S>,
        {
            type Rejection = Response;

            async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
                let auth = RequireAuth::from_request_parts(parts, state).await?;
                if !auth.0.has_any_scope($scopes) {
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(json!({
                            "error": "Forbidden",
                            "message": $msg
                        })),
                    )
                        .into_response());
                }
                Ok($name(auth.0))
            }
        }
    };
}

impl_scope_extractor!(
    RequireRead,
    &["torrents:read", "system:read", "fetcher"],
    "Read scope required (torrents:read, system:read, or fetcher)"
);
impl_scope_extractor!(
    RequireTorrentsWrite,
    &["torrents:write", "fetcher"],
    "Torrent write scope required (torrents:write or fetcher)"
);
impl_scope_extractor!(
    RequireSync,
    &["sync", "sync:write"],
    "Sync scope required (sync or sync:write)"
);
impl_scope_extractor!(
    RequireArrWrite,
    &["arr:write", "fetcher"],
    "Arr write scope required (arr:write or fetcher)"
);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbac_role_scopes() {
        let admin = AuthContext {
            user_id: "1".into(),
            username: "a".into(),
            is_admin: true,
            role: "admin".into(),
            scopes: scopes_for_role("admin"),
            is_api_token: false,
        };
        assert!(admin.has_any_scope(&["torrents:write"]));
        assert!(admin.has_any_scope(&["sync:write"]));

        let viewer = AuthContext {
            user_id: "2".into(),
            username: "v".into(),
            is_admin: false,
            role: "viewer".into(),
            scopes: scopes_for_role("viewer"),
            is_api_token: false,
        };
        assert!(viewer.has_any_scope(&["torrents:read", "system:read"]));
        assert!(!viewer.has_any_scope(&["torrents:write"]));
        assert!(!viewer.has_any_scope(&["arr:write"]));

        let requester = AuthContext {
            user_id: "3".into(),
            username: "r".into(),
            is_admin: false,
            role: "requester".into(),
            scopes: scopes_for_role("requester"),
            is_api_token: false,
        };
        assert!(requester.has_any_scope(&["arr:write"]));
        assert!(!requester.has_any_scope(&["torrents:write"]));

        let operator = AuthContext {
            user_id: "4".into(),
            username: "o".into(),
            is_admin: false,
            role: "operator".into(),
            scopes: scopes_for_role("operator"),
            is_api_token: false,
        };
        assert!(operator.has_any_scope(&["torrents:write"]));
        assert!(operator.has_any_scope(&["sync:write"]));
    }

    fn parts_from_trusted_peer(peer: &str, xff: Option<&str>) -> Parts {
        let mut req = axum::http::Request::builder().body(()).unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            peer.parse::<std::net::SocketAddr>().unwrap(),
        ));
        if let Some(xff) = xff {
            req.headers_mut().insert("x-forwarded-for", xff.parse().unwrap());
        }
        req.into_parts().0
    }

    #[test]
    fn client_ip_ignores_a_spoofed_prefix_and_uses_the_hop_the_trusted_proxy_appended() {
        // A single trusted nginx/Traefik hop appends the real client IP to the end of the
        // header; anything before that is attacker-supplied and must not be trusted.
        let parts = parts_from_trusted_peer("127.0.0.1:1234", Some("203.0.113.9, 10.0.0.5"));
        assert_eq!(extract_client_ip(&parts), "203.0.113.9");
    }

    #[test]
    fn client_ip_falls_back_to_the_peer_when_forwarded_for_is_entirely_private() {
        let parts = parts_from_trusted_peer("127.0.0.1:1234", Some("10.0.0.1, 10.0.0.2"));
        assert_eq!(extract_client_ip(&parts), "127.0.0.1");
    }

    #[test]
    fn client_ip_from_an_untrusted_peer_ignores_forwarded_for_entirely() {
        // No reverse proxy in front — the header is whatever the direct, untrusted caller sent.
        let parts = parts_from_trusted_peer("203.0.113.1:1234", Some("1.2.3.4"));
        assert_eq!(extract_client_ip(&parts), "203.0.113.1");
    }
}
