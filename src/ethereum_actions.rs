use crate::alerts::{AlertLevel, WatchtowerAlerts};
use crate::config::WatchtowerConfig;
use crate::ethereum_watcher::consensus_contract::ConsesnsusContract;
use crate::ethereum_watcher::gateway_contract::GatewayContract;
use crate::ethereum_watcher::portal_contract::PortalContract;

use anyhow::Result;
use ethers::providers::{Http, Middleware, Provider};
use serde::Deserialize;
use tokio::sync::mpsc::{self, UnboundedSender};

pub static THREAD_CONNECTIONS_ERR: &str = "Connections to the ethereum actions thread have all closed.";

#[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum EthereumAction {
    None,
    PauseConsensus,
    PauseGateway,
    PausePortal,
    PauseAll,
}

#[derive(Clone, Debug)]
pub struct WatchtowerEthereumActions {
    action_sender: UnboundedSender<ActionParams>,
}

impl WatchtowerEthereumActions {
    pub async fn new(config: &WatchtowerConfig, alerts: WatchtowerAlerts) -> Result<Self> {
        // setup provider and check that it is valid
        let provider = Provider::<Http>::try_from(&config.ethereum_rpc)?;
        let provider_result = provider.get_chainid().await;
        match provider_result {
            Err(_) => return Err(anyhow::anyhow!("Invalid ethereum RPC.")),
            _ => {}
        }

        // setup contracts
        let consensus_contract = ConsesnsusContract::new(config).await?;
        let gateway_contract = GatewayContract::new(config).await?;
        let portal_contract = PortalContract::new(config).await?;

        // start handler thread for action function
        let (tx, mut rx) = mpsc::unbounded_channel::<ActionParams>();
        tokio::spawn(async move {
            loop {
                let received_result = rx.recv().await;
                match received_result {
                    Some(params) => {
                        match params.action {
                            EthereumAction::PauseConsensus => {
                                alerts.alert(String::from("Pausing consensus contract."), AlertLevel::Info);
                                match consensus_contract.pause().await {
                                    Err(e) => alerts.alert(e.to_string(), params.alert_level),
                                    Ok(_) => {
                                        alerts.alert(
                                            String::from("Successfully paused consensus contract."),
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
                                match consensus_contract.pause().await {
                                    Err(e) => alerts.alert(e.to_string(), params.alert_level.clone()),
                                    Ok(_) => {
                                        alerts.alert(
                                            String::from("Successfully paused consensus contract."),
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
