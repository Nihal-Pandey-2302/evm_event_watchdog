mod events;
mod listener;
mod rules;
mod alerts;
mod config;
mod tui;
mod state;

use dotenv::dotenv;
use eyre::Result;
use tracing::{info, error, warn};
use std::sync::Arc;
use tokio::sync::mpsc;
use alloy::primitives::{Address, U256};
use crate::config::AppConfig;
use crate::events::Severity;

use crate::listener::{connect, watch_blocks, watch_logs};
use crate::state::AppState;
use std::time::Duration;
use crate::rules::{RuleEngine, ThresholdRule, OwnershipRule, HighApprovalRule};
use crate::alerts::AlertManager;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    // File Logging Setup (Critical for TUI)
    let file_appender = tracing_appender::rolling::daily("logs", "watchdog.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false) // Clean text for file
        .init();

    info!("Starting EVM Event Watchdog - TUI Mode");
    
    // 1. Load Config
    let mut config = AppConfig::new().expect("Failed to load config");
    
    // Allow .env override for RPC_URL (Legacy support / easy setup)
    if let Ok(env_rpc) = std::env::var("RPC_URL") {
        if let Some(chain_cfg) = config.chains.get_mut("ethereum") {
            chain_cfg.rpc_url = env_rpc;
        }
    }
    
    // Use the first chain for MVP
    let chain_name = "ethereum";
    let chain_config = config.chains.get(chain_name).expect("Ethereum chain config missing");
    
    // Use the first contract
    let contract_config = config.contracts.first().expect("No contracts configured");

    info!("Configuration Loaded:");
    info!("  RPC URL: [HIDDEN]");
    info!("  Contract: {} ({})", contract_config.name, contract_config.address);
    info!("  Webhook: {}", if config.alerts.webhook_url.is_empty() { "Disabled" } else { "Enabled" });

    // 2. Setup Components
    let provider = Arc::new(connect(&chain_config.rpc_url).await?);
    let alert_manager =  Arc::new(AlertManager::new(config.alerts.webhook_url));
    let state = Arc::new(AppState::new());
    
    let mut engine = RuleEngine::new();

    // Configure Rules from Config
    let transfer_severity = match config.rules.transfer_threshold.severity.as_str() {
        "High" => Severity::High,
        "Critical" => Severity::Critical,
        "Medium" => Severity::Medium,
        _ => Severity::Low,
    };
    
    let ownership_severity = match config.rules.ownership_change.severity.as_str() {
        "High" => Severity::High,
        "Critical" => Severity::Critical,
        "Medium" => Severity::Medium,
        _ => Severity::Low,
    };
    let min_value: U256 = config.rules.transfer_threshold.min_value.parse().unwrap_or(U256::from(1000));

    engine.add_rule(Box::new(ThresholdRule::new(min_value, transfer_severity)));
    
    if config.rules.ownership_change.enabled {
        engine.add_rule(Box::new(OwnershipRule::new(ownership_severity)));
    }

    // High Approval Rule (Infinite Allowance Detection)
    engine.add_rule(Box::new(HighApprovalRule::new(
        U256::MAX >> 1, // > 50% of uint256 max
        Severity::Critical, 
    )));
    let engine = Arc::new(engine);

    // 3. Spawn Tasks with Backpressure
    let (tx, mut rx) = mpsc::channel(100);

    // Task A: Block Listener (Background)
    let provider_clone = provider.clone();
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = watch_blocks(provider_clone, state_clone).await {
            error!("Block listener failed: {}", e);
        }
    });

    // Task B: Log Listener (Producer)
    let provider_clone = provider.clone();
    let tx_clone = tx.clone();
    let address = contract_config.address;
    
    tokio::spawn(async move {
        if let Err(e) = watch_logs(provider_clone, address, tx_clone).await {
            error!("Log listener failed: {}", e);
        }
    });

    // Task C: Orchestrator (Consumer - now Background)
    info!("Watchdog Active. Waiting for events...");
    let state_consumer = state.clone();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            info!("Processing event: {:?}", event.event_type);
            
            let alerts = engine.process(&event);
            for (msg, severity) in alerts {
                info!("RISK LEVEL {:?}: {}", severity, msg);
                
                // Record state
                // Record state
                state_consumer.add_alert(severity.clone(), msg.clone());
                
                let alert_msg = format!("[{:?}] {}", severity, msg);
                alert_manager.send_alert(alert_msg).await;
            }
        }
    });
    
    // Task D: TUI (Main Thread)
    // Runs blocking on main thread
    if let Err(e) = crate::tui::run_tui(state) {
        eprintln!("TUI Error: {}", e);
    }

    Ok(())
}
