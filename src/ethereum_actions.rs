use std::future::Future;
use std::time::Duration;
use crate::alerter::{AlertLevel, WatchtowerAlerter};
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

#[derive(Deserialize, Clone, PartialEq, Eq, Debug, Default)]
pub enum EthereumAction {
    #[default]
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

#[derive(Clone, Debug)]
struct ActionParams {
    action: EthereumAction,
    alert_level: AlertLevel,
}

impl WatchtowerEthereumActions {
    pub async fn new(
        alerts: WatchtowerAlerter,
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
            alerts.alert(String::from(THREAD_CONNECTIONS_ERR), AlertLevel::Error).await;
            panic!("{}", THREAD_CONNECTIONS_ERR);
        });

        Ok(WatchtowerEthereumActions { action_sender: tx })
    }

    async fn pause_contract<F>(
        contract_name: &str,
        pause_future: F,
        alerts: &WatchtowerAlerter,
        alert_level: AlertLevel,
    )
        where
            F: Future<Output = Result<(), anyhow::Error>> + Send,
    {
        alerts.alert(format!("Pausing {} contract.", contract_name), AlertLevel::Info).await;
    
        // Set a duration for the timeout
        let timeout_duration = Duration::from_secs(30);
    
        match timeout(timeout_duration, pause_future).await {
            Ok(Ok(_)) => {
                alerts.alert(format!("Successfully paused {} contract.", contract_name), AlertLevel::Info).await;
            },
            Ok(Err(e)) => {
                // This is the case where pause_future completed, but resulted in an error.
                alerts.alert(e.to_string(), alert_level).await;
            },
            Err(_) => {
                // This is the timeout case
                alerts.alert(format!("Timeout while pausing {} contract.", contract_name), alert_level).await;
            }
        }
    }

    async fn handle_action(
        action: EthereumAction,
        state_contract: &StateContract<GasEscalatorMiddleware<Provider<Http>>>,
        portal_contract: &PortalContract<GasEscalatorMiddleware<Provider<Http>>>,
        gateway_contract: &GatewayContract<GasEscalatorMiddleware<Provider<Http>>>,
        alerts: &WatchtowerAlerter,
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    // #[tokio::test]
    // async fn test_pause_state_contract() {
    //     let mut mock_state_contract = MockStateContract::new();
    //     mock_state_contract.expect_pause()
    //         .times(1)
    //         .returning(|| Ok(()));

    //     let mut mock_alerts = MockWatchtowerAlerter::new();
    //     mock_alerts.expect_alert()
    //         .withf(|msg, level| msg.contains("Pausing state contract") && *level == AlertLevel::Info)
    //         .times(1)
    //         .return_once(|_, _| ());

    //     mock_alerts.expect_alert()
    //         .withf(|msg, level| msg.contains("Successfully paused state contract") && *level == AlertLevel::Info)
    //         .times(1)
    //         .return_once(|_, _| ());

    //     let (tx, mut rx) = mpsc::unbounded_channel::<ActionParams>();
    //     let actions = WatchtowerEthereumActions { action_sender: tx };

    //     actions.action(EthereumAction::PauseState, Some(AlertLevel::Info));

    //     if let Some(params) = rx.recv().await {
    //         WatchtowerEthereumActions::handle_action(
    //             params.action,
    //             &mock_state_contract,
    //             &MockPortalContract::new(),  // Mocks for other contracts
    //             &MockGatewayContract::new(),
    //             &mock_alerts,
    //             params.alert_level,
    //         ).await;
    //     }
    // }

    // Similar tests can be written for testing pause_gateway, pause_portal, and pause_all
}
