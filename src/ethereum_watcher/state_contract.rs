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
pub struct StateContract<P>
where
    P: Middleware,
{
    provider: Arc<P>,
    contract: FuelChainState<SignerMiddleware<Arc<P>, Wallet<SigningKey>>>,
    address: H160,
    read_only: bool,
}

impl<P> StateContract<P>
where
    P: Middleware + 'static,
{
    pub async fn new(config: &WatchtowerConfig, provider: Arc<P>) -> Result<Self> {
        let chain_id = provider.get_chainid().await?;

        // setup wallet
        let mut read_only = false;
        let key_str: String = match &config.ethereum_wallet_key {
            Some(key) => key.clone(),
            None => {
                read_only = true;
                String::from("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
            }
        };
        let wallet: Wallet<SigningKey> = key_str.parse::<Wallet<SigningKey>>()?.with_chain_id(chain_id.as_u64());

        // setup contract
        let address = Address::from_str(&config.state_contract_address)?;
        let client = SignerMiddleware::new(provider.clone(),wallet);
        let contract = FuelChainState::new(address,Arc::new(client));

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
                Ok(logs) => {
        
                    // Create a Vec to store the extracted event data
                    let mut extracted_data: Vec<Bytes32> = Vec::new();

                    // Iterate over the logs and extract event data
                    for log in &logs {
                        // Extract the non-indexed event parameter (blockHash) from the data field
                        let mut bytes32_data: [u8; 32] = [0; 32];
                        
                        if log.data.len() == 32 {
                            bytes32_data.copy_from_slice(&log.data);
                        } else {
                            return Err(anyhow::anyhow!("Length of log.data does not match that of 32"));
                        }

                        // Add the extracted event data to the result vector
                        extracted_data.push( Bytes32::new(bytes32_data));
                    }
        
                    return Ok(extracted_data);
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


#[cfg(test)]
mod tests {
    use ethers::prelude::*;
    use ethers::providers::Provider;
    use crate::WatchtowerConfig;
    use crate::config::*;
    use std::sync::Arc;

    use super::StateContract;

    // A helper function to create a mock `WatchtowerConfig`.
    fn mock_watchtower_config() -> WatchtowerConfig {
        WatchtowerConfig {
            ethereum_rpc: "http://127.0.0.1".to_string(),
            ethereum_wallet_key: Some("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string()),
            fuel_graphql: "https://beta-4.fuel.network/graphql".to_string(),
            portal_contract_address: "0x03f2901Db5723639978deBed3aBA66d4EA03aF73".to_string(),
            gateway_contract_address:  "0x07cf0FF4fdD5d73C4ea5E96bb2cFaa324A348269".to_string(),
            state_contract_address: "0xbe7aB12653e705642eb42EF375fd0d35Cfc45b03".to_string(),
            duplicate_alert_delay: 900,
            fuel_client_watcher: FuelClientWatcher {
                connection_alert: GenericAlert {
                    alert_level: default_alert_level(),
                    alert_action: default_alert_action(),
                },
                block_production_alert: BlockProductionAlert {
                    alert_level: default_alert_level(),
                    alert_action: default_alert_action(),
                    max_block_time: default_max_block_time(),
                },
                portal_withdraw_alerts: vec![
                    WithdrawAlert {
                        alert_level: default_alert_level(),
                        alert_action: default_alert_action(),
                        token_name: default_token_name(),
                        token_decimals: default_token_decimals_fuel(),
                        token_address: default_token_address(),
                        time_frame: default_time_frame(),
                        amount: default_amount(),
                    },
                    // ... more mock WithdrawAlerts if necessary
                ],
                gateway_withdraw_alerts: vec![
                    WithdrawAlert {
                        alert_level: default_alert_level(),
                        alert_action: default_alert_action(),
                        token_name: default_token_name(),
                        token_decimals: default_token_decimals_fuel(),
                        token_address: default_token_address(),
                        time_frame: default_time_frame(),
                        amount: default_amount(),
                    },
                ],
            },
            ethereum_client_watcher: EthereumClientWatcher {
                connection_alert: GenericAlert {
                    alert_level: default_alert_level(),
                    alert_action: default_alert_action(),
                },
                block_production_alert: BlockProductionAlert {
                    alert_level: default_alert_level(),
                    alert_action: default_alert_action(),
                    max_block_time: default_max_block_time(),
                },
                account_funds_alert: AccountFundsAlert {
                    alert_level: default_alert_level(),
                    alert_action: default_alert_action(),
                    min_balance: default_minimum_balance(),
                },
                invalid_state_commit_alert: GenericAlert {
                    alert_level: default_alert_level(),
                    alert_action: default_alert_action(),
                },
                portal_deposit_alerts: vec![
                    DepositAlert {
                        alert_level: default_alert_level(),
                        alert_action: default_alert_action(),
                        token_name: default_token_name(),
                        token_decimals: default_token_decimals_ethereum(),
                        token_address: default_token_address(),
                        time_frame: default_time_frame(),
                        amount: default_amount(),
                    },
                ],
                gateway_deposit_alerts: vec![
                    DepositAlert {
                        alert_level: default_alert_level(),
                        alert_action: default_alert_action(),
                        token_name: default_token_name(),
                        token_decimals: default_token_decimals_ethereum(),
                        token_address: default_token_address(),
                        time_frame: default_time_frame(),
                        amount: default_amount(),
                    },
                ],
            },
        }
    }

    async fn setup_state_contract() -> Result<(StateContract<Provider<MockProvider>>, WatchtowerConfig, MockProvider), Box<dyn std::error::Error>> {
        let (provider, mock) = Provider::mocked();
        let config: WatchtowerConfig = mock_watchtower_config();
        let arc_provider = Arc::new(provider);

        // contract.paused().call() response
        let paused_response_hex = format!("0x{}", "00".repeat(32));
        mock.push_response(
            MockResponse::Value(serde_json::Value::String(paused_response_hex)),
        );

        // get_chainid() call response
        mock.push(U64::from(1337)).unwrap();

        // Create a new state_contract with the dependencies injected.
        let state_contract = StateContract::new(
            &config,
            arc_provider,
        ).await?;

        Ok((state_contract, config, mock))
    }

    #[tokio::test]
    async fn get_latest_commits_test() {
        let (
            state_contract,
            _config,
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
}