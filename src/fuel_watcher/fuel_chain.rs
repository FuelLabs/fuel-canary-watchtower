use super::{FUEL_BLOCK_TIME, FUEL_CONNECTION_RETRIES};
use crate::WatchtowerConfig;

use anyhow::Result;
use fuels::{
    client::{PageDirection, PaginationRequest},
    prelude::Provider,
    tx::Bytes32,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct FuelChain {
    provider: Provider,
}

impl FuelChain {
    pub async fn new(config: &WatchtowerConfig) -> Result<Self> {
        // setup provider and check that it is valid
        let provider = Provider::connect(&config.fuel_graphql).await?;
        let provider_result = provider.chain_info().await;
        match provider_result {
            Err(e) => Err(anyhow::anyhow!("Invalid fuel graphql endpoint: {e}")),
            Ok(_) => Ok(FuelChain { provider }),
        }
    }

    pub async fn check_connection(&self) -> Result<()> {
        for i in 0..FUEL_CONNECTION_RETRIES {
            match self.provider.chain_info().await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if i == FUEL_CONNECTION_RETRIES - 1 {
                        return Err(anyhow::anyhow!("{e}"));
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn get_seconds_since_last_block(&self) -> Result<u32> {
        for i in 0..FUEL_CONNECTION_RETRIES {
            match self.provider.chain_info().await {
                Ok(info) => {
                    return match info.latest_block.header.time {
                        Some(time) => {
                            let last_block_timestamp = (time.timestamp_millis() as u64) / 1000;
                            let millis_now =
                                (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64) / 1000;
                            if millis_now >= last_block_timestamp {
                                Ok((millis_now - last_block_timestamp) as u32)
                            } else {
                                Err(anyhow::anyhow!("Block time is ahead of current time"))
                            }
                        }
                        None => Err(anyhow::anyhow!("Failed to get latest block")),
                    }
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
        for i in 0..FUEL_CONNECTION_RETRIES {
            match self.provider.get_transaction_by_id(&tx_id).await {
                Ok(tx_result) => {
                    match tx_result {
                        Some(tx) => {
                            // TODO

                            return Ok(0); ///////////////////////////////
                        }
                        None => {
                            if i == FUEL_CONNECTION_RETRIES - 1 {
                                return Err(anyhow::anyhow!("Failed to find details for transaction: {tx_id}"));
                            }
                        }
                    }
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

    pub async fn verify_block_commit(&self, block_hash: &Bytes32) -> Result<bool> {
        for i in 0..FUEL_CONNECTION_RETRIES {
            match self.provider.block(block_hash).await {
                Ok(_) => return Ok(true),
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

        let value = value_fp * (10.0 as f64).powf(decimals_p1 as f64);
        let value = (value as u64) * (10 as u64).pow(decimals_p2 as u32);
        value
    }
}
