use serde::Serialize;
use reqwest::Client;
use tracing::{info, error, warn};
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::time::{Instant, Duration};

#[derive(Debug, Serialize)]
pub struct AlertPayload {
    pub text: String,
}

pub struct AlertManager {
    client: Client,
    webhook_url: String,
    last_alerts: Mutex<HashMap<String, Instant>>,
    cooldown: Duration,
}

impl AlertManager {
    pub fn new(webhook_url: String) -> Self {
        Self {
            client: Client::new(),
            webhook_url,
            last_alerts: Mutex::new(HashMap::new()),
            cooldown: Duration::from_secs(60), // 1 minute cooldown per unique alert message
        }
    }

    pub async fn send_alert(&self, message: String) {
        // Rate Limiting Check
        let mut history = self.last_alerts.lock().await;
        if let Some(last_time) = history.get(&message) {
            if last_time.elapsed() < self.cooldown {
                warn!("Alert suppressed (Rate Limit): {}", message);
                return;
            }
        }
        history.insert(message.clone(), Instant::now());

        // Send Alert
        info!("Sending Alert: {}", message);
        if self.webhook_url.is_empty() {
             info!("Mock Alert Sent: {}", message);
             return;
        }

        let payload = AlertPayload { text: message };
        match self.client.post(&self.webhook_url).json(&payload).send().await {
            Ok(_) => info!("Alert sent successfully"),
            Err(e) => error!("Failed to send alert: {}", e),
        }
    }
}
