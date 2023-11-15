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

    // start thread
    let handle = tokio::spawn(async move {
        loop {
            // update the log every so often to notify that everything is working
            alerts.alert(String::from("Watching ethereum chain."), AlertLevel::Info);
            for _i in 0..POLL_LOGGING_SKIP {
                // check chain connection
                if watch_config.connection_alert.alert_level != AlertLevel::None {
                    match ethereum_chain.check_connection().await {
                        Ok(_) => {}
                        Err(e) => {
                            alerts.alert(
                                format!("Failed to check ethereum connection: {e}"),
                                watch_config.connection_alert.alert_level.clone(),
                            );
                            actions.action(
                                watch_config.connection_alert.alert_action.clone(),
                                Some(watch_config.connection_alert.alert_level.clone()),
                            );
                        }
                    }
                }

                // check block production
                if watch_config.block_production_alert.alert_level != AlertLevel::None {
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
                                actions.action(
                                    watch_config.block_production_alert.alert_action.clone(),
                                    Some(watch_config.block_production_alert.alert_level.clone()),
                                );
                            }
                        }
                        Err(e) => {
                            alerts.alert(
                                format!("Failed to check ethereum block production: {e}"),
                                watch_config.connection_alert.alert_level.clone(),
                            );
                            actions.action(
                                watch_config.connection_alert.alert_action.clone(),
                                Some(watch_config.connection_alert.alert_level.clone()),
                            );
                        }
                    }
                }

                // check account balance
                let account_address = account_address.clone();
                if account_address.is_some() && watch_config.account_funds_alert.alert_level != AlertLevel::None {
                    let account_address = account_address.unwrap();
                    match ethereum_chain.get_account_balance(&account_address).await {
                        Ok(balance) => {
                            let min_balance =
                                ethereum_utils::get_value(watch_config.account_funds_alert.min_balance, 18);
                            if balance < min_balance {
                                alerts.alert(
                                    format!(
                                        "Ethereum account ({}) is low on funds. Current balance: {}",
                                        &account_address, balance
                                    ),
                                    watch_config.account_funds_alert.alert_level.clone(),
                                );
                                actions.action(
                                    watch_config.account_funds_alert.alert_action.clone(),
                                    Some(watch_config.account_funds_alert.alert_level.clone()),
                                );
                            }
                        }
                        Err(e) => {
                            alerts.alert(
                                format!("Failed to check ethereum account funds: {e}"),
                                watch_config.account_funds_alert.alert_level.clone(),
                            );
                            actions.action(
                                watch_config.account_funds_alert.alert_action.clone(),
                                Some(watch_config.account_funds_alert.alert_level.clone()),
                            );
                        }
                    }
                }

                // check invalid commits
                if watch_config.invalid_state_commit_alert.alert_level != AlertLevel::None {
                    match state_contract.get_latest_commits(last_commit_check_block).await {
                        Ok(hashes) => {
                            for hash in hashes {
                                match fuel_chain.verify_block_commit(&hash).await {
                                    Ok(valid) => {
                                        if !valid {
                                            alerts.alert(
                                                format!("An invalid commit was made on the state contract. Hash: {hash}"),
                                                watch_config.invalid_state_commit_alert.alert_level.clone(),
                                            );
                                            actions.action(
                                                watch_config.invalid_state_commit_alert.alert_action.clone(),
                                                Some(watch_config.invalid_state_commit_alert.alert_level.clone()),
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        alerts.alert(
                                            format!("Failed to check state contract commits: {e}"),
                                            watch_config.invalid_state_commit_alert.alert_level.clone(),
                                        );
                                        actions.action(
                                            watch_config.invalid_state_commit_alert.alert_action.clone(),
                                            Some(watch_config.invalid_state_commit_alert.alert_level.clone()),
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            alerts.alert(
                                format!("Failed to check state contract commits: {e}"),
                                watch_config.invalid_state_commit_alert.alert_level.clone(),
                            );
                            actions.action(
                                watch_config.invalid_state_commit_alert.alert_action.clone(),
                                Some(watch_config.invalid_state_commit_alert.alert_level.clone()),
                            );
                        }
                    }
                    last_commit_check_block = match ethereum_chain.get_latest_block_number().await {
                        Ok(block_num) => block_num,
                        Err(_) => last_commit_check_block,
                    };
                }

                // check base asset deposits
                for portal_deposit_alert in &watch_config.portal_deposit_alerts {
                    if portal_deposit_alert.alert_level != AlertLevel::None {
                        let latest_block = last_commit_check_block;
                        let time_frame = portal_deposit_alert.time_frame;
                        match portal_contract.get_amount_deposited(
                            time_frame,
                            latest_block,
                        ).await {
                            Ok(amount) => {
                                println!("Total ETH deposited: {:?}", amount);
                                let amount_threshold = ethereum_utils::get_value(portal_deposit_alert.amount, 18);
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
                            Err(e) => {
                                alerts.alert(
                                    format!("Failed to check base asset deposits: {e}"),
                                    portal_deposit_alert.alert_level.clone(),
                                );
                                actions.action(
                                    portal_deposit_alert.alert_action.clone(),
                                    Some(portal_deposit_alert.alert_level.clone()),
                                );
                            }
                        }
                    }
                }

                // check ERC20 token deposits
                for gateway_deposit_alert in &watch_config.gateway_deposit_alerts {
                    if gateway_deposit_alert.alert_level != AlertLevel::None {
                        let latest_block = last_commit_check_block;
                        match gateway_contract
                            .get_amount_deposited(
                                gateway_deposit_alert.time_frame,
                                &gateway_deposit_alert.token_address,
                                latest_block,
                            )
                            .await
                        {
                            Ok(amount) => {
                                println!("Total Tokens deposited: {:?}", amount);
                                let amount_threshold = ethereum_utils::get_value(
                                    gateway_deposit_alert.amount,
                                    gateway_deposit_alert.token_decimals,
                                );
                                if amount >= amount_threshold {
                                    alerts.alert(
                                        format!(
                                            "ERC20 deposit threshold of {}{} over {} seconds has been reached. Amount deposited: {}{}",
                                            amount_threshold, gateway_deposit_alert.token_name, gateway_deposit_alert.time_frame, amount, gateway_deposit_alert.token_name
                                        ),
                                        gateway_deposit_alert.alert_level.clone(),
                                    );
                                    actions.action(
                                        gateway_deposit_alert.alert_action.clone(),
                                        Some(gateway_deposit_alert.alert_level.clone()),
                                    );
                                }
                            }
                            Err(e) => {
                                alerts.alert(
                                    format!("Failed to check ERC20 deposits: {e}"),
                                    gateway_deposit_alert.alert_level.clone(),
                                );
                                actions.action(
                                    gateway_deposit_alert.alert_action.clone(),
                                    Some(gateway_deposit_alert.alert_level.clone()),
                                );
                            }
                        }
                    }
                }

                thread::sleep(POLL_DURATION);
            }
        }
    });

    Ok(handle)
}
