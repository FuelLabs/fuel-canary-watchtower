use super::ETHEREUM_CONNECTION_RETRIES;

use anyhow::{Result, anyhow};
use ethers::providers::Middleware;
use ethers::types::Address;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::Arc;

pub use ethers::types::U256;

#[derive(Clone, Debug)]
pub struct EthereumChain<P>
where
    P: Middleware,
{
    provider: Arc<P>,
}

impl <P>EthereumChain<P>
where
    P: Middleware + 'static,
{
    pub async fn new(provider: Arc<P>) -> Result<Self> {
        Ok(EthereumChain { provider })
    }

    pub async fn check_connection(&self) -> Result<()> {
        for _ in 0..ETHEREUM_CONNECTION_RETRIES {
            if let Ok(_) = self.provider.get_chainid().await {
                return Ok(());
            }
        }
        Err(anyhow::anyhow!(
            "Failed to establish connection after {} retries", ETHEREUM_CONNECTION_RETRIES),
        )
    }

    pub async fn get_seconds_since_last_block(&self) -> Result<u32> {
        let block_num = self.get_latest_block_number().await?;
        let mut block_option = None;

        for _ in 0..ETHEREUM_CONNECTION_RETRIES {
            match self.provider.get_block(block_num).await {
                Ok(block) => {
                    block_option = block;
                    break;
                }
                Err(_) => {
                    // Optionally log each retry failure here
                }
            }
        }

        let block = block_option.ok_or_else(|| anyhow!(
            "Failed to get block after {} retries", ETHEREUM_CONNECTION_RETRIES),
        )?;

        let last_block_timestamp = block.timestamp.as_u64();
        let millis_now = (
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64
        ) / 1000;

        if millis_now >= last_block_timestamp {
            Ok((millis_now - last_block_timestamp) as u32)
        }else{
            Err(anyhow!("Block time is ahead of current time"))
        }
    }

    pub async fn get_latest_block_number(&self) -> Result<u64> {
        for _ in 0..ETHEREUM_CONNECTION_RETRIES {
            if let Ok(num) = self.provider.get_block_number().await {
                return Ok(num.as_u64());
            }
        }
        Err(anyhow::anyhow!(
            "Failed to retrieve block number after {} retries", ETHEREUM_CONNECTION_RETRIES),
        )
    }

    pub async fn get_account_balance(&self, addr: &str) -> Result<U256> {
        for i in 0..ETHEREUM_CONNECTION_RETRIES {
            if let Ok(balance) = self.provider.get_balance(
                Address::from_str(addr)?,
                None,
            ).await {
                return Ok(balance);
            }
        }
        Err(anyhow::anyhow!(
            "Failed to retrieve balance after {} retries", ETHEREUM_CONNECTION_RETRIES),
        )
    }
}
