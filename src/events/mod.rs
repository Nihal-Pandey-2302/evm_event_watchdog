use alloy::primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};
use alloy::sol;

sol! {
    #[derive(Debug, Serialize, Deserialize)]
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    #[derive(Debug, Serialize, Deserialize)]
    event Transfer(address indexed from, address indexed to, uint256 value);

    #[derive(Debug, Serialize, Deserialize)]
    event Approval(address indexed owner, address indexed spender, uint256 value);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    Transfer,
    OwnershipTransferred,
    Approval,
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedEvent {
    pub chain_id: u64,
    pub chain_name: String,
    pub contract_address: Address,
    pub tx_hash: B256,
    pub block_number: u64,
    pub event_type: EventType,
    pub severity: Severity,
    pub data: serde_json::Value, // Flexible payload for rule engine
}
