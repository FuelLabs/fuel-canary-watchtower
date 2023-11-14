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
use tokio::task::JoinHandle;

pub async fn run(config: &WatchtowerConfig) -> Result<()> {
    let alerts = initialize_alerts(config)?;
    let actions = initialize_ethereum_actions(config, &alerts).await?;
    let fuel_thread = start_fuel_watcher(config, actions.clone(), alerts.clone()).await?;
    let ethereum_thread = start_ethereum_watcher(config, actions.clone(), alerts.clone()).await?;

    handle_watcher_threads(fuel_thread, ethereum_thread, &alerts).await
}

fn initialize_alerts(config: &WatchtowerConfig) -> Result<WatchtowerAlerts> {
    WatchtowerAlerts::new(config)
        .map_err(|e| anyhow::anyhow!("Failed to setup alerts: {}", e))
}

async fn initialize_ethereum_actions(
    config: &WatchtowerConfig,
    alerts: &WatchtowerAlerts,
) -> Result<WatchtowerEthereumActions> {
    WatchtowerEthereumActions::new(config, alerts.clone()).await
        .map_err(|e| anyhow::anyhow!("Failed to setup actions: {}", e))
}

async fn handle_watcher_threads(
    fuel_thread: JoinHandle<()>,
    ethereum_thread: JoinHandle<()>,
    alerts: &WatchtowerAlerts,
) -> Result<()> {

    if let Err(e) = ethereum_thread.await {
        alerts.alert(String::from("Ethereum watcher thread failed."), AlertLevel::Error);
        return Err(anyhow::anyhow!("Ethereum watcher thread failed: {}", e));
    }

    if let Err(e) = fuel_thread.await {
        alerts.alert(String::from("Fuel watcher thread failed."), AlertLevel::Error);
        return Err(anyhow::anyhow!("Fuel watcher thread failed: {}", e));
    }

    Ok(())
}
