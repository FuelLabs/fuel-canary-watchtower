use crate::alerter::{AlertLevel, AlertParams, send_alert};
use crate::ethereum_actions::{ActionParams, send_action};
use crate::fuel_watcher::fuel_chain::{FuelChainTrait};
use crate::WatchtowerConfig;


use anyhow::Result;
use tokio::sync::mpsc::UnboundedSender;
use std::cmp::max;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::task::JoinHandle;

use crate::config::EthereumClientWatcher;
use crate::ethereum_watcher::ethereum_utils::get_value;

use gateway_contract::GatewayContractTrait;
use portal_contract::PortalContractTrait;
use state_contract::StateContractTrait;

use ethereum_chain::EthereumChainTrait;

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
    ethereum_chain: Arc<dyn EthereumChainTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &EthereumClientWatcher,
) {
    if watch_config.connection_alert.alert_level == AlertLevel::None {
        return;
    }

    if let Err(e) = ethereum_chain.check_connection().await {
        send_alert(
            &alert_sender,
            String::from("Failed to check ethereum connection"),
            format!("Failed to check ethereum connection: {}", e),
            watch_config.connection_alert.alert_level.clone(),
        );
        send_action(
            &action_sender,
            watch_config.connection_alert.alert_action.clone(),
            Some(watch_config.connection_alert.alert_level.clone()),
        );
    }
}

async fn check_block_production(
    ethereum_chain: Arc<dyn EthereumChainTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &EthereumClientWatcher,
) {

    if watch_config.block_production_alert.alert_level == AlertLevel::None {
        return;
    }

    let seconds_since_last_block = match ethereum_chain.get_seconds_since_last_block().await {
        Ok(seconds) => seconds,
        Err(e) => {
            send_alert(
                &alert_sender,
                    String::from("Failed to check ethereum block"),
                format!("Failed to check ethereum block production: {}", e),
                watch_config.block_production_alert.alert_level.clone(),
            );
            send_action(
                &action_sender,
                watch_config.block_production_alert.alert_action.clone(),
                Some(watch_config.block_production_alert.alert_level.clone()),
            );
            return;
        }
    };

    if seconds_since_last_block > watch_config.block_production_alert.max_block_time {
        send_alert(
            &alert_sender,
                String::from("Ethereum block is taking long"),
        format!(
                "Next ethereum block is taking longer than {} seconds. Last block was {} seconds ago.",
                watch_config.block_production_alert.max_block_time, seconds_since_last_block
            ),
            watch_config.block_production_alert.alert_level.clone(),
        );
        send_action(
            &action_sender,
            watch_config.block_production_alert.alert_action.clone(),
            Some(watch_config.block_production_alert.alert_level.clone()),
        );
    }
}

async fn check_account_balance(
    ethereum_chain: Arc<dyn EthereumChainTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &EthereumClientWatcher,
    account_address: &Option<String>,
) {

    // Return early if there's no address or if the alert level is None.
    let address = match account_address {
        Some(addr) => addr,
        None => return,
    };

    if watch_config.account_funds_alert.alert_level == AlertLevel::None {
        return;
    }

    // Proceed with checking the account balance
    let retrieved_balance = match ethereum_chain.get_account_balance(address).await {
        Ok(balance) => balance,
        Err(e) => {
            send_alert(
                &alert_sender,
                String::from("Failed to check ethereum account funds"),
                format!("Failed to check ethereum account funds: {}", e),
                watch_config.account_funds_alert.alert_level.clone(),
            );
            send_action(
                &action_sender,
                watch_config.account_funds_alert.alert_action.clone(),
                Some(watch_config.account_funds_alert.alert_level.clone()),
            );
            return;
        }
    };

    let min_balance = get_value(
        watch_config.account_funds_alert.min_balance,
        18,
    );
    if retrieved_balance < min_balance {
        send_alert(
            &alert_sender,
            String::from("Ethereum account low on funds"),
            format!(
                "Ethereum account ({}) is low on funds. Current balance: {}",
                address, retrieved_balance,
            ),
            watch_config.account_funds_alert.alert_level.clone(),
        );
        send_action(
            &action_sender,
            watch_config.account_funds_alert.alert_action.clone(),
            Some(watch_config.account_funds_alert.alert_level.clone()),
        );
    }
}

async fn check_invalid_commits(
    ethereum_chain: Arc<dyn EthereumChainTrait>,
    state_contract: Arc<dyn StateContractTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &EthereumClientWatcher,
    fuel_chain: Arc<dyn FuelChainTrait>,
    last_commit_check_block: &mut u64,
) {

    if watch_config.account_funds_alert.alert_level == AlertLevel::None {
        return;
    }

    let hashes = match state_contract.get_latest_commits(
        *last_commit_check_block,
    ).await {
        Ok(hashes) => hashes,
        Err(e) => {
            send_alert(
                &alert_sender,
                String::from("Failed to check state contract"),
                format!("Failed to check state contract commits: {e}"),
                watch_config.invalid_state_commit_alert.alert_level.clone(),
            );
            send_action(
                &action_sender,
                watch_config.invalid_state_commit_alert.alert_action.clone(),
                Some(watch_config.invalid_state_commit_alert.alert_level.clone()),
            );
            return;
        },
    };

    for hash in hashes {
        match fuel_chain.verify_block_commit(&hash).await {
            Ok(valid) => {
                if !valid {
                    send_alert(
                        &alert_sender,
                        String::from("Invalid commit was made on the state contract"),
                        format!(
                            "An invalid commit was made on the state contract. Hash: {}", hash,
                        ),
                        watch_config.invalid_state_commit_alert.alert_level.clone(),
                    );
                    send_action(
                        &action_sender,
                        watch_config.invalid_state_commit_alert.alert_action.clone(),
                        Some(watch_config.invalid_state_commit_alert.alert_level.clone()),
                    );
                }
            }
            Err(e) => {
                send_alert(
                    &alert_sender,
                    String::from("Failed to check fuel chain state commit contract"),
                    format!("Failed to check fuel chain state commit: {}", e),
                    watch_config.invalid_state_commit_alert.alert_level.clone(),
                );
                send_action(
                    &action_sender,
                    watch_config.invalid_state_commit_alert.alert_action.clone(),
                    Some(watch_config.invalid_state_commit_alert.alert_level.clone()),
                );
            }
        }
    }

    *last_commit_check_block = match ethereum_chain.get_latest_block_number().await {
        Ok(block_num) => block_num,
        Err(_) => *last_commit_check_block,
    };
}

async fn check_base_asset_deposits(
    portal_contract: Arc<dyn PortalContractTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &EthereumClientWatcher,
    last_commit_check_block: &u64,
) {
    for portal_deposit_alert in &watch_config.portal_deposit_alerts {
        if portal_deposit_alert.alert_level == AlertLevel::None {
            continue;
        }

        let time_frame = portal_deposit_alert.time_frame;
        let amount = match portal_contract.get_base_amount_deposited(
            time_frame,
            *last_commit_check_block,
        ).await {
            Ok(amt) => amt,
            Err(e) => {
                send_alert(
                    &alert_sender,
                        String::from("Failed to check portal contract for base asset deposits"),
                    format!("Failed to check base asset deposits: {}", e),
                    portal_deposit_alert.alert_level.clone(),
                );
                send_action(
                    &action_sender,
                    portal_deposit_alert.alert_action.clone(),
                    Some(portal_deposit_alert.alert_level.clone()),
                );
                continue;
            }
        };

        let amount_threshold = get_value(
            portal_deposit_alert.amount,
            portal_deposit_alert.token_decimals,
        );
        if amount >= amount_threshold {
            send_alert(
                &alert_sender,
                    String::from("Base asset is above deposit threshold."),
                format!(
                    "Base asset deposit threshold of {} over {} seconds has been reached. Amount deposited: {}",
                    amount_threshold, time_frame, amount
                ),
                portal_deposit_alert.alert_level.clone(),
            );
            send_action(
                &action_sender,
                portal_deposit_alert.alert_action.clone(),
                Some(portal_deposit_alert.alert_level.clone()),
            );
        }
    }
}

async fn check_base_asset_withdrawals(
    portal_contract: Arc<dyn PortalContractTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &EthereumClientWatcher,
    last_commit_check_block: &u64,
) {
    for portal_withdrawal_alert in &watch_config.portal_withdrawal_alerts {
        if portal_withdrawal_alert.alert_level == AlertLevel::None {
            continue;
        }

        let time_frame = portal_withdrawal_alert.time_frame;
        let amount = match portal_contract.get_base_amount_withdrawn(
            time_frame,
            *last_commit_check_block,
        ).await {
            Ok(amt) => amt,
            Err(e) => {
                send_alert(
                    &alert_sender,
                    String::from("Failed to check portal contract for base asset withdrawals"),
                    format!("Failed to check base asset withdrawals: {}", e),
                    portal_withdrawal_alert.alert_level.clone(),
                );
                send_action(
                    &action_sender,
                    portal_withdrawal_alert.alert_action.clone(),
                    Some(portal_withdrawal_alert.alert_level.clone()),
                );
                continue;
            }
        };

        let amount_threshold = get_value(
            portal_withdrawal_alert.amount,
            portal_withdrawal_alert.token_decimals,
        );
        if amount >= amount_threshold {
            send_alert(
                &alert_sender,
                String::from("Base asset is above withdrawal threshold."),
                format!(
                    "Base asset withdrawal threshold of {} over {} seconds has been exceeded. Amount withdrawn: {}",
                    amount_threshold, time_frame, amount
                ),
                portal_withdrawal_alert.alert_level.clone(),
            );
            send_action(
                &action_sender,
                portal_withdrawal_alert.alert_action.clone(),
                Some(portal_withdrawal_alert.alert_level.clone()),
            );
        }
    }
}

async fn check_token_token_deposits(
    gateway_contract: Arc<dyn GatewayContractTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &EthereumClientWatcher,
    last_commit_check_block: u64,
) {
    for gateway_deposit_alert in &watch_config.gateway_deposit_alerts {

        // Skip iterations where alert level is None
        if gateway_deposit_alert.alert_level == AlertLevel::None {
            continue;
        }

        let latest_block = last_commit_check_block;
        let amount = match gateway_contract
            .get_token_amount_deposited(
                gateway_deposit_alert.time_frame,
                &gateway_deposit_alert.token_address,
                latest_block,
            )
            .await
        {
            Ok(amt) => amt,
            Err(e) => {
                send_alert(
                    &alert_sender,
                    format!(
                            "Failed to check ERC20 deposits {} at address {}",
                            gateway_deposit_alert.token_name, 
                            gateway_deposit_alert.token_address,
                        ),
                    format!("Failed to check ERC20 deposits: {}", e),
                    gateway_deposit_alert.alert_level.clone(),
                );
                send_action(
                    &action_sender,
                    gateway_deposit_alert.alert_action.clone(),
                    Some(gateway_deposit_alert.alert_level.clone()),
                );
                continue;
            }
        };

        let amount_threshold = get_value(
            gateway_deposit_alert.amount,
            gateway_deposit_alert.token_decimals,
        );
        if amount >= amount_threshold {
            send_alert(
                &alert_sender,
                format!(
                        "ERC20 {} at address {} is above deposit threshold",
                        gateway_deposit_alert.token_name, 
                        gateway_deposit_alert.token_address,
                    ),
                    format!(
                        "ERC20 deposit threshold of {}{} over {} seconds has been reached. Amount deposited: {}{}",
                        amount_threshold, gateway_deposit_alert.token_name,
                        gateway_deposit_alert.time_frame, amount, gateway_deposit_alert.token_name
                    ),
            gateway_deposit_alert.alert_level.clone(),
            );
            send_action(
                &action_sender,
                gateway_deposit_alert.alert_action.clone(),
                Some(gateway_deposit_alert.alert_level.clone()),
            );
        }
    }
}

async fn check_token_withdrawals(
    gateway_contract: Arc<dyn GatewayContractTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &EthereumClientWatcher,
    last_commit_check_block: u64,
) {
    for gateway_withdrawal_alert in &watch_config.gateway_withdrawal_alerts {
        if gateway_withdrawal_alert.alert_level == AlertLevel::None {
            continue;
        }

        let latest_block = last_commit_check_block;
        let amount = match gateway_contract
            .get_token_amount_withdrawn(
                gateway_withdrawal_alert.time_frame,
                &gateway_withdrawal_alert.token_address,
                latest_block,
            )
            .await
        {
            Ok(amt) => amt,
            Err(e) => {
                send_alert(
                    &alert_sender,
                    format!(
                            "Failed to check ERC20 withdrawals {} at address {}",
                            gateway_withdrawal_alert.token_name, 
                            gateway_withdrawal_alert.token_address,
                        ),
                    format!("Failed to check ERC20 withdrawals: {}", e),
                    gateway_withdrawal_alert.alert_level.clone(),
                );
                send_action(
                    &action_sender,
                    gateway_withdrawal_alert.alert_action.clone(),
                    Some(gateway_withdrawal_alert.alert_level.clone()),
                );
                continue;
            }
        };

        let amount_threshold = get_value(
            gateway_withdrawal_alert.amount,
            gateway_withdrawal_alert.token_decimals,
        );
        if amount >= amount_threshold {
            send_alert(
                &alert_sender,
                format!(
                        "ERC20 {} at address {} is above withdrawal threshold",
                        gateway_withdrawal_alert.token_name, 
                        gateway_withdrawal_alert.token_address,
                    ),
                    format!(
                        "ERC20 withdrawal threshold of {}{} over {} seconds has been reached. Amount withdrawn: {}{}",
                        amount_threshold, gateway_withdrawal_alert.token_name,
                        gateway_withdrawal_alert.time_frame, amount, gateway_withdrawal_alert.token_name
                    ),
                    gateway_withdrawal_alert.alert_level.clone(),
            );
            send_action(
                &action_sender,
                gateway_withdrawal_alert.alert_action.clone(),
                Some(gateway_withdrawal_alert.alert_level.clone()),
            );
        }
    }
}

pub async fn start_ethereum_watcher(
    config: &WatchtowerConfig,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    fuel_chain: Arc<dyn FuelChainTrait>,
    ethereum_chain: Arc<dyn EthereumChainTrait>,
    state_contract: Arc<dyn StateContractTrait>,
    portal_contract: Arc<dyn PortalContractTrait>,
    gateway_contract: Arc<dyn GatewayContractTrait>,
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
            for _ in 0..POLL_LOGGING_SKIP {

                send_alert(
                    &alert_sender.clone(),
                    String::from("Watching ethereum chain."),
                    String::from("Starting to periodically query the ethereum chain."),
                    AlertLevel::Info,
                );

                check_chain_connection(ethereum_chain.clone(), action_sender.clone(),
                                        alert_sender.clone(), &watch_config).await;

                check_block_production(ethereum_chain.clone(), action_sender.clone(),
                                        alert_sender.clone(), &watch_config).await;

                check_account_balance(ethereum_chain.clone(), action_sender.clone(),
                                      alert_sender.clone(), &watch_config, &account_address).await;

                check_invalid_commits(ethereum_chain.clone(), state_contract.clone(), action_sender.clone(),
                                        alert_sender.clone(), &watch_config, fuel_chain.clone(), 
                                        &mut last_commit_check_block).await;

                check_base_asset_deposits(portal_contract.clone(), action_sender.clone(), alert_sender.clone(),
                                            &watch_config, &last_commit_check_block).await;

                check_base_asset_withdrawals(portal_contract.clone(), action_sender.clone(), alert_sender.clone(),
                                                &watch_config, &last_commit_check_block).await;

                check_token_token_deposits(gateway_contract.clone(), action_sender.clone(), alert_sender.clone(),
                                             &watch_config, last_commit_check_block).await;

                check_token_withdrawals(gateway_contract.clone(), action_sender.clone(), alert_sender.clone(),
                                        &watch_config, last_commit_check_block).await;

                thread::sleep(POLL_DURATION);
            }
        }
    });

    Ok(handle)
}
