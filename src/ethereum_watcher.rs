use crate::alerter::{AlertLevel, WatchtowerAlerter};
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
    alerts: &WatchtowerAlerter,
    actions: &WatchtowerEthereumActions,
    watch_config: &EthereumClientWatcher,
) {
    if watch_config.connection_alert.alert_level == AlertLevel::None {
        return;
    }

    if let Err(e) = ethereum_chain.check_connection().await {
        alerts.alert(
            format!("Failed to check ethereum connection: {}", e),
            watch_config.connection_alert.alert_level.clone(),
        ).await;
        actions.action(
            watch_config.connection_alert.alert_action.clone(),
            Some(watch_config.connection_alert.alert_level.clone()),
        );
    }
}

async fn check_block_production(
    ethereum_chain: &EthereumChain<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerter,
    actions: &WatchtowerEthereumActions,
    watch_config: &EthereumClientWatcher,
) {

    if watch_config.block_production_alert.alert_level == AlertLevel::None {
        return;
    }

    let seconds_since_last_block = match ethereum_chain.get_seconds_since_last_block().await {
        Ok(seconds) => seconds,
        Err(e) => {
            alerts.alert(
                format!("Failed to check ethereum block production: {}", e),
                watch_config.block_production_alert.alert_level.clone(),
            ).await;
            actions.action(
                watch_config.block_production_alert.alert_action.clone(),
                Some(watch_config.block_production_alert.alert_level.clone()),
            );
            return;
        }
    };

    if seconds_since_last_block > watch_config.block_production_alert.max_block_time {
        alerts.alert(
            format!(
                "Next ethereum block is taking longer than {} seconds. Last block was {} seconds ago.",
                watch_config.block_production_alert.max_block_time, seconds_since_last_block
            ),
            watch_config.block_production_alert.alert_level.clone(),
        ).await;
        actions.action(
            watch_config.block_production_alert.alert_action.clone(),
            Some(watch_config.block_production_alert.alert_level.clone()),
        );
    }
}

async fn check_account_balance(
    ethereum_chain: &EthereumChain<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerter,
    actions: &WatchtowerEthereumActions,
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
    match ethereum_chain.get_account_balance(address).await {
        Ok(balance) => {
            let min_balance = get_value(
                watch_config.account_funds_alert.min_balance,
                18,
            );
            if balance < min_balance {
                alerts.alert(
                    format!(
                        "Ethereum account ({}) is low on funds. Current balance: {}",
                        address, balance,
                    ),
                    watch_config.account_funds_alert.alert_level.clone(),
                ).await;
                actions.action(
                    watch_config.account_funds_alert.alert_action.clone(),
                    Some(watch_config.account_funds_alert.alert_level.clone()),
                );
            }
        }
        Err(e) => {
            alerts.alert(
                format!("Failed to check ethereum account funds: {}", e),
                watch_config.account_funds_alert.alert_level.clone(),
            ).await;
            actions.action(
                watch_config.account_funds_alert.alert_action.clone(),
                Some(watch_config.account_funds_alert.alert_level.clone()),
            );
        }
    }
}

async fn check_invalid_commits(
    state_contract: &StateContract<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerter,
    actions: &WatchtowerEthereumActions,
    watch_config: &EthereumClientWatcher,
    fuel_chain: &FuelChain,
    last_commit_check_block: &mut u64,
    ethereum_chain: &EthereumChain<GasEscalatorMiddleware<Provider<Http>>>,
) {

    if watch_config.account_funds_alert.alert_level == AlertLevel::None {
        return;
    }

    let hashes = match state_contract.get_latest_commits(
        *last_commit_check_block,
    ).await {
        Ok(hashes) => hashes,
        Err(e) => {
            alerts.alert(
                format!("Failed to check state contract commits: {e}"),
                watch_config.invalid_state_commit_alert.alert_level.clone(),
            ).await;
            actions.action(
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
                    alerts.alert(
                        format!(
                            "An invalid commit was made on the state contract. Hash: {}", hash,
                        ),
                        watch_config.invalid_state_commit_alert.alert_level.clone(),
                    ).await;
                    actions.action(
                        watch_config.invalid_state_commit_alert.alert_action.clone(),
                        Some(watch_config.invalid_state_commit_alert.alert_level.clone()),
                    );
                }
            }
            Err(e) => {
                alerts.alert(
                    format!("Failed to check state contract commits: {}", e),
                    watch_config.invalid_state_commit_alert.alert_level.clone(),
                ).await;
                actions.action(
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
    portal_contract: &PortalContract<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerter,
    actions: &WatchtowerEthereumActions,
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
            Ok(amt) => {
                println!("Total ETH deposited: {:?}", amt);
                amt
            },
            Err(e) => {
                alerts.alert(
                    format!("Failed to check base asset deposits: {}", e),
                    portal_deposit_alert.alert_level.clone(),
                ).await;
                actions.action(
                    portal_deposit_alert.alert_action.clone(),
                    Some(portal_deposit_alert.alert_level.clone()),
                );
                continue;
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
            ).await;
            actions.action(
                portal_deposit_alert.alert_action.clone(),
                Some(portal_deposit_alert.alert_level.clone()),
            );
        }
    }
}

async fn check_base_asset_withdrawals(
    portal_contract: &PortalContract<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerter,
    actions: &WatchtowerEthereumActions,
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
            Ok(amt) => {
                println!("Total ETH withdrawn: {:?}", amt);
                amt
            },
            Err(e) => {
                alerts.alert(
                    format!("Failed to check base asset withdrawals: {}", e),
                    portal_withdrawal_alert.alert_level.clone(),
                ).await;
                actions.action(
                    portal_withdrawal_alert.alert_action.clone(),
                    Some(portal_withdrawal_alert.alert_level.clone()),
                );
                continue;
            }
        };

        let amount_threshold = get_value(
            portal_withdrawal_alert.amount,
            18, // Assuming 18 is the decimal precision for ETH
        );
        if amount >= amount_threshold {
            alerts.alert(
                format!(
                    "Base asset withdrawal threshold of {} over {} seconds has been exceeded. Amount withdrawn: {}",
                    amount_threshold, time_frame, amount
                ),
                portal_withdrawal_alert.alert_level.clone(),
            ).await;
            actions.action(
                portal_withdrawal_alert.alert_action.clone(),
                Some(portal_withdrawal_alert.alert_level.clone()),
            );
        }
    }
}

async fn check_token_token_deposits(
    gateway_contract: &GatewayContract<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerter,
    actions: &WatchtowerEthereumActions,
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
            Ok(amt) => {
                println!("Total Tokens deposited: {:?}", amt);
                amt
            },
            Err(e) => {
                alerts.alert(
                    format!("Failed to check ERC20 deposits: {}", e),
                    gateway_deposit_alert.alert_level.clone(),
                ).await;
                actions.action(
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
            alerts.alert(
                format!(
                    "ERC20 deposit threshold of {}{} over {} seconds has been reached. Amount deposited: {}{}",
                    amount_threshold,gateway_deposit_alert.token_name,
                    gateway_deposit_alert.time_frame, amount, gateway_deposit_alert.token_name
                ),
                gateway_deposit_alert.alert_level.clone(),
            ).await;
            actions.action(
                gateway_deposit_alert.alert_action.clone(),
                Some(gateway_deposit_alert.alert_level.clone()),
            );
        }
    }
}

async fn check_token_withdrawals(
    gateway_contract: &GatewayContract<GasEscalatorMiddleware<Provider<Http>>>,
    alerts: &WatchtowerAlerter,
    actions: &WatchtowerEthereumActions,
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
                alerts.alert(
                    format!("Failed to check ERC20 withdrawals: {}", e),
                    gateway_withdrawal_alert.alert_level.clone(),
                ).await;
                actions.action(
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
            alerts.alert(
            format!(
                    "ERC20 withdrawal threshold of {}{} over {} seconds has been exceeded. Amount withdrawn: {}{}",
                    amount_threshold, gateway_withdrawal_alert.token_name,
                    gateway_withdrawal_alert.time_frame, amount, gateway_withdrawal_alert.token_name
                ),
                gateway_withdrawal_alert.alert_level.clone(),
            ).await;
            actions.action(
                gateway_withdrawal_alert.alert_action.clone(),
                Some(gateway_withdrawal_alert.alert_level.clone()),
            );
        }
    }
}

pub async fn start_ethereum_watcher(
    config: &WatchtowerConfig,
    actions: WatchtowerEthereumActions,
    alerts: WatchtowerAlerter,
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

            alerts.alert(String::from("Watching ethereum chain."), AlertLevel::Info).await;
            for _ in 0..POLL_LOGGING_SKIP {

                check_chain_connection(&ethereum_chain, &alerts, &actions, &watch_config).await;

                check_block_production(&ethereum_chain, &alerts, &actions, &watch_config).await;

                check_account_balance(&ethereum_chain, &alerts, &actions, &watch_config,
                                      &account_address).await;

                check_invalid_commits(&state_contract, &alerts, &actions, &watch_config,
                                      &fuel_chain, &mut last_commit_check_block,
                                      &ethereum_chain).await;

                check_base_asset_deposits(&portal_contract, &alerts, &actions, &watch_config,
                                          &last_commit_check_block).await;

                check_base_asset_withdrawals(&portal_contract, &alerts, &actions, &watch_config,
                                             &last_commit_check_block).await;

                check_token_token_deposits(&gateway_contract, &alerts,  &actions, &watch_config,
                                           last_commit_check_block).await;

                check_token_withdrawals(&gateway_contract, &alerts, &actions, &watch_config,
                                        last_commit_check_block).await;

                thread::sleep(POLL_DURATION);
            }
        }
    });

    Ok(handle)
}
