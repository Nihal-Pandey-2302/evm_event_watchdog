use serde::Deserialize;
use std::collections::HashMap;
use config::{Config, ConfigError, File};
use alloy::primitives::Address;

#[derive(Debug, Deserialize)]
pub struct ChainConfig {
    pub rpc_url: String,
    pub chain_id: u64,
}

#[derive(Debug, Deserialize)]
pub struct ContractConfig {
    pub name: String,
    pub address: Address,
    pub chain: String,
    pub events: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct TransferRuleConfig {
    pub min_value: String,
    pub severity: String,
}

#[derive(Debug, Deserialize)]
pub struct OwnershipRuleConfig {
    pub enabled: bool,
    pub severity: String,
}

#[derive(Debug, Deserialize)]
pub struct RulesConfig {
    pub transfer_threshold: TransferRuleConfig,
    pub ownership_change: OwnershipRuleConfig,
}

#[derive(Debug, Deserialize)]
pub struct AlertsConfig {
    pub webhook_url: String,
}

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub chains: HashMap<String, ChainConfig>,
    pub contracts: Vec<ContractConfig>,
    pub rules: RulesConfig,
    pub alerts: AlertsConfig,
}

impl AppConfig {
    pub fn new() -> Result<Self, ConfigError> {
        let builder = Config::builder()
            .add_source(File::with_name("config"));
        
        let cfg = builder.build()?;
        cfg.try_deserialize()
    }
}
