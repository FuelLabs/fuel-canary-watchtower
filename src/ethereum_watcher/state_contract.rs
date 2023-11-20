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


#[cfg(test)]
mod tests {
    use ethers::prelude::*;
    use ethers::providers::Provider;
    use fuels::accounts::fuel_crypto::coins_bip32::ecdsa::SigningKey;
    use std::sync::Arc;

    use super::StateContract;

    async fn setup_state_contract() -> Result<(StateContract<Provider<MockProvider>>, MockProvider), Box<dyn std::error::Error>> {
        let (provider, mock) = Provider::mocked();
        let arc_provider = Arc::new(provider);

        // contract.paused().call() response
        let paused_response_hex: String = format!("0x{}", "00".repeat(32));
        mock.push_response(
            MockResponse::Value(serde_json::Value::String(paused_response_hex)),
        );
        
        let read_only: bool = false;
        let chain_id: U64 = ethers::types::U64::from(1337);
        let key_str: String = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string();
        let state_contract_address: String = "0xbe7aB12653e705642eb42EF375fd0d35Cfc45b03".to_string();
        let wallet: Wallet<SigningKey> = key_str.parse::<Wallet<SigningKey>>()?.with_chain_id(chain_id.as_u64());


        // Create a new state_contract with the dependencies injected.
        let state_contract: StateContract<Provider<MockProvider>> = StateContract::new(
            state_contract_address,
            read_only,
            arc_provider,
            wallet,
        )?;

        Ok((state_contract, mock))
    }

    #[tokio::test]
    async fn new_state_contract_test() {
        let (
            state_contract,
            _mock,
        ) = setup_state_contract().await.expect("Setup failed");
        assert_eq!(state_contract.read_only, false);
        assert_eq!(state_contract.address, "0xbe7aB12653e705642eb42EF375fd0d35Cfc45b03".parse().unwrap());
    }

    #[tokio::test]
    async fn initialize_state_contract_test() {
        let (
            mut state_contract,
              mock,
        ) = setup_state_contract().await.expect("Setup failed");

        // Mock a successful response for the `paused` call
        let paused_response_hex: String = format!("0x{}", "00".repeat(32));
        mock.push_response(
            MockResponse::Value(serde_json::Value::String(paused_response_hex)),
        );

        let result = state_contract.initialize().await;
        assert!(result.is_ok());
        assert!(state_contract.contract.is_some());
    }

    #[tokio::test]
    async fn get_latest_commits_test() {
        let (
            state_contract,
            mock,
        ) = setup_state_contract().await.expect("Setup failed");

        let empty_data = "0x0000000000000000000000000000000000000000000000000000000000000000".parse().unwrap();
        let expected_commit:Bytes = "0xc84e7c26f85536eb8c9c1928f89c10748dd11232a3f86826e67f5caee55ceede".parse().unwrap();
        let log_entry = Log {
            address: "0x0000000000000000000000000000000000000001".parse().unwrap(),
            topics: vec![empty_data],
            data: expected_commit.clone(),
            block_hash: Some(empty_data),
            block_number: Some(U64::from(42)),
            transaction_hash: Some(empty_data),
            transaction_index: Some(U64::from(1)),
            log_index: Some(U256::from(2)),
            transaction_log_index: Some(U256::from(3)),
            log_type: Some("mined".to_string()),
            removed: Some(false),
        };

        mock.push::<Vec<Log>, _>(vec![log_entry]).unwrap();

        let mut bytes32_data: [u8; 32] = [0; 32];
        bytes32_data.copy_from_slice(&expected_commit);

        let block_num: u64 = 42;
        let commits = state_contract.get_latest_commits(block_num).await.unwrap();
        assert_eq!(&commits[0].as_slice(), &bytes32_data.as_slice());
    }

    #[tokio::test]
    async fn pause_state_contract_test() {
        let (
            mut state_contract,
              mock,
        ) = setup_state_contract().await.expect("Setup failed");

        // Test pause before initialization
        assert!(state_contract.pause().await.is_err());

        // Initialize and test pause after initialization
        state_contract.initialize().await.expect("Initialization failed");

        // Mock a successful response for the `pause` call
        let pause_response_hex: String = format!("0x{}", "01".repeat(32));
        mock.push_response(MockResponse::Value(serde_json::Value::String(pause_response_hex)));

        // Test pause with the contract initialized
        assert!(state_contract.pause().await.is_ok());
    }
}