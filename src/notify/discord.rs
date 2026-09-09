// src/notify/discord.rs
use crate::config::DiscordConfig;
use serde::Serialize;
use tracing::{debug, error};

#[derive(Debug, Serialize, Clone)]
pub struct DiscordField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct DiscordImage {
    pub url: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct DiscordFooter {
    pub text: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct DiscordEmbed {
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<DiscordImage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<DiscordImage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<DiscordField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<DiscordFooter>,
}

#[derive(Debug, Serialize)]
pub struct DiscordPayload {
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub embeds: Vec<DiscordEmbed>,
}

pub async fn send_discord_notification(
    config: &DiscordConfig,
    title: &str,
    message: &str,
    color_hex: Option<&str>,
) -> anyhow::Result<()> {
    if !config.enabled || config.webhook_url.is_empty() {
        return Ok(());
    }

    let color_num = color_hex.and_then(|c| {
        let clean = c.trim_start_matches('#');
        u32::from_str_radix(clean, 16).ok()
    }).unwrap_or(0x3B82F6);

    let payload = DiscordPayload {
        username: config.username.clone(),
        avatar_url: config.avatar_url.clone(),
        embeds: vec![DiscordEmbed {
            title: title.to_string(),
            description: message.to_string(),
            color: Some(color_num),
            thumbnail: None,
            image: None,
            fields: Vec::new(),
            footer: Some(DiscordFooter {
                text: "🐾 Conduit • Living Pipeline".to_string(),
            }),
        }],
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    let resp = client.post(&config.webhook_url).json(&payload).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        error!("Discord notification failed (HTTP {}): {}", status, body);
        return Err(anyhow::anyhow!("Discord error HTTP {}: {}", status, body));
    }

    debug!("Discord notification sent successfully: {}", title);
    Ok(())
}

pub async fn send_discord_rich_notification(
    config: &DiscordConfig,
    title: &str,
    overview: &str,
    poster_url: Option<&str>,
    fields: Vec<(&str, &str, bool)>,
    color_hex: Option<&str>,
) -> anyhow::Result<()> {
    if !config.enabled || config.webhook_url.is_empty() {
        return Ok(());
    }

    let color_num = color_hex.and_then(|c| {
        let clean = c.trim_start_matches('#');
        u32::from_str_radix(clean, 16).ok()
    }).unwrap_or(0x10B981);

    let dc_fields: Vec<DiscordField> = fields
        .into_iter()
        .map(|(n, v, i)| DiscordField {
            name: n.to_string(),
            value: v.to_string(),
            inline: i,
        })
        .collect();

    let payload = DiscordPayload {
        username: config.username.clone(),
        avatar_url: config.avatar_url.clone(),
        embeds: vec![DiscordEmbed {
            title: title.to_string(),
            description: overview.to_string(),
            color: Some(color_num),
            thumbnail: poster_url.map(|u| DiscordImage { url: u.to_string() }),
            image: None,
            fields: dc_fields,
            footer: Some(DiscordFooter {
                text: "🐾 Conduit • Living Pipeline".to_string(),
            }),
        }],
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    let resp = client.post(&config.webhook_url).json(&payload).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        error!("Discord rich notification failed (HTTP {}): {}", status, body);
        return Err(anyhow::anyhow!("Discord rich error HTTP {}: {}", status, body));
    }

    debug!("Discord rich notification sent successfully: {}", title);
    Ok(())
}

