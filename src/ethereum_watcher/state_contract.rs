use super::ETHEREUM_CONNECTION_RETRIES;
use crate::WatchtowerConfig;

use anyhow::Result;
use ethers::abi::Address;
use ethers::prelude::k256::ecdsa::SigningKey;
use ethers::prelude::{abigen, SignerMiddleware};
use ethers::providers::{Http, Middleware, Provider};
use ethers::signers::{Signer, Wallet};
use ethers::types::{Filter, H160};
use fuels::tx::Bytes32;
use std::convert::TryFrom;
use std::str::FromStr;
use std::sync::Arc;

abigen!(FuelChainState, "./abi/FuelChainState.json");

#[derive(Clone, Debug)]
pub struct StateContract {
    provider: Provider<Http>,
    contract: FuelChainState<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>,
    address: H160,
    read_only: bool,
}

impl StateContract {
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
        let address = Address::from_str(&config.state_contract_address)?;
        let client = SignerMiddleware::new(provider.clone(), wallet);
        let contract = FuelChainState::new(address, Arc::new(client));

        // verify contract setup is valid
        let contract_result = contract.paused().call().await;
        match contract_result {
            Err(_) => Err(anyhow::anyhow!("Invalid state contract.")),
            Ok(_) => Ok(StateContract {
                provider,
                contract,
                address,
                read_only,
            }),
        }
    }

    pub async fn get_latest_commits(&self, from_block: u64) -> Result<Vec<Bytes32>> {
        //CommitSubmitted(uint256 indexed commitHeight, bytes32 blockHash)
        let filter = Filter::new()
            .address(self.address)
            .event("CommitSubmitted(uint256,bytes32)")
            .from_block(from_block);
        for i in 0..ETHEREUM_CONNECTION_RETRIES {
            match self.provider.get_logs(&filter).await {
                Ok(_logs) => {
                    // TODO

                    return Ok(vec![
                        Bytes32::new([1; 32]),
                        Bytes32::new([1; 32]),
                        Bytes32::new([1; 32]),
                    ]);
                }
                Err(e) => {
                    if i == ETHEREUM_CONNECTION_RETRIES - 1 {
                        return Err(anyhow::anyhow!("{e}"));
                    }
                }
            }
        }
        Ok(vec![])
    }

    pub async fn pause(&self) -> Result<()> {
        if self.read_only {
            return Err(anyhow::anyhow!("Ethereum account not configured."));
        }

        // TODO: implement alert on timeout and a gas escalator (https://github.com/gakonst/ethers-rs/blob/master/examples/middleware/examples/gas_escalator.rs)
        let result = self.contract.pause().call().await;
        match result {
            Err(e) => Err(anyhow::anyhow!("Failed to pause state contract: {}", e)),
            Ok(_) => Ok(()),
        }
    }
}
