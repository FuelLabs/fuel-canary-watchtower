use super::ETHEREUM_CONNECTION_RETRIES;
use crate::WatchtowerConfig;

use anyhow::Result;
use ethers::abi::Address;
use ethers::prelude::k256::ecdsa::SigningKey;
use ethers::prelude::{abigen, SignerMiddleware};
use ethers::providers::{Http, Middleware, Provider};
use ethers::signers::{Signer, Wallet};
use std::convert::TryFrom;
use std::str::FromStr;
use std::sync::Arc;

abigen!(FuelChainConsensus, "./abi/FuelChainConsensus.json");

#[derive(Clone, Debug)]
pub struct ConsesnsusContract {
    contract: FuelChainConsensus<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>,
    read_only: bool,
}

impl ConsesnsusContract {
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
        let contract = FuelChainConsensus::new(addr, Arc::new(client));

        // verify contract setup is valid
        let contract_result = contract.paused().call().await;
        match contract_result {
            Err(_) => Err(anyhow::anyhow!("Invalid consensus contract.")),
            Ok(_) => Ok(ConsesnsusContract { contract, read_only }),
        }
    }

    pub async fn get_latest_commits(&self, from_block: u64) -> Result<Vec<String>> {
        // TODO

        Ok(vec![
            String::from("0xe605385d3c61bdb625d58b2c55999730d9916fd90b62ce4af0d143539c3a5cb9"),
            String::from("0x24246dd5331b1eb52588ae8c0082987b1eb679edc09a4b5da305e4ebe5425c5a"),
            String::from("0x3de6af85929f81780a443b8db918a37036b7fe1b2e46e32fe876250806ab739d"),
        ])
    }

    pub async fn pause(&self) -> Result<()> {
        if self.read_only {
            return Err(anyhow::anyhow!("Ethereum account not configured."))
        }

        // TODO: implement alert on timeout and a gas escalator (https://github.com/gakonst/ethers-rs/blob/master/examples/middleware/examples/gas_escalator.rs)
        let result = self.contract.pause().call().await;
        match result {
            Err(e) => Err(anyhow::anyhow!("Failed to pause consensus contract: {}", e)),
            Ok(_) => Ok(()),
        }
    }
}
