use super::{ETHEREUM_CONNECTION_RETRIES, ethereum_utils};


use anyhow::Result;
use ethers::abi::Address;
use ethers::prelude::k256::ecdsa::SigningKey;
use ethers::prelude::{abigen, SignerMiddleware};
use ethers::providers::{Middleware};
use ethers::signers::{Wallet};
use ethers::types::{Filter, H160};
use fuels::tx::Bytes32;

use std::str::FromStr;
use std::sync::Arc;

abigen!(FuelChainState, "./abi/FuelChainState.json");

#[derive(Clone, Debug)]
pub struct StateContract<P>
where
    P: Middleware,
{
    provider: Arc<P>,
    wallet:  Wallet<SigningKey>,
    contract: Option<FuelChainState<SignerMiddleware<Arc<P>, Wallet<SigningKey>>>>,
    address: H160,
    read_only: bool,
}

impl <P>StateContract<P>
where
    P: Middleware + 'static,
{   
    pub fn new(
        state_contract_address: String,
        read_only: bool,
        provider: Arc<P>,
        wallet: Wallet<SigningKey>,
    ) -> Result<Self> {
        let address: H160 = Address::from_str(&state_contract_address)?;

        Ok(StateContract {
            provider,
            wallet,
            address,
            contract: None,
            read_only,
        })
    }

    pub async fn initialize(&mut self) -> Result<()> {
        
        // Create the contract instance
        let client = SignerMiddleware::new(
            self.provider.clone(),
             self.wallet.clone(),
            );

        let contract = FuelChainState::new(
            self.address, Arc::new(client),
        );

        // Try calling a read function to check if the contract is valid
        match contract.paused().call().await {
            Err(_) => Err(anyhow::anyhow!("Invalid state contract.")),
            Ok(_) => {
                self.contract = Some(contract);
                Ok(())
            }
        }
    }

    pub async fn get_latest_commits(&self, from_block: u64) -> Result<Vec<Bytes32>> {
        //CommitSubmitted(uint256 indexed commitHeight, bytes32 blockHash)
        let filter = Filter::new()
            .address(self.address)
            .event("CommitSubmitted(uint256,bytes32)")
            .from_block(from_block);

        for i in 0..ETHEREUM_CONNECTION_RETRIES {
            let logs = match self.provider.get_logs(&filter).await {
                Ok(logs) => logs,
                Err(e) if i == ETHEREUM_CONNECTION_RETRIES - 1 => return Err(anyhow::anyhow!("{e}")),
                _ => continue,
            };

            return ethereum_utils::process_logs(logs);
        }

        Ok(vec![])
    }

    pub async fn pause(&self) -> Result<()> {
        if self.read_only {
            return Err(anyhow::anyhow!("Ethereum account not configured."));
        }
        
        match &self.contract {
            Some(contract) => {
                let result = contract.pause().call().await;
                match result {
                    Err(e) => Err(anyhow::anyhow!("Failed to pause state contract: {}", e)),
                    Ok(_) => Ok(()),
                }
            }
            None => Err(anyhow::anyhow!("Contract not initialized")),
        }
    }
}
