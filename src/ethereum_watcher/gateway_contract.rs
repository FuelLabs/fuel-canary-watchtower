use super::{ETHEREUM_BLOCK_TIME, ETHEREUM_CONNECTION_RETRIES};

use anyhow::Result;
use ethers::abi::Address;
use ethers::prelude::k256::ecdsa::SigningKey;
use ethers::prelude::{abigen, SignerMiddleware};
use ethers::providers::{Middleware};
use ethers::signers::{Signer, Wallet};
use ethers::types::{Filter, H160, H256, U256};
use std::cmp::max;

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

    pub async fn get_amount_deposited(
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
                        let amount = U256::from_big_endian(&log.data[32..64]);
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

    // pub async fn get_amount_withdrawn(
    //     &self,
    //     timeframe: u32,
    //     token_address: &str,
    //     latest_block_num: u64,
    // ) -> Result<U256> {
    //     let block_offset = timeframe as u64 / ETHEREUM_BLOCK_TIME;
    //     let start_block = max(latest_block_num, block_offset) - block_offset;
    //     let token_address = match token_address.parse::<H160>() {
    //         Ok(addr) => addr,
    //         Err(e) => return Err(anyhow::anyhow!("{e}")),
    //     };
    //
    //     // Withdrawal(bytes32 indexed recipient, address indexed tokenId, bytes32 fuelTokenId,
    //     // uint256 amount)
    //     let token_topics = H256::from(token_address);
    //     let filter = Filter::new()
    //         .address(self.address)
    //         .event("Withdrawal(bytes32,address,bytes32,uint256)")
    //         .topic2(token_topics)
    //         .from_block(start_block);
    //     for i in 0..ETHEREUM_CONNECTION_RETRIES {
    //         match self.provider.get_logs(&filter).await {
    //             Ok(logs) => {
    //                 let mut total = U256::zero();
    //                 for log in logs {
    //                     let amount = U256::from_big_endian(&log.data[32..64]);
    //                     total += amount;
    //                 }
    //                 return Ok(total);
    //             }
    //             Err(e) => {
    //                 if i == ETHEREUM_CONNECTION_RETRIES - 1 {
    //                     return Err(anyhow::anyhow!("{e}"));
    //                 }
    //             }
    //         }
    //     }
    //
    //     Ok(U256::zero())
    // }

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
                }.expect("TODO: panic message");
                Ok(())
            }
            None => Err(anyhow::anyhow!("Contract not initialized")),
        }
    }
}
