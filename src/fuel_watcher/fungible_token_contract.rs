use super::{FUEL_BLOCK_TIME};
use crate::WatchtowerConfig;

use anyhow::Result;


#[derive(Clone, Debug)]
pub struct FungibleTokenContract {}

impl FungibleTokenContract {
    pub async fn new(_config: &WatchtowerConfig) -> Result<Self> {
        Ok(FungibleTokenContract {})
    }

    pub async fn get_amount_withdrawn(&self, timeframe: u32, _token_address: &str) -> Result<u64> {
        let _block_offset = timeframe as u64 / FUEL_BLOCK_TIME;
        // TODO

        Ok(0)
    }
}
