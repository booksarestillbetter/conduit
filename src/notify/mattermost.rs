// src/notify/mattermost.rs
use crate::config::MattermostConfig;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info, warn};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MattermostField {
    pub title: String,
    pub value: String,
    pub short: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MattermostAttachment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_link: Option<String>,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<MattermostField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MattermostPayload {
    pub text: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<MattermostAttachment>,
}

#[derive(Debug, Deserialize)]
struct MattermostPostResponse {
    pub id: String,
}

/// Dispatches or updates a rich media card using either Mattermost Bot REST API v4 (with in-place edits and threading) or Webhook fallback.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_or_update_mattermost_card(
    config: &MattermostConfig,
    existing_post_id: Option<&str>,
    root_id: Option<&str>,
    title: &str,
    overview: &str,
    poster_url: Option<&str>,
    fields: Vec<(&str, &str, bool)>,
    color: Option<&str>,
) -> anyhow::Result<Option<String>> {
    if !config.enabled {
        return Ok(None);
    }

    let bot_token = config.bot_token.as_deref().unwrap_or("").trim();
    let server_url = config.server_url.as_deref().unwrap_or("").trim().trim_end_matches('/');
    let channel_id = config.channel_id.as_deref().unwrap_or("").trim();

    // 1. If Bot Token is configured with Server URL & Channel ID, use REST API v4
    if !bot_token.is_empty() && !server_url.is_empty() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();
        let mm_fields: Vec<MattermostField> = fields
            .clone()
            .into_iter()
            .map(|(t, v, s)| MattermostField {
                title: t.to_string(),
                value: v.to_string(),
                short: s,
            })
            .collect();

        let attachment = MattermostAttachment {
            color: color.map(|c| c.to_string()).or_else(|| Some("#10B981".to_string())),
            title: Some(title.to_string()),
            title_link: None,
            text: overview.to_string(),
            thumb_url: poster_url.map(|u| u.to_string()),
            image_url: None,
            footer: Some("Conduit • Living Pipeline".to_string()),
            fields: mm_fields,
            fallback: Some(format!("{}: {}", title, overview)),
        };

        // 1a. In-Place Update of Existing Card (if living cards enabled and post_id present)
        if config.use_living_cards {
            if let Some(post_id) = existing_post_id {
                if !post_id.trim().is_empty() {
                    let update_url = format!("{}/api/v4/posts/{}", server_url, post_id.trim());
                    let update_payload = json!({
                        "id": post_id.trim(),
                        "message": "",
                        "props": {
                            "attachments": [attachment]
                        }
                    });

                    let resp = client
                        .put(&update_url)
                        .header("Authorization", format!("Bearer {}", bot_token))
                        .json(&update_payload)
                        .send()
                        .await?;

                    if resp.status().is_success() {
                        info!("🔄 Mattermost card updated in-place (Post ID: {}): '{}'", post_id, title);
                        return Ok(Some(post_id.to_string()));
                    } else {
                        let err_body = resp.text().await.unwrap_or_default();
                        warn!("Failed in-place edit of Mattermost post '{}' (falling back to create new): {}", post_id, err_body);
                    }
                }
            }
        }

        // 1b. Create New Post (or threaded reply if root_id specified)
        if !channel_id.is_empty() {
            let create_url = format!("{}/api/v4/posts", server_url);
            let mut post_body = json!({
                "channel_id": channel_id,
                "message": "",
                "props": {
                    "attachments": [attachment]
                }
            });

            if config.use_threading {
                if let Some(r_id) = root_id {
                    if !r_id.trim().is_empty() {
                        post_body["root_id"] = json!(r_id.trim());
                    }
                }
            }

            let resp = client
                .post(&create_url)
                .header("Authorization", format!("Bearer {}", bot_token))
                .json(&post_body)
                .send()
                .await?;

            if resp.status().is_success() {
                let post_resp: MattermostPostResponse = resp.json().await?;
                info!("📣 Mattermost Bot post created successfully (ID: {}): '{}'", post_resp.id, title);
                return Ok(Some(post_resp.id));
            } else {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                error!("❌ Mattermost REST API error HTTP {}: {}", status, err_body);
                return Err(anyhow::anyhow!("Mattermost Bot API HTTP {}: {}", status, err_body));
            }
        }
    }

    // 2. Fallback to Webhook if Bot Token is not configured
    if !config.webhook_url.trim().is_empty() {
        send_mattermost_rich_notification(config, title, overview, poster_url, fields, color).await?;
        return Ok(None);
    }

    Ok(None)
}

pub async fn send_mattermost_notification(
    config: &MattermostConfig,
    title: &str,
    message: &str,
    color: Option<&str>,
) -> anyhow::Result<()> {
    if !config.enabled {
        return Ok(());
    }

    let bot_token = config.bot_token.as_deref().unwrap_or("").trim();
    let server_url = config.server_url.as_deref().unwrap_or("").trim().trim_end_matches('/');
    let channel_id = config.channel_id.as_deref().unwrap_or("").trim();

    if !bot_token.is_empty() && !server_url.is_empty() && !channel_id.is_empty() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();
        let payload = json!({
            "channel_id": channel_id,
            "message": "",
            "props": {
                "attachments": [{
                    "color": color.unwrap_or("#10B981"),
                    "title": title,
                    "text": message,
                    "footer": "Conduit • Pipeline"
                }]
            }
        });

        let resp = client
            .post(format!("{}/api/v4/posts", server_url))
            .header("Authorization", format!("Bearer {}", bot_token))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            error!("❌ Mattermost Bot API error HTTP {}: {}", status, body);
            return Err(anyhow::anyhow!("Mattermost Bot API HTTP {}: {}", status, body));
        }

        info!("📣 Mattermost Bot notification sent successfully: {}", title);
        return Ok(());
    }

    if config.webhook_url.trim().is_empty() {
        return Ok(());
    }

    let channel = config.channel.as_ref().filter(|c| !c.trim().is_empty()).cloned();
    let icon_url = config.icon_url.as_ref().filter(|u| !u.trim().is_empty()).cloned();

    let payload = MattermostPayload {
        text: "".to_string(),
        username: if !config.bot_name.trim().is_empty() { config.bot_name.clone() } else { "Conduit".to_string() },
        channel,
        icon_url,
        attachments: vec![MattermostAttachment {
            color: color.map(|c| c.to_string()).or_else(|| Some("#10B981".to_string())),
            title: Some(title.to_string()),
            title_link: None,
            text: message.to_string(),
            thumb_url: None,
            image_url: None,
            footer: Some("Conduit • Pipeline".to_string()),
            fields: Vec::new(),
            fallback: Some(format!("{}: {}", title, message)),
        }],
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    let resp = client.post(config.webhook_url.trim()).json(&payload).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        error!("❌ Mattermost notification failed (HTTP {}): {}", status, body);
        return Err(anyhow::anyhow!("Mattermost error HTTP {}: {}", status, body));
    }

    info!("📣 Mattermost notification sent successfully: {}", title);
    Ok(())
}

pub async fn send_mattermost_rich_notification(
    config: &MattermostConfig,
    title: &str,
    overview: &str,
    poster_url: Option<&str>,
    fields: Vec<(&str, &str, bool)>,
    color: Option<&str>,
) -> anyhow::Result<()> {
    if !config.enabled || config.webhook_url.trim().is_empty() {
        return Ok(());
    }

    let mm_fields: Vec<MattermostField> = fields
        .into_iter()
        .map(|(t, v, s)| MattermostField {
            title: t.to_string(),
            value: v.to_string(),
            short: s,
        })
        .collect();

    let channel = config.channel.as_ref().filter(|c| !c.trim().is_empty()).cloned();
    let icon_url = config.icon_url.as_ref().filter(|u| !u.trim().is_empty()).cloned();

    let payload = MattermostPayload {
        text: "".to_string(),
        username: if !config.bot_name.trim().is_empty() { config.bot_name.clone() } else { "Conduit".to_string() },
        channel,
        icon_url,
        attachments: vec![MattermostAttachment {
            color: color.map(|c| c.to_string()).or_else(|| Some("#10B981".to_string())),
            title: Some(title.to_string()),
            title_link: None,
            text: overview.to_string(),
            thumb_url: poster_url.map(|u| u.to_string()),
            image_url: None,
            footer: Some("Conduit • Pipeline".to_string()),
            fields: mm_fields,
            fallback: Some(format!("{}: {}", title, overview)),
        }],
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    let resp = client.post(config.webhook_url.trim()).json(&payload).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        error!("❌ Mattermost rich notification failed (HTTP {}): {}", status, body);
        return Err(anyhow::anyhow!("Mattermost rich error HTTP {}: {}", status, body));
    }

    info!("📣 Mattermost rich notification sent successfully: {}", title);
    Ok(())
}

