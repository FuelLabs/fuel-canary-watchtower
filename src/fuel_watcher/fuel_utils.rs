use std::sync::Arc;
use anyhow::{Result, anyhow};
use fuels::prelude::{Provider};

pub async fn setup_fuel_provider(fuels_graphql: &str) -> Result<Arc<Provider>> {
    let provider = Provider::connect(&fuels_graphql).await?;
    let provider_result = provider.chain_info().await;
    match provider_result {
        Ok(_) => Ok(Arc::new(provider)),
        Err(e) => Err(anyhow!("Failed to get chain ID: {e}")),
    }
}
