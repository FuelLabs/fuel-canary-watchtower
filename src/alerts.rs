use crate::WatchtowerConfig;

use anyhow::Result;
use serde::Deserialize;
use std::thread;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc::error::TryRecvError::{Disconnected, Empty};
use tokio::sync::mpsc::{self, UnboundedSender};

static MIN_DURATION_FROM_START_TO_ERR: Duration = Duration::from_millis(60 * 60 * 1000);
static THREAD_CONNECTIONS_ERR: &str = "Connections to the alerts thread have all closed.";
static POLL_DURATION: Duration = Duration::from_millis(1000);

#[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum AlertLevel {
    None,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct WatchtowerAlerts {
    alert_sender: UnboundedSender<AlertParams>,
}

#[derive(Clone, Debug)]
struct AlertParams {
    text: String,
    level: AlertLevel,
}

// TODO: buffer message alerts to avoid duplicates

impl WatchtowerAlerts {
    pub fn new(_config: &WatchtowerConfig) -> Result<Self> {
        let start = SystemTime::now();

        // TODO: setup connection with alert messaging

        // start handler thread for alert function
        let (tx, mut rx) = mpsc::unbounded_channel::<AlertParams>();
        tokio::spawn(async move {
            loop {
                let received_result = rx.try_recv();
                match received_result {
                    Ok(params) => {
                        match params.level {
                            AlertLevel::None => {}
                            AlertLevel::Info => log::info!("{}", params.text),
                            AlertLevel::Warn => {
                                log::warn!("{}", params.text);
                                let min_time_elapsed = match SystemTime::now().duration_since(start) {
                                    Ok(d) => d > MIN_DURATION_FROM_START_TO_ERR,
                                    _ => true,
                                };
                                if min_time_elapsed {
                                    // TODO: send warning through communication channels (with time buffer for duplicates)
                                }
                            }
                            AlertLevel::Error => {
                                log::error!("{}", params.text);
                                let min_time_elapsed = match SystemTime::now().duration_since(start) {
                                    Ok(d) => d > MIN_DURATION_FROM_START_TO_ERR,
                                    _ => true,
                                };
                                if min_time_elapsed {
                                    // TODO: send error through communication channels (with time buffer for duplicates)
                                }
                            }
                        }
                    }
                    Err(recv_error) => {
                        match recv_error {
                            Disconnected => {
                                log::error!("{}", THREAD_CONNECTIONS_ERR);
                                // TODO: send error through communication channels

                                panic!("{}", THREAD_CONNECTIONS_ERR);
                            }
                            Empty => {
                                // wait a bit until next try
                                thread::sleep(POLL_DURATION);
                            }
                        }
                    }
                }
            }
        });

        Ok(WatchtowerAlerts { alert_sender: tx })
    }

    pub fn alert(&self, text: String, level: AlertLevel) {
        let params = AlertParams { text, level };
        self.alert_sender.send(params).unwrap();
    }
}
