#[cfg(test)]
pub mod test_utils;

mod alerter;
mod config;
mod pagerduty;
mod ethereum_actions;
pub(crate) mod ethereum_watcher;
mod fuel_watcher;

use std::sync::Arc;

pub use config::{load_config, WatchtowerConfig};
use alerter::{AlertLevel, WatchtowerAlerter};
use anyhow::Result;
use ethers::middleware::Middleware;
use ethereum_actions::WatchtowerEthereumActions;
use ethereum_watcher::{
    ethereum_utils::{
        setup_ethereum_provider, setup_ethereum_wallet,
    },
    ethereum_chain::EthereumChain,
    start_ethereum_watcher,
};
use fuel_watcher::start_fuel_watcher;
use pagerduty::PagerDutyClient;
use reqwest::Client;
use tokio::task::JoinHandle;
use crate::ethereum_watcher::{
    gateway_contract::{
        GatewayContract,
        GatewayContractTrait,
    },
    portal_contract::{
        PortalContract,
        PortalContractTrait,
    },
    state_contract::{
        StateContract,
        StateContractTrait,
    }
};
use tokio::sync::Mutex;

use crate::fuel_watcher::fuel_utils::setup_fuel_provider;
use crate::fuel_watcher::fuel_chain::FuelChain;

pub async fn run(config: &WatchtowerConfig) -> Result<()> {

    // Setup the providers and wallets.
    let fuel_provider = setup_fuel_provider(&config.fuel_graphql).await?;
    let ether_provider = setup_ethereum_provider(
        &config.ethereum_rpc,
    ).await?;
    let chain_id: u64 = ether_provider.get_chainid().await?.as_u64();
    let (wallet, read_only) = setup_ethereum_wallet(
        config.ethereum_wallet_key.clone(),
        chain_id,
    )?;

    // Setup the ethereum based contracts.
    let state_contract_address: String = config.state_contract_address.to_string();
    let portal_contract_address: String = config.portal_contract_address.to_string();
    let gateway_contract_address: String = config.gateway_contract_address.to_string();

    let mut state_contract = StateContract::new(
        state_contract_address,
        read_only,
        ether_provider.clone(),
        wallet.clone(),
    ).unwrap();
    let mut portal_contract = PortalContract::new(
        portal_contract_address,
        read_only,
        ether_provider.clone(),
        wallet.clone(),
    ).unwrap();
    let mut gateway_contract = GatewayContract::new(
        gateway_contract_address,
        read_only,
        ether_provider.clone(),
        wallet,
    ).unwrap();

    // Initialize the contracts.
    state_contract.initialize().await?;
    portal_contract.initialize().await?;
    gateway_contract.initialize().await?;

    let arc_state_contract = Arc::new(Mutex::new(state_contract));
    let arc_portal_contract = Arc::new(Mutex::new(portal_contract));
    let arc_gateway_contract = Arc::new(Mutex::new(gateway_contract));

    // Create the chains.
    let fuel_chain: FuelChain = FuelChain::new(fuel_provider).unwrap();
    let ethereum_chain = EthereumChain::new(
        ether_provider,
    ).await?;

    let pagerduty_client = PagerDutyClient::new(
        config.pagerduty_api_key.clone(),
         Arc::new(Client::new()),
    );
    let alerts = WatchtowerAlerter::new(config, pagerduty_client).map_err(
        |e| anyhow::anyhow!("Failed to setup alerts: {}", e),
    )?;
    alerts.start_alert_handling_thread();

    let actions = WatchtowerEthereumActions::new(
        alerts.get_alert_sender(),
        arc_state_contract.clone(),
        arc_portal_contract.clone(),
        arc_gateway_contract.clone(),
    );
    actions.start_action_handling_thread();

    // let ethereum_thread = start_ethereum_watcher(
    //     config,
    //     actions.get_action_sender(),
    //     alerts.get_alert_sender(),
    //     fuel_chain.clone(),
    //     ethereum_chain,
    //     state_contract,
    //     portal_contract,
    //     gateway_contract,
    // ).await?;
    // let fuel_thread = start_fuel_watcher(
    //     config,
    //     actions.get_action_sender(),
    //     alerts.get_alert_sender(),
    //     fuel_chain,
    // ).await?;

    // handle_watcher_threads(fuel_thread, ethereum_thread, &alerts).await
    Ok(())
}

async fn handle_watcher_threads(
    fuel_thread: JoinHandle<()>,
    ethereum_thread: JoinHandle<()>,
    _alerts: &WatchtowerAlerter,
) -> Result<()> {

    if let Err(e) = ethereum_thread.await {
        // alerts.alert(
        //     String::from("Ethereum watcher thread failed."),
        //     AlertLevel::Error,
        //     AlertType::EthereumWatcherThreadFailed,
        // ).await;
        return Err(anyhow::anyhow!("Ethereum watcher thread failed: {}", e));
    }

    if let Err(e) = fuel_thread.await {
        // alerts.alert(
        //     String::from("Fuel watcher thread failed."),
        //     AlertLevel::Error,
        // ).await;
        return Err(anyhow::anyhow!("Fuel watcher thread failed: {}", e));
    }

    Ok(())
}
