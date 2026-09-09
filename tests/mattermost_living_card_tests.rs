// tests/mattermost_living_card_tests.rs
use conduit::config::MattermostConfig;
use conduit::notify::mattermost::{dispatch_or_update_mattermost_card, MattermostAttachment, MattermostPayload};

#[test]
fn test_mattermost_payload_and_attachment_structure() {
    let attachment = MattermostAttachment {
        color: Some("#10B981".to_string()),
        title: Some("🐕🦴 Conduit Sniffed TV Series • Severance (2024)".to_string()),
        title_link: None,
        text: "Mark leads a team of office workers.".to_string(),
        thumb_url: Some("https://example.com/severance.jpg".to_string()),
        image_url: None,
        footer: Some("🐾 Conduit • Living Pipeline".to_string()),
        fields: vec![],
        fallback: Some("Severance (2024)".to_string()),
    };

    let payload = MattermostPayload {
        text: "**Severance**\nMark leads a team of office workers.".to_string(),
        username: "Conduit".to_string(),
        channel: None,
        icon_url: None,
        attachments: vec![attachment],
    };

    let json_str = serde_json::to_string(&payload).expect("Failed to serialize payload");
    assert!(json_str.contains("Conduit • Living Pipeline"));
    assert!(json_str.contains("#10B981"));
}

#[tokio::test]
async fn test_mattermost_disabled_returns_none() {
    let cfg = MattermostConfig {
        enabled: false,
        webhook_url: "".to_string(),
        bot_name: "Conduit".to_string(),
        channel: None,
        icon_url: None,
        server_url: Some("https://mattermost.example.com".to_string()),
        bot_token: Some("1f85xcsdibri9qks7sczk76nxc".to_string()),
        channel_id: Some("chan123".to_string()),
        use_living_cards: true,
        use_threading: true,
    };

    let res = dispatch_or_update_mattermost_card(
        &cfg,
        None,
        None,
        "Test Title",
        "Test Overview",
        None,
        vec![("Status", "Fetching", true)],
        Some("#10B981"),
    ).await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), None);
}
