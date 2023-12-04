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
pub mod extended_provider;

pub static POLL_DURATION: Duration = Duration::from_millis(4000);
pub static FUEL_CONNECTION_RETRIES: u64 = 2;
pub static FUEL_BLOCK_TIME: u64 = 1;

async fn check_fuel_chain_connection(
    fuel_chain: &Arc<dyn FuelChainTrait>,
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
    fuel_chain: &Arc<dyn FuelChainTrait>,
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
            String::from("Fuel block is taking long"),
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
    fuel_chain: &Arc<dyn FuelChainTrait>,
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
                println!(
                    "Fuel Chain: Total Base Asset Withdrawn {} for time frame {}",
                    amt,
                    time_frame,
                );
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
                String::from("Fuel Chain: Base asset is above withdrawal threshold"),
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
    fuel_chain: &Arc<dyn FuelChainTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
    watch_config: &FuelClientWatcher,
) {
    for gateway_withdraw_alert in &watch_config.gateway_withdraw_alerts {
        if gateway_withdraw_alert.alert_level == AlertLevel::None {
            continue;
        }

        let time_frame = gateway_withdraw_alert.time_frame;
        let amount = match fuel_chain
            .get_token_amount_withdrawn(
                time_frame,
                &gateway_withdraw_alert.token_address,
            )
            .await
        {
            Ok(amt) => {
                println!(
                    "Fuel Chain: Total {} Tokens Withdrawn {} for time frame {}",
                    gateway_withdraw_alert.token_name, amt, time_frame,
                );
                amt
            },
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
                    "Fuel Chain: ERC20 {} at address {} is above withdrawal threshold",
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
    fuel_chain: &Arc<dyn FuelChainTrait>,
    action_sender: UnboundedSender<ActionParams>,
    alert_sender: UnboundedSender<AlertParams>,
) -> Result<JoinHandle<()>> {
    let watch_config = config.fuel_client_watcher.clone();
    let fuel_chain = Arc::clone(fuel_chain);
    let handle = tokio::spawn(async move {
        loop {
            // update the log every so often to notify that everything is working
            send_alert(
                &alert_sender.clone(),
                String::from("Watching fuel chain."),
                String::from("Periodically querying the fuel chain."),
                AlertLevel::Info,
            );

            check_fuel_chain_connection(&fuel_chain, action_sender.clone(),
                                        alert_sender.clone(), &watch_config).await;

            check_fuel_block_production(&fuel_chain, action_sender.clone(),
                                        alert_sender.clone(), &watch_config).await;

            check_fuel_base_asset_withdrawals(&fuel_chain, action_sender.clone(),
                                                alert_sender.clone(), &watch_config).await;

            check_fuel_token_withdrawals(&fuel_chain, action_sender.clone(),
                                            alert_sender.clone(), &watch_config).await;

            thread::sleep(POLL_DURATION);
        }
    });

    Ok(handle)
}


#[cfg(test)]
mod tests {
    use super::*;
    
    use crate::{
        ethereum_actions::EthereumAction,
        config::*, fuel_watcher::fuel_chain::MockFuelChainTrait,
    };
    use tokio::sync::mpsc::unbounded_channel;

    #[tokio::test]
    async fn test_check_fuel_chain_connection_failure() {
        let mut mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {
            connection_alert: GenericAlert {
                alert_level: AlertLevel::Warn,
                alert_action: EthereumAction::None,
            },
            ..Default::default()
        };

        // Simulate a connection failure
        mock_fuel_chain
            .expect_check_connection()
            .times(1)
            .returning(|| Box::pin(async { Err(anyhow::anyhow!("Connection error")) }));
        
        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_chain_connection(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        // Check if the alert was sent
        if let Ok(alert) = alert_receiver.try_recv() {
            assert!(alert.is_name_equal("Failed to check fuel connection"));
            assert!(alert.is_description_equal("Failed to check fuel connection: Connection error"));
            assert!(alert.is_level_equal(AlertLevel::Warn));
        } else {
            panic!("Alert was not sent");
        }

        // Check if the action was sent
        if let Ok(action) = action_receiver.try_recv() {
            assert!(action.is_action_equal(EthereumAction::None));
            assert!(action.is_alert_level_equal(AlertLevel::Warn));
        } else {
            panic!("Action was not sent");
        }
    }    

    #[tokio::test]
    async fn test_check_fuel_chain_connection_success() {
        let mut mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();
    
        let watch_config = FuelClientWatcher {
            connection_alert: GenericAlert {
                alert_level: AlertLevel::Warn,
                alert_action: EthereumAction::None,
            },
            ..Default::default()
        };
    
        // Simulate a successful connection
        mock_fuel_chain
            .expect_check_connection()
            .times(1)
            .returning(|| Box::pin(async { Ok(()) }));
        
        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_chain_connection(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        assert!(alert_receiver.try_recv().is_err(), "No alert should be sent");
        assert!(action_receiver.try_recv().is_err(), "No action should be sent");
    }


    #[tokio::test]
    async fn test_check_fuel_chain_connection_alert_level_none() {
        let mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {
            connection_alert: GenericAlert {
                alert_level: AlertLevel::Warn,
                alert_action: EthereumAction::None,
            },
            ..Default::default()
        };

        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_block_production(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        assert!(alert_receiver.try_recv().is_err(), "No alert should be sent");
        assert!(action_receiver.try_recv().is_err(), "No action should be sent");
    }

    #[tokio::test]
    async fn test_check_fuel_block_production_alert_level_none() {
        let mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {
            block_production_alert: BlockProductionAlert {
                alert_level: AlertLevel::None,
                max_block_time: 60,
                alert_action: EthereumAction::None,
            },
            ..Default::default()
        };

        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_block_production(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        assert!(alert_receiver.try_recv().is_err(), "No alert should be sent");
        assert!(action_receiver.try_recv().is_err(), "No action should be sent");
    }

    #[tokio::test]
    async fn test_check_fuel_block_production_success() {
        let mut mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {
            block_production_alert: BlockProductionAlert {
                alert_level: AlertLevel::Warn,
                max_block_time: 60,
                alert_action: EthereumAction::None,
            },
            ..Default::default()
        };

        // Simulate block production time within the maximum allowed time
        let simulated_block_time = 30; // Less than max_block_time
        mock_fuel_chain
            .expect_get_seconds_since_last_block()
            .times(1)
            .returning(move || Box::pin(async move { Ok(simulated_block_time) }));

        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_block_production(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        assert!(alert_receiver.try_recv().is_err(), "No alert should be sent");
        assert!(action_receiver.try_recv().is_err(), "No action should be sent");
    }

    #[tokio::test]
    async fn test_check_fuel_block_production_error() {
        let mut mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {
            block_production_alert: BlockProductionAlert {
                alert_level: AlertLevel::Warn,
                max_block_time: 60,
                alert_action: EthereumAction::None,
            },
            ..Default::default()
        };

        // Simulate an error in retrieving block production time
        mock_fuel_chain
            .expect_get_seconds_since_last_block()
            .times(1)
            .returning(|| Box::pin(async move { Err(anyhow::anyhow!("Error fetching block time")) }));

        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_block_production(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        // Check if the alert was sent
        if let Ok(alert) = alert_receiver.try_recv() {
            assert!(alert.is_name_equal("Failed to check fuel block production"));
            assert!(alert.is_description_equal("Failed to check fuel block production: Error fetching block time"));
            assert!(alert.is_level_equal(AlertLevel::Warn));
        } else {
            panic!("Alert was not sent");
        }

        // Check if the action was sent
        if let Ok(action) = action_receiver.try_recv() {
            assert!(action.is_action_equal(EthereumAction::None));
            assert!(action.is_alert_level_equal(AlertLevel::Warn));
        } else {
            panic!("Action was not sent");
        }
    }

    #[tokio::test]
    async fn test_check_fuel_block_production_delay() {
        let mut mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {
            block_production_alert: BlockProductionAlert {
                alert_level: AlertLevel::Warn,
                max_block_time: 60,
                alert_action: EthereumAction::None,
            },
            ..Default::default()
        };

        // Simulate block production time exceeding the maximum allowed time
        let simulated_block_time = 70; // Exceeds max_block_time
        mock_fuel_chain
            .expect_get_seconds_since_last_block()
            .times(1)
            .returning(move || Box::pin(async move { Ok(simulated_block_time) }));

        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_block_production(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        // Check if the alert was sent
        if let Ok(alert) = alert_receiver.try_recv() {
            assert!(alert.is_name_equal("Fuel block is taking long"));
            assert!(alert.is_description_equal("Next fuel block is taking longer than 60 seconds. Last block was 70 seconds ago."));
            assert!(alert.is_level_equal(AlertLevel::Warn));
        } else {
            panic!("Alert was not sent");
        }

        // Check if the action was sent
        if let Ok(action) = action_receiver.try_recv() {
            assert!(action.is_action_equal(EthereumAction::None));
            assert!(action.is_alert_level_equal(AlertLevel::Warn));
        } else {
            panic!("Action was not sent");
        }
    }

    #[tokio::test]
    async fn test_check_fuel_base_asset_withdrawals_within_threshold() {
        let mut mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {
            portal_withdraw_alerts: vec![
                WithdrawAlert {
                    alert_level: AlertLevel::Warn,
                    amount: 1000.0,
                    token_decimals: 2,
                    time_frame: 3600,
                    alert_action: EthereumAction::None,
                    token_name: String::from("ETH"),
                    token_address: String::from("0x0000000000000000000000000000000000000000000000000000000000000000"),
                }],
            ..Default::default()
        };

        // Simulate withdrawal amount within the threshold
        let withdrawal_amount = 500;
        mock_fuel_chain
            .expect_get_base_amount_withdrawn()
            .withf(|&time_frame| time_frame == 3600)
            .times(1)
            .returning(move |_| Box::pin(async move { Ok(withdrawal_amount) }));

        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_base_asset_withdrawals(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        // Assert that no alert was sent
        assert!(alert_receiver.try_recv().is_err(), "No alert should be sent");
        assert!(action_receiver.try_recv().is_err(), "No action should be sent");
    }

    #[tokio::test]
    async fn test_check_fuel_base_asset_withdrawals_error() {
        let mut mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {
            portal_withdraw_alerts: vec![
                WithdrawAlert {
                    alert_level: AlertLevel::Warn,
                    amount: 1000.0,
                    token_decimals: 2,
                    time_frame: 3600,
                    alert_action: EthereumAction::None,
                    token_name: String::from("ETH"),
                    token_address: String::from("0x0000000000000000000000000000000000000000000000000000000000000000"),
                }],
            ..Default::default()
        };

        // Simulate an error in retrieving base asset withdrawal amount
        mock_fuel_chain
            .expect_get_base_amount_withdrawn()
            .withf(|&time_frame| time_frame == 3600)
            .times(1)
            .returning(move |_| Box::pin(async move { Err(anyhow::anyhow!("Error fetching withdrawal amount")) }));

        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_base_asset_withdrawals(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        // Check if the alert was sent
        if let Ok(alert) = alert_receiver.try_recv() {
            assert!(alert.is_name_equal("Failed to check fuel chain for base asset withdrawals"));
            assert!(alert.is_description_equal("Failed to check base asset withdrawals: Error fetching withdrawal amount"));
            assert!(alert.is_level_equal(AlertLevel::Warn));
        } else {
            panic!("Alert was not sent");
        }

        // Check if the action was sent
        if let Ok(action) = action_receiver.try_recv() {
            assert!(action.is_action_equal(EthereumAction::None));
            assert!(action.is_alert_level_equal(AlertLevel::Warn));
        } else {
            panic!("Action was not sent");
        }
    }

    #[tokio::test]
    async fn test_check_fuel_base_asset_withdrawals_alert_level_none() {
        let mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {                
            portal_withdraw_alerts: vec![
                WithdrawAlert {
                    alert_level: AlertLevel::None,
                    amount: 1000.0,
                    token_decimals: 2,
                    time_frame: 3600,
                    alert_action: EthereumAction::None,
                    token_name: String::from("ETH"),
                    token_address: String::from("0x0000000000000000000000000000000000000000000000000000000000000000"),
                }],
            ..Default::default()
        };

        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_base_asset_withdrawals(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        assert!(alert_receiver.try_recv().is_err(), "No alert should be sent");
        assert!(action_receiver.try_recv().is_err(), "No action should be sent");
    }

    #[tokio::test]
    async fn test_check_fuel_base_asset_withdrawals_no_alerts() {
        let mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {                
            portal_withdraw_alerts: vec![],
            ..Default::default()
        };

        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_base_asset_withdrawals(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        assert!(alert_receiver.try_recv().is_err(), "No alert should be sent");
        assert!(action_receiver.try_recv().is_err(), "No action should be sent");
    }

    #[tokio::test]
    async fn test_check_fuel_token_withdrawals_within_threshold() {
        let mut mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();
    
        let watch_config = FuelClientWatcher {
            gateway_withdraw_alerts: vec![
                WithdrawAlert {
                    alert_level: AlertLevel::Warn,
                    amount: 1000.0,
                    token_decimals: 9,
                    time_frame: 3600,
                    alert_action: EthereumAction::None,
                    token_name: String::from("USDC"),
                    token_address: String::from("0x3a0126dfe64631f1caaebccbdb334570f40bcdc2426fd3c87e9ac690b2fa3964"),
                }],
            ..Default::default()
        };
    
        // Simulate withdrawal amount within the threshold
        let withdrawal_amount = get_value(500.0, 9);
        mock_fuel_chain
            .expect_get_token_amount_withdrawn()
            .withf(move |time_frame, token_address| {
                *time_frame == 3600 && token_address == "0x3a0126dfe64631f1caaebccbdb334570f40bcdc2426fd3c87e9ac690b2fa3964"
            })
            .times(1)
            .returning(move |_, _| Box::pin(async move { Ok(withdrawal_amount) }));
    
        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_token_withdrawals(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        assert!(alert_receiver.try_recv().is_err(), "No alert should be sent");
        assert!(action_receiver.try_recv().is_err(), "No action should be sent");
    }

    #[tokio::test]
    async fn test_check_fuel_token_withdrawals_error() {
        let mut mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();
    
        let watch_config = FuelClientWatcher {
            gateway_withdraw_alerts: vec![
                WithdrawAlert {
                    alert_level: AlertLevel::Warn,
                    amount: 1000.0,
                    token_decimals: 9,
                    time_frame: 3600,
                    alert_action: EthereumAction::None,
                    token_name: String::from("USDC"),
                    token_address: String::from("0x3a0126dfe64631f1caaebccbdb334570f40bcdc2426fd3c87e9ac690b2fa3964"),
                }],
            ..Default::default()
        };
    
        // Simulate an error in retrieving token withdrawal amount
        mock_fuel_chain
            .expect_get_token_amount_withdrawn()
            .withf(|time_frame, token_address| {
                *time_frame == 3600 && token_address == "0x3a0126dfe64631f1caaebccbdb334570f40bcdc2426fd3c87e9ac690b2fa3964"
            })
            .times(1)
            .returning(|_, _| Box::pin(async { Err(anyhow::anyhow!("Error fetching withdrawal amount")) }));
    
        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_token_withdrawals(&fuel_chain, action_sender, alert_sender, &watch_config).await;
    
        // Check if the alert was sent
        if let Ok(alert) = alert_receiver.try_recv() {
            assert!(alert.is_name_equal("Failed to check fuel chain for ERC20 USDC withdrawals at address 0x3a0126dfe64631f1caaebccbdb334570f40bcdc2426fd3c87e9ac690b2fa3964"));
            assert!(alert.is_description_equal("Failed to check ERC20 withdrawals: Error fetching withdrawal amount"));
            assert!(alert.is_level_equal(AlertLevel::Warn));
        } else {
            panic!("Alert was not sent");
        }
    
        // Check if the action was sent
        if let Ok(action) = action_receiver.try_recv() {
            assert!(action.is_action_equal(EthereumAction::None));
            assert!(action.is_alert_level_equal(AlertLevel::Warn));
        } else {
            panic!("Action was not sent");
        }
    }

    #[tokio::test]
    async fn test_check_fuel_token_withdrawals_alert_level_none() {
        let mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {                
            gateway_withdraw_alerts: vec![
                WithdrawAlert {
                    alert_level: AlertLevel::None,
                    amount: 1000.0,
                    token_decimals: 2,
                    time_frame: 3600,
                    alert_action: EthereumAction::None,
                    token_name: String::from("USDC"),
                    token_address: String::from("0x3a0126dfe64631f1caaebccbdb334570f40bcdc2426fd3c87e9ac690b2fa3964"),
                }],
            ..Default::default()
        };

        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_token_withdrawals(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        assert!(alert_receiver.try_recv().is_err(), "No alert should be sent");
        assert!(action_receiver.try_recv().is_err(), "No action should be sent");
    }

    #[tokio::test]
    async fn test_check_fuel_token_withdrawals_no_alerts() {
        let mock_fuel_chain = MockFuelChainTrait::new();
        let (
            action_sender,
            mut action_receiver,
        ) = unbounded_channel();
        let (
            alert_sender,
            mut alert_receiver,
        ) = unbounded_channel();

        let watch_config = FuelClientWatcher {                
            gateway_withdraw_alerts: vec![],
            ..Default::default()
        };

        let fuel_chain = Arc::new(mock_fuel_chain) as Arc<dyn FuelChainTrait>;
        check_fuel_token_withdrawals(&fuel_chain, action_sender, alert_sender, &watch_config).await;

        assert!(alert_receiver.try_recv().is_err(), "No alert should be sent");
        assert!(action_receiver.try_recv().is_err(), "No action should be sent");
    }

}
