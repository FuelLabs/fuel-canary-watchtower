use super::{ETHEREUM_BLOCK_TIME, ETHEREUM_CONNECTION_RETRIES};

use anyhow::Result;
use ethers::abi::Address;
use ethers::prelude::k256::ecdsa::SigningKey;
use ethers::prelude::{abigen, SignerMiddleware};
use ethers::providers::{Middleware};
use ethers::signers::{Wallet};
use ethers::types::{Filter, H160, H256, U256};
use std::cmp::max;

use std::ops::Mul;
use std::str::FromStr;
use std::sync::Arc;

abigen!(FuelERC20Gateway, "./abi/FuelERC20Gateway.json");

#[derive(Clone, Debug)]
pub struct GatewayContract<P>
where
    P: Middleware,
{
    provider: Arc<P>,
    wallet:  Wallet<SigningKey>,
    contract: Option<FuelERC20Gateway<SignerMiddleware<Arc<P>, Wallet<SigningKey>>>>,
    address: H160,
    read_only: bool,
}

impl <P>GatewayContract<P>
where
    P: Middleware + 'static,
{
    pub fn new(
        gateway_contract_address: String,
        read_only: bool,
        provider: Arc<P>,
        wallet: Wallet<SigningKey>,
    ) -> Result<Self> {
        let address: H160 = Address::from_str(&gateway_contract_address)?;

        Ok(GatewayContract {
            provider,
            wallet,
            contract: None,
            address,
            read_only,
        })
    }

    pub async fn initialize(&mut self) -> Result<()> {

        // Create the contract instance
        let client = SignerMiddleware::new(
            self.provider.clone(),
            self.wallet.clone(),
        );

        let contract = FuelERC20Gateway::new(
            self.address, Arc::new(client),
        );

        // Try calling a read function to check if the contract is valid
        match contract.paused().call().await {
            Err(_) => Err(anyhow::anyhow!("Invalid gateway contract.")),
            Ok(_) => {
                self.contract = Some(contract);
                Ok(())
            }
        }
    }

    pub async fn get_token_amount_deposited(
        &self,
        timeframe: u32,
        token_address: &str,
        latest_block_num: u64,
    ) -> Result<U256> {
        let block_offset = timeframe as u64 / ETHEREUM_BLOCK_TIME;
        let start_block = max(latest_block_num, block_offset) - block_offset;
        let token_address = match token_address.parse::<H160>() {
            Ok(addr) => addr,
            Err(e) => return Err(anyhow::anyhow!("{e}")),
        };

        // Deposit(bytes32 indexed sender, address indexed tokenId, bytes32 fuelTokenId,
        // uint256 amount)
        let token_topics = H256::from(token_address);
        let filter = Filter::new()
            .address(self.address)
            .event("Deposit(bytes32,address,bytes32,uint256)")
            .topic2(token_topics)
            .from_block(start_block);
        for i in 0..ETHEREUM_CONNECTION_RETRIES {
            match self.provider.get_logs(&filter).await {
                Ok(logs) => {
                    let mut total = U256::zero();
                    for log in logs {
                        let amount = U256::from_big_endian(
                            &log.data[32..64],
                        ).mul(U256::from(1_000_000_000));
                        total += amount;
                    }
                    return Ok(total);
                }
                Err(e) => {
                    if i == ETHEREUM_CONNECTION_RETRIES - 1 {
                        return Err(anyhow::anyhow!("{e}"));
                    }
                }
            }
        }

        Ok(U256::zero())
    }

    pub async fn get_token_amount_withdrawn(
        &self,
        timeframe: u32,
        token_address: &str,
        latest_block_num: u64,
    ) -> Result<U256> {
        let block_offset = timeframe as u64 / ETHEREUM_BLOCK_TIME;
        let start_block = max(latest_block_num, block_offset) - block_offset;
        let token_address = match token_address.parse::<H160>() {
            Ok(addr) => addr,
            Err(e) => return Err(anyhow::anyhow!("{e}")),
        };
    
        // Withdrawal(bytes32 indexed recipient, address indexed tokenId, bytes32 fuelTokenId,
        // uint256 amount)
        let token_topics = H256::from(token_address);
        let filter = Filter::new()
            .address(self.address)
            .event("Withdrawal(bytes32,address,bytes32,uint256)")
            .topic2(token_topics)
            .from_block(start_block);
        for i in 0..ETHEREUM_CONNECTION_RETRIES {
            match self.provider.get_logs(&filter).await {
                Ok(logs) => {
                    let mut total = U256::zero();
                    for log in logs {
                        let amount = U256::from_big_endian(
                            &log.data[32..64],
                        ).mul(U256::from(1_000_000_000));
                        total += amount;
                    }
                    return Ok(total);
                }
                Err(e) => {
                    if i == ETHEREUM_CONNECTION_RETRIES - 1 {
                        return Err(anyhow::anyhow!("{e}"));
                    }
                }
            }
        }
    
        Ok(U256::zero())
    }

    pub async fn pause(&self) -> Result<()> {
        if self.read_only {
            return Err(anyhow::anyhow!("Ethereum account not configured."));
        }

        match &self.contract {
            Some(contract) => {
                let result = contract.pause().call().await;
                match result {
                    Err(e) => Err(anyhow::anyhow!("Failed to pause gateway contract: {}", e)),
                    Ok(_) => Ok(()),
                }
            }
            None => Err(anyhow::anyhow!("Contract not initialized")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::abi::Token;
    use ethers::prelude::*;
    use ethers::providers::Provider;
    use std::sync::Arc;
    use std::str::FromStr;

    use super::GatewayContract;

    async fn setup_gateway_contract() -> Result<(GatewayContract<Provider<MockProvider>>, MockProvider), Box<dyn std::error::Error>> {

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
        let gateway_contract_address: String = "0xbe7aB12653e705642eb42EF375fd0d35Cfc45b03".to_string();
        let wallet: Wallet<SigningKey> = key_str.parse::<Wallet<SigningKey>>()?.with_chain_id(chain_id.as_u64());
    
        // Create a new gateway_contract with the dependencies injected.
        let gateway_contract: GatewayContract<Provider<MockProvider>> = GatewayContract::new(
            gateway_contract_address,
            read_only,
            arc_provider,
            wallet,
        )?;
    
        Ok((gateway_contract, mock))
    }

    #[tokio::test]
    async fn new_gateway_contract_creates_instance_correctly() {
        let (
            gateway_contract,
             _mock,
        ) = setup_gateway_contract().await.expect("Setup failed");
        assert!(!gateway_contract.read_only);
        assert_eq!(gateway_contract.address, H160::from_str("0xbe7aB12653e705642eb42EF375fd0d35Cfc45b03").unwrap());
    }

    #[tokio::test]
    async fn initialize_gateway_contract_initializes_contract() {
        let (
            mut gateway_contract,
             mock,
        ) = setup_gateway_contract().await.expect("Setup failed");

        let additional_response_hex = format!("0x{}", "00".repeat(32));
        mock.push_response(MockResponse::Value(serde_json::Value::String(additional_response_hex.to_string())));

        let result = gateway_contract.initialize().await;
        assert!(result.is_ok());
        assert!(gateway_contract.contract.is_some());
    }

    #[tokio::test]
    async fn get_token_amount_deposited_retrieves_correct_amount() {
        let (
            gateway_contract,
            mock,
        ) = setup_gateway_contract().await.expect("Setup failed");

        // Serialize the deposit amounts to a byte vector
        // Create a vector with 32 zero bytes
        let mut deposit_data_one: Vec<u8> = vec![0u8; 32];
        let mut deposit_data_two: Vec<u8> = vec![0u8; 32];

        // Extend the vectors with the encoded deposit amounts
        deposit_data_one.extend(ethers::abi::encode(&[Token::Uint(U256::from(100000u64))]));
        deposit_data_two.extend(ethers::abi::encode(&[Token::Uint(U256::from(230000u64))]));

        let empty_data = "0x0000000000000000000000000000000000000000000000000000000000000000".parse().unwrap();
        let log_entry_one = Log {
            address: "0x0000000000000000000000000000000000000001".parse().unwrap(),
            topics: vec![empty_data],
            data: deposit_data_one.clone().into(),
            block_hash: Some(empty_data),
            block_number: Some(U64::from(42)),
            transaction_hash: Some(empty_data),
            transaction_index: Some(U64::from(1)),
            log_index: Some(U256::from(2)),
            transaction_log_index: Some(U256::from(3)),
            log_type: Some("mined".to_string()),
            removed: Some(false),
        };
        let log_entry_two = Log {
            address: "0x0000000000000000000000000000000000000001".parse().unwrap(),
            topics: vec![empty_data],
            data: deposit_data_two.clone().into(),
            block_hash: Some(empty_data),
            block_number: Some(U64::from(42)),
            transaction_hash: Some(empty_data),
            transaction_index: Some(U64::from(1)),
            log_index: Some(U256::from(2)),
            transaction_log_index: Some(U256::from(3)),
            log_type: Some("mined".to_string()),
            removed: Some(false),
        };

        mock.push::<Vec<Log>, _>(vec![log_entry_one, log_entry_two]).unwrap();

        let token_address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
        let timeframe = 30;
        let latest_block_num = 42;

        let result = gateway_contract
            .get_token_amount_deposited(timeframe, token_address, latest_block_num)
            .await;
    
        assert!(result.is_ok(), "Failed to get token amount deposited");

        let total_amount: U256 = result.unwrap();
        assert_eq!(total_amount.as_u64(), 330000000000000, "Total amount deposited does not match expected value");
    }

    #[tokio::test]
    async fn get_token_amount_withdrawn_retrieves_correct_amount() {
        let (
            gateway_contract,
            mock,
        ) = setup_gateway_contract().await.expect("Setup failed");

        // Create and extend the vectors with the encoded withdrawal amounts, multiplied by 1_000_000_000
        let mut withdrawal_data_one: Vec<u8> = vec![0u8; 32];
        let mut withdrawal_data_two: Vec<u8> = vec![0u8; 32];

        withdrawal_data_one.extend(ethers::abi::encode(&[Token::Uint(U256::from(100000u64))]));
        withdrawal_data_two.extend(ethers::abi::encode(&[Token::Uint(U256::from(230000u64))]));

        let empty_data = "0x0000000000000000000000000000000000000000000000000000000000000000".parse().unwrap();
        let log_entry_one = Log {
            address: "0x0000000000000000000000000000000000000001".parse().unwrap(),
            topics: vec![empty_data],
            data: withdrawal_data_one.clone().into(),
            block_hash: Some(empty_data),
            block_number: Some(U64::from(42)),
            transaction_hash: Some(empty_data),
            transaction_index: Some(U64::from(1)),
            log_index: Some(U256::from(2)),
            transaction_log_index: Some(U256::from(3)),
            log_type: Some("mined".to_string()),
            removed: Some(false),
        };
        let log_entry_two = Log {
            address: "0x0000000000000000000000000000000000000001".parse().unwrap(),
            topics: vec![empty_data],
            data: withdrawal_data_two.clone().into(),
            block_hash: Some(empty_data),
            block_number: Some(U64::from(42)),
            transaction_hash: Some(empty_data),
            transaction_index: Some(U64::from(1)),
            log_index: Some(U256::from(2)),
            transaction_log_index: Some(U256::from(3)),
            log_type: Some("mined".to_string()),
            removed: Some(false),
        };

        mock.push::<Vec<Log>, _>(vec![log_entry_one, log_entry_two]).unwrap();

        let token_address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
        let timeframe = 30;
        let latest_block_num = 42;

        let result = gateway_contract
            .get_token_amount_withdrawn(timeframe, token_address, latest_block_num)
            .await;

        assert!(result.is_ok(), "Failed to get token amount withdrawn");

        let total_amount: U256 = result.unwrap();
        let expected_total = 330000u64 * 1_000_000_000;
        assert_eq!(total_amount.as_u64(), expected_total, "Total amount withdrawn does not match expected value");
    }

    #[tokio::test]
    async fn pause_gateway_contract_pauses_contract() {
        let (
            mut gateway_contract,
             mock,
        ) = setup_gateway_contract().await.expect("Setup failed");

        // Test pause before initialization
        assert!(gateway_contract.pause().await.is_err());

        // Initialize and test pause after initialization
        gateway_contract.initialize().await.expect("Initialization failed");

        let pause_response_hex: String = format!("0x{}", "01".repeat(32));
        mock.push_response(MockResponse::Value(serde_json::Value::String(pause_response_hex.to_string())));

        // Test pause with the contract initialized
        assert!(gateway_contract.pause().await.is_ok());
    }
}
