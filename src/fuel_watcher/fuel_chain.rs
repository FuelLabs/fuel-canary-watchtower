use super::{FUEL_BLOCK_TIME, FUEL_CONNECTION_RETRIES};

use anyhow::Result;
use fuels::{
    client::{PageDirection, PaginationRequest},
    tx::Bytes32,
};
use fuels::prelude::{Provider, TransactionType};
use fuels::types::tx_status::TxStatus;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use fuels::tx::Receipt;
use fuels::types::chain_info::ChainInfo;

// TODO: we need create an interface for the provider, so we can create a mockprovider
// this will allow us to test functions.
#[derive(Clone, Debug)]
pub struct FuelChain {
    provider: Arc<Provider>,
}

impl FuelChain {

    pub fn new(provider: Arc<Provider>) -> Result<Self> {
        Ok(FuelChain { provider })
    }

    pub async fn check_connection(&self) -> Result<()> {
        for _ in 0..FUEL_CONNECTION_RETRIES {
            if let Ok(_) = self.provider.chain_info().await {
                return Ok(());
            }
        }
        Err(anyhow::anyhow!(
            "Failed to establish connection after {} retries", FUEL_CONNECTION_RETRIES),
        )
    }

    pub async fn get_seconds_since_last_block(&self) -> Result<u32> {
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

    pub async fn get_amount_withdrawn(&self, timeframe: u32) -> Result<u64> {
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
                            match self.get_amount_withdrawn_from_tx(&tx_id).await {
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

    pub async fn get_amount_withdrawn_from_tx(&self, tx_id: &Bytes32) -> Result<u64> {

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

        println!("{}", tx_id);

        // Fetch the receipts from the transaction.
        let receipts = self.provider.tx_status(tx_id).await?.take_receipts();
        for receipt in receipts{
            if let Receipt::MessageOut { amount, .. } = receipt {
                total_amount += amount;
            }
        }

        Ok(total_amount)
    }

    pub async fn verify_block_commit(&self, block_hash: &Bytes32) -> Result<bool> {
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

    pub fn get_value(value_fp: f64, decimals: u8) -> u64 {
        let decimals_p1 = if decimals < 9 { decimals } else { decimals - 9 };
        let decimals_p2 = decimals - decimals_p1;

        let value = value_fp * 10.0_f64.powf(decimals_p1 as f64);
        
        (value as u64) * 10_u64.pow(decimals_p2 as u32)
    }
}
