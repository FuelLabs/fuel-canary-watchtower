mod alerts;
mod config;
mod ethereum_actions;
mod ethereum_watcher;
mod fuel_watcher;

pub use config::{load_config, WatchtowerConfig};

use alerts::{AlertLevel, WatchtowerAlerts};
use anyhow::Result;
use ethereum_actions::WatchtowerEthereumActions;
use ethereum_watcher::start_ethereum_watcher;
use fuel_watcher::start_fuel_watcher;

pub async fn run(config: &WatchtowerConfig) -> Result<()> {

    let alerts_result = WatchtowerAlerts::new(config);
    if alerts_result.is_err() {
        return Err(anyhow::anyhow!(
            "Failed to setup alerts: {}",
            alerts_result.err().unwrap()
        ));
    }
    let alerts = alerts_result.unwrap();

    // build ethereum actions service
    let actions_result = WatchtowerEthereumActions::new(
        config,
        alerts.clone(),
    ).await;
    if actions_result.is_err() {
        return Err(anyhow::anyhow!(
            "Failed to setup actions: {}",
            actions_result.err().unwrap()
        ));
    }
    let actions = actions_result.unwrap();

    // start fuel watcher
    let fuel_watcher_result = start_fuel_watcher(
        config,
        actions.clone(),
        alerts.clone(),
    ).await;
    if fuel_watcher_result.is_err() {
        return Err(anyhow::anyhow!(
            "Failed to start fuel watcher: {}",
            fuel_watcher_result.err().unwrap()
        ));
    }
    let fuel_thread = fuel_watcher_result.unwrap();

    // start ethereum watcher
    let ethereum_watcher_result = start_ethereum_watcher(
        config,
        actions.clone(),
        alerts.clone(),
    ).await;
    if ethereum_watcher_result.is_err() {
        return Err(anyhow::anyhow!(
            "Failed to start ethereum watcher: {}",
            ethereum_watcher_result.err().unwrap()
        ));
    }
    let ethereum_thread = ethereum_watcher_result.unwrap();

    // wait for threads to finish (if ever)
    match ethereum_thread.await {
        Err(e) => {
            alerts.alert(
                String::from("Ethereum watcher thread failed."),
                AlertLevel::Error,
            );
            return Err(anyhow::anyhow!("Ethereum watcher thread failed: {}", e));
        }
        Ok(_) => {}
    }
    match fuel_thread.await {
        Err(e) => {
            alerts.alert(
                String::from("Fuel watcher thread failed."),
                AlertLevel::Error,
            );
            return Err(anyhow::anyhow!("Fuel watcher thread failed: {}", e));
        }
        Ok(_) => {}
    }

    Ok(())
}
