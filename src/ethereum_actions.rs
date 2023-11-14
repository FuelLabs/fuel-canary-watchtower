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
    pub async fn new(config: &WatchtowerConfig, alerts: WatchtowerAlerts) -> Result<Self> {
        // setup provider and check that it is valid
        let provider = Provider::<Http>::try_from(&config.ethereum_rpc)?;
        let chain_id = provider.get_chainid().await?.as_u64();
        let arc_provider = Arc::new(provider);

        let provider_result = arc_provider.get_chainid().await;
        if let Err(_) = provider_result {
            return Err(anyhow::anyhow!("Invalid ethereum RPC."));
        }

        // setup wallet
        let mut read_only = false;
        let key_str = match &config.ethereum_wallet_key {
            Some(key) => key.clone(),
            None => {
                read_only = true;
                String::from("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
            }
        };
        let wallet: Wallet<SigningKey> = key_str.parse::<Wallet<SigningKey>>()?.with_chain_id(chain_id);

        let state_contract_address: String = config.state_contract_address.to_string();
        let portal_contract_address: String = config.portal_contract_address.to_string();
        let gateway_contract_address: String = config.gateway_contract_address.to_string();

        // Setup the contracts
        let mut state_contract = StateContract::new(
            state_contract_address,
            read_only,
            arc_provider.clone(),
            wallet.clone(),
        ).unwrap();
        let mut portal_contract = PortalContract::new(
            portal_contract_address,
            read_only,
            arc_provider.clone(),
            wallet.clone(),
        ).unwrap();
        let mut gateway_contract = GatewayContract::new(
            gateway_contract_address,
            read_only,
            arc_provider,
            wallet,
        ).unwrap();

        // Initialize the contracts
        state_contract.initialize().await?;
        portal_contract.initialize().await?;
        gateway_contract.initialize().await?;

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
