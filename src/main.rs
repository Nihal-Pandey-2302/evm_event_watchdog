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
    
    // Parse args immediately
    let args: Vec<String> = std::env::args().collect();
    
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
    


    info!("Configuration Loaded.");
    info!("  Discord Webhook: {}", if config.alerts.webhook_url.is_empty() { "Disabled" } else { "Enabled" });
    info!("  Telegram Bot: {}", if config.alerts.telegram_bot_token.is_some() { "Enabled" } else { "Disabled" });

    // 2. Setup Components
    let alert_manager =  Arc::new(AlertManager::new(config.alerts));
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

    // Interactive Chain Selection
    // Skip if --simulate is passed (automated demo)
    if !args.contains(&"--simulate".to_string()) && !config.chains.is_empty() {
        let mut chain_names: Vec<String> = config.chains.keys().cloned().collect();
        chain_names.sort();

        println!("\n🌍 Select Chain to Monitor:");
        for (i, name) in chain_names.iter().enumerate() {
            println!("  {}. {}", i + 1, name);
        }
        println!("  {}. Monitor All", chain_names.len() + 1);

        print!("\n> Enter selection [1-{}]: ", chain_names.len() + 1);
        use std::io::{self, Write};
        io::stdout().flush().ok();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            if let Ok(choice) = input.trim().parse::<usize>() {
                if choice > 0 && choice <= chain_names.len() {
                    let selected = &chain_names[choice - 1];
                    info!("User selected: {}", selected);
                    println!("🚀 Starting Watchdog for: {}\n", selected);
                    
                    // Filter config to keep only selected
                    config.chains.retain(|k, _| k == selected);
                } else if choice == chain_names.len() + 1 {
                    println!("🚀 Starting Watchdog for: ALL CHAINS\n");
                } else {
                    println!("Invalid selection, defaulting to ALL.");
                }
            } else {
                println!("Invalid input, defaulting to ALL.");
            }
        }
        // Small delay to let user read
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    // Multi-Chain Loop
    for (chain_name, chain_cfg) in &config.chains {
        info!("Initializing Chain: {}", chain_name);
        
        let provider = match connect(&chain_cfg.rpc_url).await {
            Ok(p) => Arc::new(p),
            Err(e) => {
                error!("Failed to connect to {}: {}", chain_name, e);
                continue;
            }
        };

        // Task A: Block Listener (Per Chain)
        let provider_blocks = provider.clone();
        let state_clone = state.clone();
        let c_name = chain_name.clone();
        tokio::spawn(async move {
            if let Err(e) = watch_blocks(provider_blocks, state_clone, c_name).await {
                error!("Block listener failed: {}", e);
            }
        });

        // Task B: Log Listener (Per Contract on this Chain)
        for contract in &config.contracts {
            if contract.chain == *chain_name {
                info!("  Watching Contract: {} on {}", contract.name, chain_name);
                let provider_logs = provider.clone();
                let tx_clone = tx.clone();
                let address = contract.address;
                let c_id = chain_cfg.chain_id;
                let c_name_log = chain_name.clone();
                
                tokio::spawn(async move {
                    if let Err(e) = watch_logs(provider_logs, address, c_id, c_name_log, tx_clone).await {
                        error!("Log listener failed: {}", e);
                    }
                });
            }
        }
    }

    // 4. Simulation Mode (Chaos Monkey for Demo)
    if args.contains(&"--simulate".to_string()) {
        info!("🚀 SIMULATION MODE ACTIVE: Chaos Monkey Enabled 🚀");
        let tx_sim = tx.clone();
        
        tokio::spawn(async move {
            use rand::Rng; // trait for random_range
            
            loop {
                // Scoped RNG usage to avoid holding !Send across await
                let delay = rand::rng().random_range(100..800);
                tokio::time::sleep(Duration::from_millis(delay)).await;
                
                // Randomly generate an event
                let event_type_idx = rand::rng().random_range(0..3);
                let event_type = match event_type_idx {
                    0 => crate::events::EventType::Transfer,
                    1 => crate::events::EventType::Approval,
                    _ => crate::events::EventType::OwnershipTransferred,
                };
                
                // Demo Values
                let use_high = rand::rng().random_bool(0.3);
                let val: u64 = if use_high {
                    rand::rng().random_range(1_000_000_000..50_000_000_000) 
                } else {
                    rand::rng().random_range(100..900)
                };
                
                let mock_event = crate::events::NormalizedEvent {
                    chain_id: 1,
                    chain_name: "Simulation".to_string(),
                    contract_address: Address::ZERO,
                    tx_hash: Default::default(),
                    block_number: 1000,
                    event_type,
                    severity: Severity::Low,
                    data: serde_json::json!({
                        "value": val.to_string(),
                        "from": "0x000000000000000000000000000000000000dead",
                        "to": "0x000000000000000000000000000000000000beef",
                    }),
                };
                
                if let Err(e) = tx_sim.send(mock_event).await {
                     error!("Simulation failed: {}", e);
                }
            }
        });
    }

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
                state_consumer.add_alert(severity.clone(), event.chain_name.clone(), msg.clone());
                
                alert_manager.send_alert(severity, msg).await;
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
