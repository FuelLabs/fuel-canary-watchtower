use super::{FUEL_BLOCK_TIME, FUEL_CONNECTION_RETRIES};
use crate::WatchtowerConfig;

use anyhow::Result;
use std::cmp::max;

#[derive(Clone, Debug)]
pub struct FungibleTokenContract {}

impl FungibleTokenContract {
    pub async fn new(config: &WatchtowerConfig) -> Result<Self> {
        Ok(FungibleTokenContract {})
    }

    pub async fn get_amount_withdrawn(
        &self,
        timeframe: u32,
        token_address: &str,
        latest_block_num: u64,
    ) -> Result<u64> {
        let block_offset = timeframe as u64 / FUEL_BLOCK_TIME;
        let start_block = max(latest_block_num, block_offset) - block_offset;
        // TODO

        Ok(0)
    }
}
