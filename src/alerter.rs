use crate::WatchtowerConfig;
use crate::PagerDutyClient;

use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, Instant};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::Mutex;

static MIN_DURATION_FROM_START_TO_ERR: Duration = Duration::from_millis(60 * 60 * 1000);

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
    watchtower_system_name: String,
    system_start_time: SystemTime,
}

#[derive(Clone, Debug)]
struct AlertParams {
    text: String,
    level: AlertLevel,
}

impl WatchtowerAlerter {
    pub fn new(config: &WatchtowerConfig, pagerduty_client: PagerDutyClient) -> Result<Self> {

        let alert_cache = Arc::new(Mutex::new(HashMap::with_capacity(config.alert_cache_size)));
        let alert_cache_expiry = config.alert_cache_expiry;
        let watchtower_system_name = config.watchtower_system_name.to_string();
        let system_start_time = SystemTime::now();

        // start handler thread for alert function
        let (
            alert_sender,
            mut rx,
        ) = mpsc::unbounded_channel::<AlertParams>();

        tokio::spawn(async move {
            while let Some(params) = rx.recv().await {
                match params.level {
                    AlertLevel::Info => log::info!("{}", params.text),
                    AlertLevel::Warn => log::warn!("{}", params.text),
                    AlertLevel::Error => log::error!("{}", params.text),
                    AlertLevel::None => {}
                }
            }
        });

        Ok(WatchtowerAlerter {
            alert_sender,
            alert_cache,
            alert_cache_expiry,
            pagerduty_client,
            watchtower_system_name,
            system_start_time,
        })
    }

    pub async fn alert(&self, text: String, level: AlertLevel) {
        let now = Instant::now();
        let mut cache = self.alert_cache.lock().await;

        cache.retain(|_, &mut expiry| now < expiry);

        if cache.get(&text).map(|&expiry| now >= expiry).unwrap_or(true) {
            cache.insert(text.clone(), now + self.alert_cache_expiry);

            let severity = match level {
                AlertLevel::Info => "info",
                AlertLevel::Warn => "warning",
                AlertLevel::Error => "critical",
                _ => return,
            };

            if SystemTime::now().duration_since(self.system_start_time)
                .map(|d| d > MIN_DURATION_FROM_START_TO_ERR)
                .unwrap_or(true) && (level == AlertLevel::Warn || level == AlertLevel::Error) {

                if let Err(e) = self.pagerduty_client.send_alert(
                    severity.to_string(),
                     text.clone(),
                      self.watchtower_system_name.clone(),
                ).await {
                    log::error!("Failed to send alert to PagerDuty: {}", e);
                }
            }
        }

        let params = AlertParams { text, level };
        self.alert_sender.send(params).unwrap();
    }
}
