use crate::alerts::{AlertLevel, WatchtowerAlerts};
use crate::ethereum_actions::WatchtowerEthereumActions;
use crate::fuel_watcher::fuel_chain::FuelChain;
use crate::WatchtowerConfig;

use anyhow::Result;
use ethereum_chain::EthereumChain;
use state_contract::StateContract;
use gateway_contract::GatewayContract;
use portal_contract::PortalContract;
use std::cmp::max;
use std::thread;
use std::time::Duration;
use tokio::task::JoinHandle;
use ethers::prelude::*;
use crate::config::EthereumClientWatcher;
use crate::ethereum_watcher::ethereum_utils::get_value;

pub mod state_contract;
pub mod ethereum_chain;
pub mod gateway_contract;
pub mod portal_contract;
pub mod ethereum_utils;

pub static POLL_DURATION: Duration = Duration::from_millis(6000);
pub static POLL_LOGGING_SKIP: u64 = 50;
pub static COMMIT_CHECK_STARTING_OFFSET: u64 = 24 * 60 * 60;
pub static ETHEREUM_CONNECTION_RETRIES: u64 = 2;
pub static ETHEREUM_BLOCK_TIME: u64 = 12;

async fn check_chain_connection(
    ethereum_chain: &EthereumChain<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerts,
    watch_config: &EthereumClientWatcher, // Assuming this is part of your config
) -> Result<(), anyhow::Error> {
    match ethereum_chain.check_connection().await {
        Ok(_) => Ok(()),
        Err(e) => {
            alerts.alert(
                format!("Failed to check ethereum connection: {}", e),
                watch_config.connection_alert.alert_level.clone(),
            );
            Err(e.into())
        }
    }
}

async fn check_block_production(
    ethereum_chain: &EthereumChain<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerts,
    watch_config: &EthereumClientWatcher,
) -> Result<(), anyhow::Error> {
    match ethereum_chain.get_seconds_since_last_block().await {
        Ok(seconds_since_last_block) => {
            if seconds_since_last_block > watch_config.block_production_alert.max_block_time {
                alerts.alert(
                    format!(
                        "Next ethereum block is taking longer than {} seconds. Last block was {} seconds ago.",
                        watch_config.block_production_alert.max_block_time, seconds_since_last_block
                    ),
                    watch_config.block_production_alert.alert_level.clone(),
                );
            }
            Ok(())
        }
        Err(e) => {
            alerts.alert(
                format!("Failed to check ethereum block production: {}", e),
                watch_config.block_production_alert.alert_level.clone(),
            );
            Err(e.into())
        }
    }
}

async fn check_account_balance(
    ethereum_chain: &EthereumChain<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerts,
    watch_config: &EthereumClientWatcher,
    actions: &WatchtowerEthereumActions,
    account_address: &Option<String>,
) -> Result<(), anyhow::Error> {
    let address = match account_address {
        Some(addr) => addr,
        None => return Ok(()), // Return early if there's no address
    };

    let balance = ethereum_chain.get_account_balance(address).await?;
    let min_balance = get_value(watch_config.account_funds_alert.min_balance, 18);

    if balance < min_balance {
        alerts.alert(
            format!("Ethereum account ({}) is low on funds. Current balance: {}", address, balance),
            watch_config.account_funds_alert.alert_level.clone(),
        );
        // Here, you may also invoke an action if necessary
        actions.action(
            watch_config.account_funds_alert.alert_action.clone(),
            Some(watch_config.account_funds_alert.alert_level.clone()),
        );
    }

    Ok(())
}

async fn check_invalid_commits(
    state_contract: &StateContract<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerts,
    alert_config: &EthereumClientWatcher,
    fuel_chain: &FuelChain,
    last_commit_check_block: &mut u64,
    actions: &WatchtowerEthereumActions,
    ethereum_chain: &EthereumChain<GasEscalatorMiddleware<Provider<Http>>>,
) -> Result<(), anyhow::Error> {

    let hashes = state_contract.get_latest_commits(*last_commit_check_block).await?;

    for hash in hashes {
        match fuel_chain.verify_block_commit(&hash).await {
            Ok(valid) => {
                if !valid {
                    alerts.alert(
                        format!("An invalid commit was made on the state contract. Hash: {}", hash),
                        alert_config.invalid_state_commit_alert.alert_level.clone(),
                    );
                    actions.action(
                        alert_config.invalid_state_commit_alert.alert_action.clone(),
                        Some(alert_config.invalid_state_commit_alert.alert_level.clone()),
                    );
                }
            }
            Err(e) => {
                alerts.alert(
                    format!("Failed to check state contract commits: {}", e),
                    alert_config.invalid_state_commit_alert.alert_level.clone(),
                );
                actions.action(
                    alert_config.invalid_state_commit_alert.alert_action.clone(),
                    Some(alert_config.invalid_state_commit_alert.alert_level.clone()),
                );
            }
        }
    }

    *last_commit_check_block = match ethereum_chain.get_latest_block_number().await {
        Ok(block_num) => block_num,
        Err(_) => *last_commit_check_block,
    };

    Ok(())
}

async fn check_base_asset_deposits(
    portal_contract: &PortalContract<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerts,
    watch_config: &EthereumClientWatcher,
    latest_block: &u64,
    actions: &WatchtowerEthereumActions,
) -> Result<(), anyhow::Error> {
    for portal_deposit_alert in &watch_config.portal_deposit_alerts {
        if portal_deposit_alert.alert_level != AlertLevel::None {
            let time_frame = portal_deposit_alert.time_frame;

            let amount = match portal_contract.get_amount_deposited(
                time_frame,
                *latest_block,
            ).await {
                Ok(amt) => amt,
                Err(e) => {
                    alerts.alert(
                        format!("Failed to check base asset deposits: {}", e),
                        portal_deposit_alert.alert_level.clone(),
                    );
                    actions.action(
                        portal_deposit_alert.alert_action.clone(),
                        Some(portal_deposit_alert.alert_level.clone()),
                    );
                    continue; // Skip to the next iteration
                }
            };

            let amount_threshold = get_value(
                portal_deposit_alert.amount,
                18,
            );
            if amount >= amount_threshold {
                alerts.alert(
                    format!(
                        "Base asset deposit threshold of {} over {} seconds has been reached. Amount deposited: {}",
                        amount_threshold, time_frame, amount
                    ),
                    portal_deposit_alert.alert_level.clone(),
                );
                actions.action(
                    portal_deposit_alert.alert_action.clone(),
                    Some(portal_deposit_alert.alert_level.clone()),
                );
            }
        }
    }

    Ok(())
}

async fn check_erc20_token_deposits(
    gateway_contract: &GatewayContract<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerts,
    watch_config: &EthereumClientWatcher,
    last_commit_check_block: u64,
    actions: &WatchtowerEthereumActions,
) -> Result<(), anyhow::Error> {
    for gateway_deposit_alert in &watch_config.gateway_deposit_alerts {
        if gateway_deposit_alert.alert_level != AlertLevel::None {
            let latest_block = last_commit_check_block;
            let amount = match gateway_contract
                .get_amount_deposited(
                    gateway_deposit_alert.time_frame,
                    &gateway_deposit_alert.token_address,
                    latest_block,
                )
                .await
            {
                Ok(amt) => amt,
                Err(e) => {
                    alerts.alert(
                        format!("Failed to check ERC20 deposits: {}", e),
                        gateway_deposit_alert.alert_level.clone(),
                    );
                    actions.action(
                        gateway_deposit_alert.alert_action.clone(),
                        Some(gateway_deposit_alert.alert_level.clone()),
                    );
                    continue;
                }
            };

            let amount_threshold = ethereum_utils::get_value(
                gateway_deposit_alert.amount,
                gateway_deposit_alert.token_decimals,
            );
            if amount >= amount_threshold {
                alerts.alert(
                    format!(
                        "ERC20 deposit threshold of {}{} over {} seconds has been reached. Amount deposited: {}{}",
                        amount_threshold,gateway_deposit_alert.token_name, gateway_deposit_alert.time_frame, amount, gateway_deposit_alert.token_name
                    ),
                    gateway_deposit_alert.alert_level.clone(),
                );
                actions.action(
                    gateway_deposit_alert.alert_action.clone(),
                    Some(gateway_deposit_alert.alert_level.clone()),
                );
            }
        }
    }

    Ok(())
}

pub async fn start_ethereum_watcher(
    config: &WatchtowerConfig,
    actions: WatchtowerEthereumActions,
    alerts: WatchtowerAlerts,
    fuel_chain: FuelChain,
    ethereum_chain: EthereumChain<GasEscalatorMiddleware<Provider<Http>>>,
    state_contract: StateContract<GasEscalatorMiddleware<Provider<Http>>>,
    portal_contract: PortalContract<GasEscalatorMiddleware<Provider<Http>>>,
    gateway_contract: GatewayContract<GasEscalatorMiddleware<Provider<Http>>>,
) -> Result<JoinHandle<()>> {

    let watch_config = config.ethereum_client_watcher.clone();
    let account_address = match &config.ethereum_wallet_key {
        Some(key) => Some(ethereum_utils::get_public_address(key)?),
        None => None,
    };
    let commit_start_block_offset = COMMIT_CHECK_STARTING_OFFSET / ETHEREUM_BLOCK_TIME;
    let mut last_commit_check_block = max(
        ethereum_chain.get_latest_block_number().await?,
        commit_start_block_offset,
    ) - commit_start_block_offset;

    let handle = tokio::spawn(async move {
        loop {
            if let Err(e) = check_chain_connection(
                &ethereum_chain,
                &alerts,
                &watch_config,
            ).await {
                eprintln!("Error in check_chain_connection: {}", e);
            }

            if let Err(e) = check_block_production(
                &ethereum_chain,
                &alerts,
                &watch_config,
            ).await {
                eprintln!("Error in check_block_production: {}", e);
            }

            if let Err(e) = check_account_balance(
                &ethereum_chain,
                &alerts,
                &watch_config,
                &actions,
                &account_address,
            ).await {
                eprintln!("Error in check_account_balance: {}", e);
            }

            // Update last_commit_check_block if necessary
            let mut last_commit_check_block_copy = last_commit_check_block;
            if let Err(e) = check_invalid_commits(
                &state_contract,
                &alerts,
                &watch_config,
                &fuel_chain,
                &mut last_commit_check_block_copy,
                &actions,
                &ethereum_chain,
            ).await {
                eprintln!("Error in check_invalid_commits: {}", e);
            }
            last_commit_check_block = last_commit_check_block_copy;

            if let Err(e) = check_base_asset_deposits(
                &portal_contract,
                &alerts,
                &watch_config,
                &last_commit_check_block,
                &actions,
            ).await {
                eprintln!("Error in check_base_asset_deposits: {}", e);
            }

            if let Err(e) = check_erc20_token_deposits(
                &gateway_contract,
                &alerts,
                &watch_config,
                last_commit_check_block,
                &actions,
            ).await {
                eprintln!("Error in check_erc20_token_deposits: {}", e);
            }

            thread::sleep(POLL_DURATION);
        }
    });

    Ok(handle)
}
