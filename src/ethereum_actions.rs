use std::future::Future;
use std::time::Duration;
use crate::alerts::{AlertLevel, WatchtowerAlerts};
use crate::ethereum_watcher::state_contract::StateContract;
use crate::ethereum_watcher::gateway_contract::GatewayContract;
use crate::ethereum_watcher::portal_contract::PortalContract;

use anyhow::Result;
use ethers::providers::{Http, Provider};
use serde::Deserialize;
use tokio::sync::mpsc::{self, UnboundedSender};
use ethers::prelude::*;
use tokio::time::timeout;

pub static THREAD_CONNECTIONS_ERR: &str = "Connections to the ethereum actions thread have all closed.";

#[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum EthereumAction {
    None,
    PauseState,
    PauseGateway,
    PausePortal,
    PauseAll,
}

#[derive(Clone, Debug)]
pub struct WatchtowerEthereumActions {
    action_sender: UnboundedSender<ActionParams>,
}

impl WatchtowerEthereumActions {
    pub async fn new(
        alerts: WatchtowerAlerts,
        state_contract: StateContract<GasEscalatorMiddleware<Provider<Http>>>,
        portal_contract: PortalContract<GasEscalatorMiddleware<Provider<Http>>>,
        gateway_contract: GatewayContract<GasEscalatorMiddleware<Provider<Http>>>,
    ) -> Result<Self> {

        let (tx, mut rx) = mpsc::unbounded_channel::<ActionParams>();
        tokio::spawn(async move {
            while let Some(params) = rx.recv().await {
                Self::handle_action(
                    params.action,
                    &state_contract.clone(),
                    &portal_contract.clone(),
                    &gateway_contract.clone(),
                    &alerts.clone(),
                    params.alert_level,
                ).await;
            }
            alerts.alert(String::from(THREAD_CONNECTIONS_ERR), AlertLevel::Error);
            panic!("{}", THREAD_CONNECTIONS_ERR);
        });

        Ok(WatchtowerEthereumActions { action_sender: tx })
    }

    async fn pause_contract<F>(
        contract_name: &str,
        pause_future: F,
        alerts: &WatchtowerAlerts,
        alert_level: AlertLevel,
    )
        where
            F: Future<Output = Result<(), anyhow::Error>> + Send,
    {
        alerts.alert(format!("Pausing {} contract.", contract_name), AlertLevel::Info);
    
        // Set a duration for the timeout
        let timeout_duration = Duration::from_secs(30);
    
        match timeout(timeout_duration, pause_future).await {
            Ok(Ok(_)) => {
                alerts.alert(format!("Successfully paused {} contract.", contract_name), AlertLevel::Info);
            },
            Ok(Err(e)) => {
                // This is the case where pause_future completed, but resulted in an error.
                alerts.alert(e.to_string(), alert_level);
            },
            Err(_) => {
                // This is the timeout case
                alerts.alert(format!("Timeout while pausing {} contract.", contract_name), alert_level);
            }
        }
    }

    async fn handle_action(
        action: EthereumAction,
        state_contract: &StateContract<GasEscalatorMiddleware<Provider<Http>>>,
        portal_contract: &PortalContract<GasEscalatorMiddleware<Provider<Http>>>,
        gateway_contract: &GatewayContract<GasEscalatorMiddleware<Provider<Http>>>,
        alerts: &WatchtowerAlerts,
        alert_level: AlertLevel,
    ) {
        match action {
            EthereumAction::PauseState => {
                Self::pause_contract("state", state_contract.pause(), alerts, alert_level).await;
            },
            EthereumAction::PauseGateway => {
                Self::pause_contract("gateway", gateway_contract.pause(), alerts, alert_level).await;
            },
            EthereumAction::PausePortal => {
                Self::pause_contract("portal", portal_contract.pause(), alerts, alert_level).await;
            },
            EthereumAction::PauseAll => {
                Self::pause_contract("state", state_contract.pause(), alerts, alert_level.clone()).await;
                Self::pause_contract("gateway", gateway_contract.pause(), alerts, alert_level.clone()).await;
                Self::pause_contract("portal", portal_contract.pause(), alerts, alert_level).await;
            },
            EthereumAction::None => {},
        }
    }

    pub fn action(&self, action: EthereumAction, alert_level: Option<AlertLevel>) {
        let alert_level = match alert_level {
            Some(level) => level,
            None => AlertLevel::Info,
        };
        let params = ActionParams { action, alert_level };
        self.action_sender.send(params).unwrap();
    }
}

#[derive(Clone, Debug)]
struct ActionParams {
    action: EthereumAction,
    alert_level: AlertLevel,
}
