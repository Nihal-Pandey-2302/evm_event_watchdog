use serde::Serialize;
use reqwest::Client;
use tracing::{info, error, warn};
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::time::{Instant, Duration};

use crate::events::Severity;

#[derive(Debug, Serialize)]
struct DiscordEmbed {
    title: String,
    description: String,
    color: u32,
    fields: Vec<EmbedField>,
}

#[derive(Debug, Serialize)]
struct EmbedField {
    name: String,
    value: String,
    inline: bool,
}

#[derive(Debug, Serialize)]
struct DiscordPayload {
    content: Option<String>,
    embeds: Vec<DiscordEmbed>,
}

use crate::config::AlertsConfig;

#[derive(Debug, Serialize)]
struct TelegramPayload {
    chat_id: String,
    text: String,
    parse_mode: String,
}

pub struct AlertManager {
    client: Client,
    config: AlertsConfig,
    last_alerts: Mutex<HashMap<String, Instant>>,
    cooldown: Duration,
}

impl AlertManager {
    pub fn new(config: AlertsConfig) -> Self {
        Self {
            client: Client::new(),
            config,
            last_alerts: Mutex::new(HashMap::new()),
            cooldown: Duration::from_secs(60),
        }
    }

    pub async fn send_alert(&self, severity: Severity, message: String) {
        // Rate Limit Key: Severity + Message
        let key = format!("{:?}:{}", severity, message);
        
        let mut history = self.last_alerts.lock().await;
        if let Some(last_time) = history.get(&key) {
            if last_time.elapsed() < self.cooldown {
                warn!("Alert suppressed (Rate Limit): {}", message);
                return;
            }
        }
        history.insert(key, Instant::now());

        info!("Sending Alert: [{:?}] {}", severity, message);
        
        // Dispatch to all configured providers
        self.send_discord_alert(&severity, &message).await;
        self.send_telegram_alert(&severity, &message).await;
    }

    async fn send_discord_alert(&self, severity: &Severity, message: &str) {
        if self.config.webhook_url.is_empty() { return; }

        let color = match severity {
            Severity::Critical => 0xFF0000,
            Severity::High => 0xE67E22,
            Severity::Medium => 0xF1C40F,
            Severity::Low => 0x3498DB,
        };

        let embed = DiscordEmbed {
            title: format!("🚨 EVM Watchdog Alert: {:?}", severity),
            description: message.to_string(),
            color,
            fields: vec![
                EmbedField { name: "Severity".to_string(), value: format!("{:?}", severity), inline: true },
                EmbedField { name: "Timestamp".to_string(), value: format!("{:?}", Instant::now()), inline: true },
            ],
        };

        let payload = DiscordPayload {
            content: None,
            embeds: vec![embed],
        };

        if let Err(e) = self.client.post(&self.config.webhook_url).json(&payload).send().await {
            error!("Failed to send Discord alert: {}", e);
        } else {
             info!("Discord Alert Sent");
        }
    }

    async fn send_telegram_alert(&self, severity: &Severity, message: &str) {
        let token = match &self.config.telegram_bot_token {
            Some(t) if !t.is_empty() => t,
            _ => return,
        };
        let chat_id = match &self.config.telegram_chat_id {
            Some(id) if !id.is_empty() => id,
            _ => return,
        };

        let telegram_msg = format!("🚨 *EVM Watchdog Alert* 🚨\n\n*Severity:* {:?}\n*Message:* {}\n*Time:* {:?}", severity, message, Instant::now());
        
        let payload = TelegramPayload {
            chat_id: chat_id.clone(),
            text: telegram_msg,
            parse_mode: "Markdown".to_string(),
        };

        let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
        
        if let Err(e) = self.client.post(&url).json(&payload).send().await {
            error!("Failed to send Telegram alert: {}", e);
        } else {
             info!("Telegram Alert Sent");
        }
    }
}
