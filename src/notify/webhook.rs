// src/notify/webhook.rs
use crate::config::{GenericWebhookConfig, PayloadMode};
use chrono::Utc;
use serde_json::json;
use tracing::{debug, error};

/// `fields` is only present on the `dispatch_rich*` paths (`None` from plain `dispatch`) —
/// `Full` payload mode includes them when available, `Summary` mode ignores them either way.
pub async fn send_generic_webhook(
    config: &GenericWebhookConfig,
    event_type: &str,
    title: &str,
    message: &str,
    fields: Option<&[(&str, &str, bool)]>,
) -> anyhow::Result<()> {
    if !config.enabled || config.url.is_empty() {
        return Ok(());
    }

    let payload = if config.payload_mode == PayloadMode::Full {
        json!({
            "event": event_type,
            "title": title,
            "message": message,
            "timestamp": Utc::now().to_rfc3339(),
            "fields": fields.unwrap_or_default().iter().map(|(name, value, inline)| json!({
                "name": name, "value": value, "inline": inline,
            })).collect::<Vec<_>>(),
        })
    } else {
        json!({
            "event": event_type,
            "title": title,
            "message": message,
            "timestamp": Utc::now().to_rfc3339(),
        })
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    let mut req = client.post(&config.url).json(&payload);

    if let Some(ref secret) = config.secret {
        req = req.header("X-Conduit-Secret", secret);
    }

    let resp = req.send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        error!("Generic webhook failed (HTTP {}): {}", status, body);
        return Err(anyhow::anyhow!("Webhook error HTTP {}: {}", status, body));
    }

    debug!("Generic webhook sent successfully to {}", config.url);
    Ok(())
}
