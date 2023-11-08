use super::ETHEREUM_CONNECTION_RETRIES;
use crate::WatchtowerConfig;

use anyhow::Result;
use ethers::prelude::k256::ecdsa::SigningKey;
use ethers::providers::{Http, Middleware, Provider};
use ethers::signers::{Signer, Wallet};
use ethers::types::Address;
use ethers::utils::hex::ToHex;
use std::ops::Mul;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

pub use ethers::types::U256;

#[derive(Clone, Debug)]
pub struct EthereumChain {
    provider: Provider<Http>,
}

impl EthereumChain {
    pub async fn new(config: &WatchtowerConfig) -> Result<Self> {
        // setup provider and check that it is valid
        let provider = Provider::<Http>::try_from(&config.ethereum_rpc)?;
        let provider_result = provider.get_chainid().await;
        match provider_result {
            Err(e) => Err(anyhow::anyhow!("Invalid ethereum RPC: {e}")),
            Ok(_) => Ok(EthereumChain { provider }),
        }
    }

    pub async fn check_connection(&self) -> Result<()> {
        for i in 0..ETHEREUM_CONNECTION_RETRIES {
            match self.provider.get_chainid().await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if i == ETHEREUM_CONNECTION_RETRIES - 1 {
                        return Err(anyhow::anyhow!("{e}"));
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn get_seconds_since_last_block(&self) -> Result<u32> {
        let block_num = self.get_latest_block_number().await?;
        for i in 0..ETHEREUM_CONNECTION_RETRIES {
            match self.provider.get_block(block_num).await {
                Ok(block_result) => {
                    return match block_result {
                        Some(block) => {
                            let last_block_timestamp = block.timestamp.as_u64();
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
                    if i == ETHEREUM_CONNECTION_RETRIES - 1 {
                        return Err(anyhow::anyhow!("{e}"));
                    }
                }
            }
        }
        Ok(0)
    }

    pub async fn get_latest_block_number(&self) -> Result<u64> {
        for i in 0..ETHEREUM_CONNECTION_RETRIES {
            match self.provider.get_block_number().await {
                Ok(num) => return Ok(num.as_u64()),
                Err(e) => {
                    if i == ETHEREUM_CONNECTION_RETRIES - 1 {
                        return Err(anyhow::anyhow!("{e}"));
                    }
                }
            }
        }
        Ok(0)
    }

    pub async fn get_account_balance(&self, addr: &str) -> Result<U256> {
        for i in 0..ETHEREUM_CONNECTION_RETRIES {
            match self.provider.get_balance(Address::from_str(addr)?, None).await {
                Ok(balance) => return Ok(balance),
                Err(e) => {
                    if i == ETHEREUM_CONNECTION_RETRIES - 1 {
                        return Err(anyhow::anyhow!("{e}"));
                    }
                }
            }
        }
        Ok(U256::zero())
    }

    pub async fn get_public_address(key_str: &str) -> Result<String> {
        let wallet: Wallet<SigningKey> = key_str.parse::<Wallet<SigningKey>>()?;
        Ok(wallet.address().encode_hex())
    }

    pub fn get_value(value_fp: f64, decimals: u8) -> U256 {
        let decimals_p1 = if decimals < 9 { decimals } else { decimals - 9 };
        let decimals_p2 = decimals - decimals_p1;

        let value = value_fp * 10.0_f64.powf(decimals_p1 as f64);
        let value = U256::from(value as u64);
        
        value.mul(10_u64.pow(decimals_p2 as u32))
    }
}
