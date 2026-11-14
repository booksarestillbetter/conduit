// src/auth/oidc.rs
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use parking_lot::Mutex;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::config::model::OidcConfig;

const OIDC_STATE_TTL: Duration = Duration::from_secs(600);
const MAX_PENDING_STATES: usize = 512;

#[derive(Debug, Clone)]
pub struct OidcPendingAuth {
    pub code_verifier: String,
    pub nonce: String,
    pub created_at: Instant,
}

#[derive(Clone, Default)]
pub struct OidcStateStore {
    inner: Arc<Mutex<HashMap<String, OidcPendingAuth>>>,
}

impl OidcStateStore {
    pub fn insert(&self, state: String, pending: OidcPendingAuth) {
        let mut guard = self.inner.lock();
        let now = Instant::now();
        guard.retain(|_, v| now.duration_since(v.created_at) < OIDC_STATE_TTL);
        if guard.len() >= MAX_PENDING_STATES {
            if let Some(oldest_key) = guard
                .iter()
                .min_by_key(|(_, v)| v.created_at)
                .map(|(k, _)| k.clone())
            {
                guard.remove(&oldest_key);
            }
        }
        guard.insert(state, pending);
    }

    pub fn take(&self, state: &str) -> Option<OidcPendingAuth> {
        let mut guard = self.inner.lock();
        let pending = guard.remove(state)?;
        if Instant::now().duration_since(pending.created_at) >= OIDC_STATE_TTL {
            return None;
        }
        Some(pending)
    }
}

#[derive(Debug, Deserialize)]
pub struct OidcDiscoveryDoc {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OidcTokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct OidcUserClaims {
    #[serde(default)]
    pub sub: Option<String>,
    #[serde(default)]
    pub preferred_username: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub nonce: Option<String>,
    #[serde(default)]
    pub groups: Vec<String>,
}

pub fn random_urlsafe_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(&bytes)
}

pub fn generate_pkce_pair() -> (String, String) {
    let verifier = random_urlsafe_token(32);
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

pub async fn discover_endpoints(
    client: &reqwest::Client,
    issuer_url: &str,
) -> anyhow::Result<OidcDiscoveryDoc> {
    let base = issuer_url.trim().trim_end_matches('/');
    if !base.starts_with("http://") && !base.starts_with("https://") {
        anyhow::bail!("OIDC issuer_url must start with http:// or https://");
    }
    let url = format!("{}/.well-known/openid-configuration", base);
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("OIDC discovery returned HTTP {}", resp.status());
    }
    let doc: OidcDiscoveryDoc = resp.json().await?;
    Ok(doc)
}

fn decode_jwt_payload_claims(jwt: &str) -> Option<OidcUserClaims> {
    let payload_b64 = jwt.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload_b64.as_bytes()).ok()?;
    serde_json::from_slice::<OidcUserClaims>(&decoded).ok()
}

pub async fn exchange_code_for_user(
    client: &reqwest::Client,
    cfg: &OidcConfig,
    doc: &OidcDiscoveryDoc,
    code: &str,
    code_verifier: &str,
    expected_nonce: &str,
    redirect_uri: &str,
) -> anyhow::Result<(String, String)> {
    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", cfg.client_id.as_str()),
        ("code_verifier", code_verifier),
    ];
    if !cfg.client_secret.is_empty() {
        form.push(("client_secret", cfg.client_secret.as_str()));
    }

    let token_resp = client.post(&doc.token_endpoint).form(&form).send().await?;
    if !token_resp.status().is_success() {
        let status = token_resp.status();
        let body = token_resp.text().await.unwrap_or_default();
        anyhow::bail!("OIDC token exchange failed ({}): {}", status, body);
    }
    let tokens: OidcTokenResponse = token_resp.json().await?;

    let mut claims = OidcUserClaims::default();
    if let Some(ref id_tok) = tokens.id_token {
        if let Some(id_claims) = decode_jwt_payload_claims(id_tok) {
            if let Some(ref nonce) = id_claims.nonce {
                if nonce != expected_nonce {
                    anyhow::bail!("OIDC ID token nonce mismatch");
                }
            }
            claims = id_claims;
        }
    }

    if let (Some(ref userinfo_url), Some(ref access_token)) =
        (&doc.userinfo_endpoint, &tokens.access_token)
    {
        if let Ok(ui_resp) = client
            .get(userinfo_url)
            .bearer_auth(access_token)
            .send()
            .await
        {
            if ui_resp.status().is_success() {
                if let Ok(ui_claims) = ui_resp.json::<OidcUserClaims>().await {
                    if claims.preferred_username.is_none() {
                        claims.preferred_username = ui_claims.preferred_username;
                    }
                    if claims.email.is_none() {
                        claims.email = ui_claims.email;
                    }
                    if claims.name.is_none() {
                        claims.name = ui_claims.name;
                    }
                    if claims.sub.is_none() {
                        claims.sub = ui_claims.sub;
                    }
                    if claims.groups.is_empty() {
                        claims.groups = ui_claims.groups;
                    }
                }
            }
        }
    }

    let username = claims
        .preferred_username
        .or(claims.email)
        .or(claims.name)
        .or(claims.sub)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("OIDC response did not include a username, email, or sub claim"))?;

    let is_admin_group = !cfg.admin_groups.is_empty()
        && claims
            .groups
            .iter()
            .any(|g| cfg.admin_groups.iter().any(|ag| ag == g));
    let role = if is_admin_group {
        "admin".to_string()
    } else {
        crate::db::normalize_role(&cfg.default_role).to_string()
    };

    Ok((username, role))
}
