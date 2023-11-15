use crate::alerts::{AlertLevel, WatchtowerAlerts};
use crate::ethereum_actions::WatchtowerEthereumActions;
use crate::WatchtowerConfig;

use anyhow::Result;
use fuel_chain::FuelChain;
use std::thread;
use std::time::Duration;

use tokio::task::JoinHandle;

pub mod fuel_chain;
pub mod fuel_utils;

pub static POLL_DURATION: Duration = Duration::from_millis(4000);
pub static POLL_LOGGING_SKIP: u64 = 75;
pub static FUEL_CONNECTION_RETRIES: u64 = 2;
pub static FUEL_BLOCK_TIME: u64 = 1;

pub async fn start_fuel_watcher(
    config: &WatchtowerConfig,
    actions: WatchtowerEthereumActions,
    alerts: WatchtowerAlerts,
    fuel_chain: FuelChain,
) -> Result<JoinHandle<()>> {

    // let fungible_token_contract = FungibleTokenContract::new(config).await?;
    let watch_config = config.fuel_client_watcher.clone();

    // start thread
    let handle = tokio::spawn(async move {
        loop {
            // update the log every so often to notify that everything is working
            alerts.alert(String::from("Watching fuel chain."), AlertLevel::Info);
            for _i in 0..POLL_LOGGING_SKIP {
                // check chain connection
                if watch_config.connection_alert.alert_level != AlertLevel::None {
                    match fuel_chain.check_connection().await {
                        Ok(_) => {}
                        Err(e) => {
                            alerts.alert(
                                format!("Failed to check fuel connection: {e}"),
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
                    match fuel_chain.get_seconds_since_last_block().await {
                        Ok(seconds_since_last_block) => {
                            if seconds_since_last_block > watch_config.block_production_alert.max_block_time {
                                alerts.alert(
                                    format!(
                                        "Next fuel block is taking longer than {} seconds. Last block was {} seconds ago.",
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
                                format!("Failed to check fuel block production: {e}"),
                                watch_config.connection_alert.alert_level.clone(),
                            );
                            actions.action(
                                watch_config.connection_alert.alert_action.clone(),
                                Some(watch_config.connection_alert.alert_level.clone()),
                            );
                        }
                    }
                }

                // check base asset withdrawals
                for portal_withdraw_alert in &watch_config.portal_withdraw_alerts {
                    if portal_withdraw_alert.alert_level != AlertLevel::None {
                        let time_frame = portal_withdraw_alert.time_frame;
                        match fuel_chain.get_amount_withdrawn(time_frame).await {
                            Ok(amount) => {
                                let amount_threshold = FuelChain::get_value(portal_withdraw_alert.amount, 9);
                                if amount >= amount_threshold {
                                    alerts.alert(
                                        format!(
                                            "Base asset withdraw threshold of {} over {} seconds has been reached. Amount withdrawn: {}",
                                            amount_threshold, time_frame, amount
                                        ),
                                        portal_withdraw_alert.alert_level.clone(),
                                    );
                                    actions.action(
                                        portal_withdraw_alert.alert_action.clone(),
                                        Some(portal_withdraw_alert.alert_level.clone()),
                                    );
                                }
                            }
                            Err(e) => {
                                alerts.alert(
                                    format!("Failed to check base asset withdrawals: {e}"),
                                    portal_withdraw_alert.alert_level.clone(),
                                );
                                actions.action(
                                    portal_withdraw_alert.alert_action.clone(),
                                    Some(portal_withdraw_alert.alert_level.clone()),
                                );
                            }
                        }
                    }
                }

                // // check ERC20 token withdrawals
                // for gateway_withdraw_alert in &watch_config.gateway_withdraw_alerts {
                //     if gateway_withdraw_alert.alert_level != AlertLevel::None {
                //         match fungible_token_contract
                //             .get_amount_withdrawn(
                //                 gateway_withdraw_alert.time_frame,
                //                 &gateway_withdraw_alert.token_address,
                //             )
                //             .await
                //         {
                //             Ok(amount) => {
                //                 let amount_threshold = FuelChain::get_value(
                //                     gateway_withdraw_alert.amount,
                //                     gateway_withdraw_alert.token_decimals,
                //                 );
                //                 if amount >= amount_threshold {
                //                     alerts.alert(
                //                         format!(
                //                             "ERC20 withdraw threshold of {}{} over {} seconds has been reached. Amount withdrawn: {}{}",
                //                             amount_threshold, gateway_withdraw_alert.token_name, gateway_withdraw_alert.time_frame, amount, gateway_withdraw_alert.token_name
                //                         ),
                //                         gateway_withdraw_alert.alert_level.clone(),
                //                     );
                //                     actions.action(
                //                         gateway_withdraw_alert.alert_action.clone(),
                //                         Some(gateway_withdraw_alert.alert_level.clone()),
                //                     );
                //                 }
                //             }
                //             Err(e) => {
                //                 alerts.alert(
                //                     format!("Failed to check ERC20 withdrawals: {e}"),
                //                     gateway_withdraw_alert.alert_level.clone(),
                //                 );
                //                 actions.action(
                //                     gateway_withdraw_alert.alert_action.clone(),
                //                     Some(gateway_withdraw_alert.alert_level.clone()),
                //                 );
                //             }
                //         }
                //     }
                // }

                thread::sleep(POLL_DURATION);
            }
        }
    });

    Ok(handle)
}
