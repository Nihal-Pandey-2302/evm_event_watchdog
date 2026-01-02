use alloy::providers::{Provider, ProviderBuilder, RootProvider, WsConnect};
use alloy::pubsub::PubSubFrontend;
use alloy::rpc::types::{BlockNumberOrTag, Filter, Log};
use alloy::sol_types::{SolEvent, SolInterface};
use alloy::primitives::Address;
use eyre::Result;
use futures_util::StreamExt;
use url::Url;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::{error, info};

use crate::events::{NormalizedEvent, EventType, OwnershipTransferred, Transfer, Approval};

pub type WsProvider = RootProvider<PubSubFrontend>;

pub async fn connect(rpc_url: &str) -> Result<WsProvider> {
    let url = Url::parse(rpc_url)?;
    let ws = WsConnect::new(url);
    let provider = ProviderBuilder::new().on_ws(ws).await?;
    Ok(provider)
}

use crate::state::AppState;

pub async fn watch_blocks(provider: Arc<WsProvider>, state: Arc<AppState>) -> Result<()> {
    let sub = provider.subscribe_blocks().await?;
    let mut stream = sub.into_stream();

    while let Some(block) = stream.next().await {
        let number = block.header.number.unwrap_or_default();
        state.update_block(number);
        info!("New Block: {:?}", number);
    }

    Ok(())
}

pub async fn watch_logs(
    provider: Arc<WsProvider>,
    address: Address,
    tx: Sender<NormalizedEvent>,
) -> Result<()> {
    let filter = Filter::new()
        .address(address)
        .from_block(BlockNumberOrTag::Latest);

    let sub = provider.subscribe_logs(&filter).await?;
    let mut stream = sub.into_stream();

     while let Some(log) = stream.next().await {
         let signature = log.topics().first().copied();

         if let Some(sig) = signature {
              if sig == OwnershipTransferred::SIGNATURE_HASH {
                  if let Ok(decoded) = OwnershipTransferred::decode_log(&log.inner, true) {
                      info!("Detected OwnershipTransferred: {:?}", decoded);
                      let event = NormalizedEvent {
                          chain_id: 1, // TODO: parameterized
                          contract_address: log.address(),
                          tx_hash: log.transaction_hash.unwrap_or_default(),
                          block_number: log.block_number.unwrap_or_default(),
                          event_type: EventType::OwnershipTransferred,
                          severity: crate::events::Severity::Low, // Default, upgraded by rules
                          data: serde_json::to_value(&decoded).unwrap_or_default(),
                      };
                      if let Err(e) = tx.send(event).await {
                          error!("Failed to send event: {}", e);
                      }
                  }
              } else if sig == Transfer::SIGNATURE_HASH {
                  if let Ok(decoded) = Transfer::decode_log(&log.inner, true) {
                      info!("Detected Transfer: {:?}", decoded);
                      let event = NormalizedEvent {
                          chain_id: 1, 
                          contract_address: log.address(),
                          tx_hash: log.transaction_hash.unwrap_or_default(),
                          block_number: log.block_number.unwrap_or_default(),
                          event_type: EventType::Transfer,
                          severity: crate::events::Severity::Low,
                          data: serde_json::to_value(&decoded).unwrap_or_default(),
                      };
                      if let Err(e) = tx.send(event).await {
                          error!("Failed to send event: {}", e);
                      }
                  }
              } else if sig == Approval::SIGNATURE_HASH {
                  if let Ok(decoded) = Approval::decode_log(&log.inner, true) {
                      let event = NormalizedEvent {
                          chain_id: 1,
                          contract_address: log.address(),
                          tx_hash: log.transaction_hash.unwrap_or_default(),
                          block_number: log.block_number.unwrap_or_default(),
                          event_type: EventType::Approval,
                          severity: crate::events::Severity::Low,
                          data: serde_json::to_value(&decoded).unwrap_or_default(),
                      };
                      if let Err(e) = tx.send(event).await {
                          error!("Failed to send event: {}", e);
                      }
                  }
              } else {
                  tracing::debug!("Unknown event signature: {:?}", sig);
              }
         }
     }

    Ok(())
}
