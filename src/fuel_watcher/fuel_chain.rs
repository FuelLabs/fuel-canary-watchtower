use super::{FUEL_BLOCK_TIME, FUEL_CONNECTION_RETRIES};

use anyhow::Result;
use fuels::{
    client::{PageDirection, PaginationRequest},
    types::{Identity, Bits256},
    core::{
        codec::ABIDecoder,
        traits::{Parameterize, Tokenizable},
    },
    macros::{Parameterize, Tokenizable},
    tx::Bytes32,
};
use fuels::prelude::{Provider, TransactionType};
use fuels::types::chain_info::ChainInfo;
use fuels::types::tx_status::TxStatus;
use fuels::tx::Receipt;

use async_trait::async_trait;
use tokio::sync::Mutex;

use std::{sync::Arc, collections::HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use mockall::{automock, predicate::*};

#[derive(Parameterize, Tokenizable, Debug)]
pub struct WithdrawalEvent {
    amount: u64,
    from: Identity,
    to: Bits256,
}

#[async_trait]
#[cfg_attr(test, automock)] 
pub trait FuelChainTrait: Send + Sync {
    async fn check_connection(&self) -> Result<()>;
    async fn get_seconds_since_last_block(&self) -> Result<u32>;
    async fn fetch_chain_info(&self) -> Result<ChainInfo>;
    async fn get_base_amount_withdrawn(&self, timeframe: u32) -> Result<u64>;
    async fn get_base_amount_withdrawn_from_tx(&self, tx_id: &Bytes32) -> Result<u64>;
    async fn get_token_amount_withdrawn(&self, timeframe: u32, token_contract_id: &str) -> Result<u64>;
    async fn get_token_amount_withdrawn_from_tx(&self, tx_id: &Bytes32, token_contract_id: &str) -> Result<u64>;
    async fn verify_block_commit(&self, block_hash: &Bytes32) -> Result<bool>;
}

#[derive(Clone, Debug)]
pub struct FuelChain {
    provider: Arc<Provider>,
    // Nested HashMap: Asset Token Identifier -> (Timestamp -> Amount)
    asset_withdrawal_cache: Arc<Mutex<HashMap<String, HashMap<u64, u64>>>>,
}

impl FuelChain {
    pub fn new(provider: Arc<Provider>) -> Result<Self> {
        Ok(FuelChain {
            provider,
            asset_withdrawal_cache: Arc::new(Mutex::new(HashMap::new())),
         })
    }
}

#[async_trait]
impl FuelChainTrait for FuelChain {
    async fn check_connection(&self) -> Result<()> {
        for _ in 0..FUEL_CONNECTION_RETRIES {
            if self.provider.chain_info().await.is_ok() {
                return Ok(());
            }
        }
        Err(anyhow::anyhow!(
            "Failed to establish connection after {} retries", FUEL_CONNECTION_RETRIES),
        )
    }

    async fn get_seconds_since_last_block(&self) -> Result<u32> {
        let chain_info = self.fetch_chain_info().await?;

        let latest_block_time = chain_info.latest_block.header.time.ok_or_else(
            || anyhow::anyhow!("Failed to get latest block"))?;
        let last_block_timestamp = (latest_block_time.timestamp_millis() as u64) / 1000;
        let current_timestamp = (SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64) / 1000;

        if current_timestamp < last_block_timestamp {
            return Err(anyhow::anyhow!("Block time is ahead of current time"));
        }

        Ok((current_timestamp - last_block_timestamp) as u32)
    }

    async fn fetch_chain_info(&self) -> Result<ChainInfo> {
        for _ in 0..FUEL_CONNECTION_RETRIES {
            match self.provider.chain_info().await {
                Ok(info) => return Ok(info),
                _ => continue,
            }
        }
        Err(anyhow::anyhow!(
            "Failed to establish connection after {} retries", FUEL_CONNECTION_RETRIES),
        )
    }

    async fn get_base_amount_withdrawn(&self, timeframe: u32) -> Result<u64> {
        let chain_info = self.fetch_chain_info().await?;
        let current_timestamp = chain_info.latest_block.header.time
            .ok_or_else(|| anyhow::anyhow!("Failed to get current block timestamp"))?
            .timestamp() as u64;
    
        let start_timestamp = current_timestamp.saturating_sub(timeframe as u64);
        let mut cached_withdrawals = self.asset_withdrawal_cache.lock().await;
        let base_token_cache = cached_withdrawals.entry(
            String::from("base_token"),
        ).or_insert_with(HashMap::new);

        let mut total_from_cache = 0;
        let mut earliest_needed_timestamp = u64::MAX;
    
        // Check the cache for any amounts within the timeframe
        for (&timestamp, &amount) in base_token_cache.iter() {
            if timestamp >= start_timestamp {
                total_from_cache += amount;
                earliest_needed_timestamp = earliest_needed_timestamp.min(timestamp);
            }
        }

        // Release the lock
        drop(cached_withdrawals);

        // If all needed data is in the cache
        if earliest_needed_timestamp <= start_timestamp {
            return Ok(total_from_cache);
        }
    
        // Adjust timeframe to fetch only missing data
        let adjusted_timeframe = if earliest_needed_timestamp == u64::MAX {
            timeframe // Cache is empty, need to fetch for the entire timeframe
        } else {
            ((earliest_needed_timestamp - start_timestamp) / FUEL_BLOCK_TIME) as u32
        };
        let num_blocks = usize::try_from(adjusted_timeframe).map_err(|e| anyhow::anyhow!("{e}"))?;

        // Fetch and process missing blocks
        let mut total_from_blocks = 0;
        for i in 0..FUEL_CONNECTION_RETRIES {
            let req = PaginationRequest {
                cursor: None,
                results: num_blocks,
                direction: PageDirection::Backward,
            };
            match self.provider.get_blocks(req).await {
                Ok(blocks_result) => {
                    for block in blocks_result.results {
                        let mut block_total = 0;
                        for tx_id in block.transactions {
                            match self.get_base_amount_withdrawn_from_tx(&tx_id).await {
                                Ok(amount) => block_total += amount,
                                Err(e) => return Err(anyhow::anyhow!("{e}")),
                            }
                        }
                        total_from_blocks += block_total;
        
                        // Update cache with the total amount for this block
                        let block_timestamp = block.header.time.unwrap().timestamp() as u64;
                        let mut cache = self.asset_withdrawal_cache.lock().await;
                        let base_token_cache = cache.entry("base_token".to_string()).or_insert_with(HashMap::new);
                        *base_token_cache.entry(block_timestamp).or_insert(0) += block_total;
                    }
                    break;
                }
                Err(e) if i == FUEL_CONNECTION_RETRIES - 1 => return Err(anyhow::anyhow!("{e}")),
                Err(_) => continue,
            }
        }

        Ok(total_from_cache + total_from_blocks)
    }

    async fn get_base_amount_withdrawn_from_tx(&self, tx_id: &Bytes32) -> Result<u64> {

        // Query the transaction from the chain within a certain number of tries.
        let mut tx_response = None;
        let mut total_amount:u64 = 0;

        for i in 0..FUEL_CONNECTION_RETRIES {
            match self.provider.get_transaction_by_id(tx_id).await {
                Ok(Some(response)) => {
                    tx_response = Some(response);
                    break;
                }
                Ok(None) => return Ok(0), // This is a Mint Transaction that is not yet implemented.
                Err(e) if i == FUEL_CONNECTION_RETRIES - 1 => {
                    return Err(anyhow::anyhow!("{e}"));
                }
                _ => continue,
            }
        }

        // Check if the response was assigned.
        let response = match tx_response {
            Some(response) => response,
            None => return Ok(0),
        };

        // Check if the status is a success, if not we return.
        if !matches!(response.status, TxStatus::Success { .. }) {
            return Ok(0);
        }

        // Check if the transaction is of script type, if not we return.
        if !matches!(response.transaction, Some(TransactionType::Script(_))) {
            return Ok(0);
        }

        // Fetch the receipts from the transaction.
        let receipts = self.provider.tx_status(tx_id).await?.take_receipts();
        for receipt in receipts{
            if let Receipt::MessageOut { amount, .. } = receipt.clone() {
                total_amount += amount;
            }
        }

        Ok(total_amount)
    }

    async fn get_token_amount_withdrawn(
        &self, timeframe: u32, token_contract_id: &str
    ) -> Result<u64> {
        let num_blocks = match usize::try_from(timeframe as u64 / FUEL_BLOCK_TIME) {
            Ok(val) => val,
            Err(e) => return Err(anyhow::anyhow!("{e}")),
        };
        for i in 0..FUEL_CONNECTION_RETRIES {
            let req = PaginationRequest {
                cursor: None,
                results: num_blocks,
                direction: PageDirection::Backward,
            };
            match self.provider.get_blocks(req).await {
                Ok(blocks_result) => {
                    let mut total: u64 = 0;
                    for block in blocks_result.results {
                        for tx_id in block.transactions {
                            match self.get_token_amount_withdrawn_from_tx(
                                &tx_id, token_contract_id).await {
                                Ok(amount) => {
                                    total += amount;
                                }
                                Err(e) => return Err(anyhow::anyhow!("{e}")),
                            }
                        }
                    }
                    return Ok(total);
                }
                Err(e) => {
                    if i == FUEL_CONNECTION_RETRIES - 1 {
                        return Err(anyhow::anyhow!("{e}"));
                    }
                }
            }
        }
        Ok(0)
    }

    async fn get_token_amount_withdrawn_from_tx(
        &self, tx_id: &Bytes32, token_contract_id: &str,
    ) -> Result<u64> {

        // Query the transaction from the chain within a certain number of tries.
        let mut tx_response = None;
        let mut total_amount:u64 = 0;

        for i in 0..FUEL_CONNECTION_RETRIES {
            match self.provider.get_transaction_by_id(tx_id).await {
                Ok(Some(response)) => {
                    tx_response = Some(response);
                    break;
                }
                Ok(None) => return Ok(0), // This is a Mint Transaction that is not yet implemented.
                Err(e) if i == FUEL_CONNECTION_RETRIES - 1 => {
                    return Err(anyhow::anyhow!("{e}"));
                }
                _ => continue,
            }
        }

        // Check if the response was assigned.
        let response = match tx_response {
            Some(response) => response,
            None => return Ok(0),
        };

        // Check if the status is a success, if not we return.
        if !matches!(response.status, TxStatus::Success { .. }) {
            return Ok(0);
        }

        // Check if the transaction is of script type, if not we return.
        if !matches!(response.transaction, Some(TransactionType::Script(_))) {
            return Ok(0);
        }

        // Fetch the receipts from the transaction.
        let mut burn_found: bool = false;
        let receipts = self.provider.tx_status(tx_id).await?.take_receipts();
        for receipt in receipts{
            match receipt {
                Receipt::Burn { contract_id, .. } if contract_id.to_string() == token_contract_id => {
                    burn_found = true;
                }
                Receipt::LogData { data: Some(data), .. } if burn_found => {
                    let token = ABIDecoder::default().decode(
                        &WithdrawalEvent::param_type(),
                         &data,
                        )?;
                    let withdrawal_event: WithdrawalEvent = WithdrawalEvent::from_token(token)?;
                    total_amount += withdrawal_event.amount;
                }
                _ => {}
            }
        }

        Ok(total_amount)
    }

    async fn verify_block_commit(&self, block_hash: &Bytes32) -> Result<bool> {
        for i in 0..FUEL_CONNECTION_RETRIES {
            match self.provider.block(block_hash).await {
                Ok(Some(_)) => {
                    return Ok(true);
                }
                Ok(None) => {
                    return Ok(false);
                }
                Err(e) => {
                    if i == FUEL_CONNECTION_RETRIES - 1 {
                        return Err(anyhow::anyhow!("{e}"));
                    }
                }
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuels::prelude::*;

    #[tokio::test]
    async fn test_check_connection() {
        // Start a local Fuel node
        let server = FuelService::start(Config::default()).await.unwrap();

        // Create a provider pointing to the local node
        let provider = Provider::from(server.bound_address()).await.unwrap();
        let provider = Arc::new(provider);

        // Initialize the FuelChain with the local provider
        let fuel_chain = FuelChain::new(provider).unwrap();

        // Test the check_connection function
        assert!(fuel_chain.check_connection().await.is_ok());
    }

    #[tokio::test]
    async fn test_get_seconds_since_last_block() {
        // Start a local Fuel node
        let server = FuelService::start(Config::default()).await.unwrap();

        // Create a provider pointing to the local node
        let provider = Provider::from(server.bound_address()).await.unwrap();
        let provider = Arc::new(provider);

        // Initialize the FuelChain with the local provider
        let fuel_chain = FuelChain::new(provider).unwrap();

        // Test the get_seconds_since_last_block function
        let seconds_since_last_block = fuel_chain.get_seconds_since_last_block().await;
        assert!(seconds_since_last_block.is_ok());

        // Test that seconds is not 0
        let seconds = seconds_since_last_block.unwrap();
        assert_ne!(seconds, 0);
    }

    #[tokio::test]
    async fn test_fetch_chain_info() {
        // Start a local Fuel node
        let server = FuelService::start(Config::default()).await.unwrap();

        // Create a provider pointing to the local node
        let provider = Provider::from(server.bound_address()).await.unwrap();
        let provider = Arc::new(provider);

        // Initialize the FuelChain with the local provider
        let fuel_chain = FuelChain::new(provider).unwrap();

        // Test fetch_chain_info
        let result = fuel_chain.fetch_chain_info().await;
        assert!(result.is_ok());
    }
}