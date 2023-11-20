use crate::WatchtowerConfig;
use crate::PagerDutyClient;

use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, Instant};
use tokio::sync::mpsc::error::TryRecvError::{Disconnected, Empty};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::Mutex;

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
pub struct WatchtowerAlerter {
    alert_sender: UnboundedSender<AlertParams>,
    alert_cache: Arc<Mutex<HashMap<String, Instant>>>,
    alert_cache_expiry: Duration,
    pagerduty_client: PagerDutyClient,
}

#[derive(Clone, Debug)]
struct AlertParams {
    text: String,
    level: AlertLevel,
}

impl WatchtowerAlerter {
    pub fn new(config: &WatchtowerConfig, pagerduty_client: PagerDutyClient) -> Result<Self> {
        let start = SystemTime::now();

        let alert_cache = Arc::new(Mutex::new(HashMap::with_capacity(config.alert_cache_size)));
        let alert_cache_expiry = config.alert_cache_expiry;

        // start handler thread for alert function
        let (
            alert_sender,
            mut rx,
        ) = mpsc::unbounded_channel::<AlertParams>();

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

        Ok(WatchtowerAlerter { alert_sender, alert_cache, alert_cache_expiry, pagerduty_client })
    }

    pub async fn alert(&self, text: String, level: AlertLevel) {
        let now = Instant::now();
        let mut cache = self.alert_cache.lock().await;

        // Check and possibly remove expired alerts
        cache.retain(|_, &mut expiry| now < expiry);

        // Check if the alert is already in the cache and hasn't expired
        if let Some(&expiry) = cache.get(&text) {
            if now < expiry {
                return;
            }
        }

        // Update the cache with the new alert and its expiry time
        cache.insert(text.clone(), now + self.alert_cache_expiry);

        // Send alert to PagerDuty for critical alerts
        if level == AlertLevel::Error {
            let severity = "critical".to_string();
            let source = "Watchtower System".to_string();
            if let Err(e) = self.pagerduty_client.send_alert(severity, text.clone(), source).await {
                log::error!("Failed to send alert to PagerDuty: {}", e);
            }
        }

        let params = AlertParams { text, level };
        self.alert_sender.send(params).unwrap();
    }
}
