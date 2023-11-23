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

#[cfg(test)]
mod tests {
    use super::get_value;

    #[test]
    fn test_get_value() {
        // Test case 1: Simple conversion without decimal points
        assert_eq!(get_value(100.0, 0), 100);

        // Test case 2: Conversion with decimal points
        assert_eq!(get_value(123.45, 2), 12345);

        // Test case 3: Large number of decimals
        assert_eq!(get_value(1.23456788, 8), 123456788);
    }
}
