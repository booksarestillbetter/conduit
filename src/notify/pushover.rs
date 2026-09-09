// src/notify/pushover.rs
use crate::config::PushoverConfig;
use tracing::{debug, error};

pub async fn send_pushover_notification(
    config: &PushoverConfig,
    title: &str,
    message: &str,
) -> anyhow::Result<()> {
    if config.user_key.is_empty() || config.api_token.is_empty() {
        return Ok(());
    }

    let mut form: Vec<(&str, String)> = vec![
        ("token", config.api_token.clone()),
        ("user", config.user_key.clone()),
        ("title", title.to_string()),
        ("message", message.to_string()),
    ];
    if let Some(ref device) = config.device {
        if !device.is_empty() {
            form.push(("device", device.clone()));
        }
    }
    if let Some(priority) = config.priority {
        form.push(("priority", priority.to_string()));
    }
    if let Some(ref sound) = config.sound {
        if !sound.is_empty() {
            form.push(("sound", sound.clone()));
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    let resp = client
        .post("https://api.pushover.net/1/messages.json")
        .form(&form)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        error!("Pushover notification failed (HTTP {}): {}", status, body);
        return Err(anyhow::anyhow!("Pushover error HTTP {}: {}", status, body));
    }

    debug!("Pushover notification sent successfully: {}", title);
    Ok(())
}
