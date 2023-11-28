use crate::alerter::{AlertLevel, AlertParams, send_alert};
use crate::ethereum_actions::{ActionParams, send_action};
use crate::WatchtowerConfig;
use crate::config::FuelClientWatcher;
use crate::fuel_watcher::fuel_utils::get_value;

use anyhow::Result;
use fuel_chain::FuelChainTrait;

use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::sync::mpsc::UnboundedSender;

pub mod fuel_chain;
pub mod fuel_utils;

pub static POLL_DURATION: Duration = Duration::from_millis(6000);
pub static POLL_LOGGING_SKIP: u64 = 50;
pub static FUEL_CONNECTION_RETRIES: u64 = 2;
pub static FUEL_BLOCK_TIME: u64 = 1;

async fn check_fuel_chain_connection(
    fuel_chain: Arc<dyn FuelChainTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &FuelClientWatcher,
) {
    if watch_config.connection_alert.alert_level == AlertLevel::None {
        return;
    }

    if let Err(e) = fuel_chain.check_connection().await {
        send_alert(
            &alert_sender,
            String::from("Failed to check fuel connection"),
            format!("Failed to check fuel connection: {}", e),
            watch_config.connection_alert.alert_level.clone(),
        );
        send_action(
            &action_sender,
            watch_config.connection_alert.alert_action.clone(),
            Some(watch_config.connection_alert.alert_level.clone()),
        );
    }
}

async fn check_fuel_block_production(
    fuel_chain: Arc<dyn FuelChainTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &FuelClientWatcher,
) {
    if watch_config.block_production_alert.alert_level == AlertLevel::None {
        return;
    }

    let seconds_since_last_block = match fuel_chain.get_seconds_since_last_block().await {
        Ok(seconds) => seconds,
        Err(e) => {
            send_alert(
                &alert_sender,
                String::from("Failed to check fuel block production"),
                format!("Failed to check fuel block production: {}", e),
                watch_config.block_production_alert.alert_level.clone(),
            );
            send_action(
                &action_sender,
                watch_config.block_production_alert.alert_action.clone(),
                Some(watch_config.block_production_alert.alert_level.clone()),
            );
            return
        }
    };

    if seconds_since_last_block > watch_config.block_production_alert.max_block_time {
        send_alert(
            &alert_sender,
            String::from("Fuel block is taking long."),
            format!(
                "Next fuel block is taking longer than {} seconds. Last block was {} seconds ago.",
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

async fn check_fuel_base_asset_withdrawals(
    fuel_chain: Arc<dyn FuelChainTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &FuelClientWatcher,
) {
    for portal_withdraw_alert in &watch_config.portal_withdraw_alerts {
        if portal_withdraw_alert.alert_level == AlertLevel::None {
            continue;
        }
        let time_frame = portal_withdraw_alert.time_frame;
        let amount = match fuel_chain.get_base_amount_withdrawn(time_frame).await {
            Ok(amt) => {
                println!("Total Base Asset Withdrawn {} for time frame {}", amt, time_frame);
                amt
            },
            Err(e) => {
                send_alert(
                    &alert_sender,
                    String::from("Failed to check fuel chain for base asset withdrawals"),
                    format!("Failed to check base asset withdrawals: {}", e),
                    portal_withdraw_alert.alert_level.clone(),
                );
                send_action(
                    &action_sender,
                    portal_withdraw_alert.alert_action.clone(),
                    Some(portal_withdraw_alert.alert_level.clone()),
                );
                continue;
            }
        };

        let amount_threshold = get_value(
            portal_withdraw_alert.amount,
            portal_withdraw_alert.token_decimals,
        );
        if amount >= amount_threshold {
            send_alert(
                &alert_sender,
                String::from("Base asset is above withdrawal threshold"),
                format!(
                    "Base asset withdraw threshold of {} over {} seconds has been reached. Amount withdrawn: {}",
                    amount_threshold, time_frame, amount
                ),
                portal_withdraw_alert.alert_level.clone(),
            );
            send_action(
                &action_sender,
                portal_withdraw_alert.alert_action.clone(),
                Some(portal_withdraw_alert.alert_level.clone()),
            );
        }
    }
}

async fn check_fuel_token_withdrawals(
    fuel_chain: Arc<dyn FuelChainTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &FuelClientWatcher,
) {
    for gateway_withdraw_alert in &watch_config.gateway_withdraw_alerts {
        if gateway_withdraw_alert.alert_level == AlertLevel::None {
            continue;
        }

        let amount = match fuel_chain
            .get_token_amount_withdrawn(
                gateway_withdraw_alert.time_frame,
                &gateway_withdraw_alert.token_address,
            )
            .await
        {
            Ok(amt) => amt,
            Err(e) => {
                send_alert(
                    &alert_sender,
                    format!(
                        "Failed to check fuel chain for ERC20 {} withdrawals at address {}",
                        gateway_withdraw_alert.token_name, gateway_withdraw_alert.token_address,
                    ),
                    format!("Failed to check ERC20 withdrawals: {}", e),
                    gateway_withdraw_alert.alert_level.clone(),
                );
                send_action(
                    &action_sender,
                    gateway_withdraw_alert.alert_action.clone(),
                    Some(gateway_withdraw_alert.alert_level.clone()),
                );
                continue;
            }
        };

        let amount_threshold = get_value(
            gateway_withdraw_alert.amount,
            gateway_withdraw_alert.token_decimals,
        );

        if amount >= amount_threshold {
            send_alert(
                &alert_sender,
            format!(
                    "ERC20 {} at address {} is above withdrawal threshold",
                    gateway_withdraw_alert.token_name, 
                    gateway_withdraw_alert.token_address,
                ),
                format!(
                    "ERC20 withdraw threshold of {}{} over {} seconds has been reached. Amount withdrawn: {}{}",
                    amount_threshold, gateway_withdraw_alert.token_name,
                    gateway_withdraw_alert.time_frame, amount, gateway_withdraw_alert.token_name
                ),
                gateway_withdraw_alert.alert_level.clone(),
            );
            send_action(
                &action_sender,
                gateway_withdraw_alert.alert_action.clone(),
                Some(gateway_withdraw_alert.alert_level.clone()),
            );
        }
    }
}

pub async fn start_fuel_watcher(
    config: &WatchtowerConfig,
    fuel_chain: Arc<dyn FuelChainTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
) -> Result<JoinHandle<()>> {
    let watch_config = config.fuel_client_watcher.clone();
    let handle = tokio::spawn(async move {
        loop {
            for _ in 0..POLL_LOGGING_SKIP {
                // update the log every so often to notify that everything is working
                send_alert(
                    &alert_sender.clone(),
                    String::from("Watching fuel chain."),
                    String::from("Periodically querying the fuel chain."),
                    AlertLevel::Info,
                );

                check_fuel_chain_connection(fuel_chain.clone(), action_sender.clone(),
                                            alert_sender.clone(), &watch_config).await;

                check_fuel_block_production(fuel_chain.clone(), action_sender.clone(),
                                            alert_sender.clone(), &watch_config).await;

                check_fuel_base_asset_withdrawals(fuel_chain.clone(), action_sender.clone(),
                                                  alert_sender.clone(), &watch_config).await;

                // check_fuel_token_withdrawals(fuel_chain.clone(), action_sender.clone(),
                //                              alert_sender.clone(), &watch_config).await;

                thread::sleep(POLL_DURATION);
            }
        }
    });

    Ok(handle)
}
