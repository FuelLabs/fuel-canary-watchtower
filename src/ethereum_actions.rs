use crate::alerts::{AlertLevel, WatchtowerAlerts};
use crate::config::WatchtowerConfig;
use crate::ethereum_watcher::state_contract::StateContract;
use crate::ethereum_watcher::gateway_contract::GatewayContract;
use crate::ethereum_watcher::portal_contract::PortalContract;

use ethers::signers::{Signer, Wallet};
use ethers::prelude::k256::ecdsa::SigningKey;

use anyhow::Result;
use ethers::providers::{Http, Middleware, Provider};
use serde::Deserialize;
use tokio::sync::mpsc::{self, UnboundedSender};
use std::sync::Arc;
use ethers::prelude::*;

use crate::ethereum_watcher::ethereum_chain::EthereumChain;

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

        // start handler thread for action function
        let (tx, mut rx) = mpsc::unbounded_channel::<ActionParams>();
        tokio::spawn(async move {
            loop {
                let received_result = rx.recv().await;
                match received_result {
                    Some(params) => {
                        match params.action {
                            EthereumAction::PauseState => {
                                alerts.alert(String::from("Pausing state contract."), AlertLevel::Info);
                                match state_contract.pause().await {
                                    Err(e) => alerts.alert(e.to_string(), params.alert_level),
                                    Ok(_) => {
                                        alerts.alert(
                                            String::from("Successfully paused state contract."),
                                            AlertLevel::Info,
                                        );
                                    }
                                }
                            }
                            EthereumAction::PauseGateway => {
                                alerts.alert(String::from("Pausing gateway contract."), AlertLevel::Info);
                                match gateway_contract.pause().await {
                                    Err(e) => alerts.alert(e.to_string(), params.alert_level),
                                    Ok(_) => {
                                        alerts.alert(
                                            String::from("Successfully paused gateway contract."),
                                            AlertLevel::Info,
                                        );
                                    }
                                }
                            }
                            EthereumAction::PausePortal => {
                                alerts.alert(String::from("Pausing portal contract."), AlertLevel::Info);
                                match portal_contract.pause().await {
                                    Err(e) => alerts.alert(e.to_string(), params.alert_level),
                                    Ok(_) => {
                                        alerts.alert(
                                            String::from("Successfully paused portal contract."),
                                            AlertLevel::Info,
                                        );
                                    }
                                }
                            }
                            EthereumAction::PauseAll => {
                                alerts.alert(String::from("Pausing all contracts."), AlertLevel::Info);
                                match state_contract.pause().await {
                                    Err(e) => alerts.alert(e.to_string(), params.alert_level.clone()),
                                    Ok(_) => {
                                        alerts.alert(
                                            String::from("Successfully paused state contract."),
                                            AlertLevel::Info,
                                        );
                                    }
                                };
                                match gateway_contract.pause().await {
                                    Err(e) => alerts.alert(e.to_string(), params.alert_level.clone()),
                                    Ok(_) => {
                                        alerts.alert(
                                            String::from("Successfully paused gateway contract."),
                                            AlertLevel::Info,
                                        );
                                    }
                                };
                                match portal_contract.pause().await {
                                    Err(e) => alerts.alert(e.to_string(), params.alert_level.clone()),
                                    Ok(_) => {
                                        alerts.alert(
                                            String::from("Successfully paused portal contract."),
                                            AlertLevel::Info,
                                        );
                                    }
                                };
                            }
                            EthereumAction::None => {}
                        };
                    }
                    None => {
                        alerts.alert(String::from(THREAD_CONNECTIONS_ERR), AlertLevel::Error);
                        panic!("{}", THREAD_CONNECTIONS_ERR);
                    }
                }
            }
        });

        Ok(WatchtowerEthereumActions { action_sender: tx })
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
