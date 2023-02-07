use super::FUEL_CONNECTION_RETRIES;
use crate::WatchtowerConfig;

use anyhow::Result;

#[derive(Clone, Debug)]
pub struct FuelChain {}

impl FuelChain {
    pub async fn new(config: &WatchtowerConfig) -> Result<Self> {
        // setup provider and check that it is valid
        // TODO

        Ok(FuelChain {})
    }

    pub async fn check_connection(&self) -> Result<()> {
        // TODO
        Ok(())
    }

    pub async fn get_seconds_since_last_block(&self) -> Result<u32> {
        // TODO
        Ok(0)
    }

    pub async fn get_latest_block_number(&self) -> Result<u64> {
        // TODO
        Ok(0)
    }

    pub async fn get_amount_withdrawn(&self, timeframe: u32, latest_block_num: u64) -> Result<u64> {
        // TODO
        Ok(0)
    }

    pub async fn verify_block_commit(&self, block_hash: &str) -> Result<bool> {
        // TODO: check block hash exists in the chain and that the block number is at the end of an epoch
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
