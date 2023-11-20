use std::sync::Arc;
use anyhow::{Result, anyhow};
use fuels::prelude::Provider;

pub async fn setup_fuel_provider(fuels_graphql: &str) -> Result<Arc<Provider>> {
    let provider = Provider::connect(&fuels_graphql).await?;
    let provider_result = provider.chain_info().await;
    match provider_result {
        Ok(_) => Ok(Arc::new(provider)),
        Err(e) => Err(anyhow!("Failed to get chain ID: {e}")),
    }
}

pub fn get_value(value_fp: f64, decimals: u8) -> u64 {
    let decimals_p1 = if decimals < 9 { decimals } else { decimals - 9 };
    let decimals_p2 = decimals - decimals_p1;

    let value = value_fp * 10.0_f64.powf(decimals_p1 as f64);

    (value as u64) * 10_u64.pow(decimals_p2 as u32)
}