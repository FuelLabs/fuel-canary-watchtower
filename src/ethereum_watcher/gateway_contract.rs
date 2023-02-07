use super::{ETHEREUM_BLOCK_TIME, ETHEREUM_CONNECTION_RETRIES};
use crate::WatchtowerConfig;

use anyhow::Result;
use ethers::abi::Address;
use ethers::prelude::k256::ecdsa::SigningKey;
use ethers::prelude::{abigen, SignerMiddleware};
use ethers::providers::{Http, Middleware, Provider};
use ethers::signers::{Signer, Wallet};
use ethers::types::U256;
use std::cmp::max;
use std::convert::TryFrom;
use std::str::FromStr;
use std::sync::Arc;

abigen!(FuelERC20Gateway, "./abi/FuelERC20Gateway.json");

#[derive(Clone, Debug)]
pub struct GatewayContract {
    contract: FuelERC20Gateway<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>,
    read_only: bool,
}

impl GatewayContract {
    pub async fn new(config: &WatchtowerConfig) -> Result<Self> {
        // setup provider
        let provider = Provider::<Http>::try_from(&config.ethereum_rpc)?;
        let chain_id = provider.get_chainid().await?.as_u64();

        // setup wallet
        let mut read_only = false;
        let key_str = match &config.ethereum_wallet_key {
            Some(key) => key.clone(),
            None => {
                read_only = true;
                String::from("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
            }
        };
        let wallet: Wallet<SigningKey> = key_str.parse::<Wallet<SigningKey>>()?.with_chain_id(chain_id);

        // setup contract
        let addr = Address::from_str(&config.consenses_contract_address)?;
        let client = SignerMiddleware::new(provider, wallet);
        let contract = FuelERC20Gateway::new(addr, Arc::new(client));

        // verify contract setup is valid
        let contract_result = contract.paused().call().await;
        match contract_result {
            Err(_) => Err(anyhow::anyhow!("Invalid gateway contract.")),
            Ok(_) => Ok(GatewayContract { contract, read_only }),
        }
    }

    pub async fn get_amount_deposited(
        &self,
        timeframe: u32,
        token_address: &str,
        latest_block_num: u64,
    ) -> Result<U256> {
        let block_offset = timeframe as u64 / ETHEREUM_BLOCK_TIME;
        let start_block = max(latest_block_num, block_offset) - block_offset;
        // TODO

        Ok(U256::zero())
    }

    pub async fn pause(&self) -> Result<()> {
        if self.read_only {
            return Err(anyhow::anyhow!("Ethereum account not configured."))
        }

        // TODO: implement alert on timeout and a gas escalator (https://github.com/gakonst/ethers-rs/blob/master/examples/middleware/examples/gas_escalator.rs)
        let result = self.contract.pause().call().await;
        match result {
            Err(e) => Err(anyhow::anyhow!("Failed to pause gateway contract: {}", e)),
            Ok(_) => Ok(()),
        }
    }
}
