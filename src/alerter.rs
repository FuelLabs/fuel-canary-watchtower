use crate::WatchtowerConfig;
use crate::pagerduty::PagerDutyClient;

use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, Instant};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::Mutex;

#[derive(Deserialize, Clone, PartialEq, Eq, Debug, Default)]
pub enum AlertLevel {
    #[default]
    None,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct WatchtowerAlerter{
    alert_sender: UnboundedSender<AlertParams>,
    alert_cache: Arc<Mutex<HashMap<String, Instant>>>,
    alert_cache_expiry: Duration,
    pagerduty_client: PagerDutyClient,
    watchtower_system_name: String,
    allowed_alerting_start_time: SystemTime,
}

#[derive(Clone, Debug)]
struct AlertParams {
    text: String,
    level: AlertLevel,
}

impl WatchtowerAlerter{
    pub fn new(config: &WatchtowerConfig, pagerduty_client: PagerDutyClient) -> Result<Self> {

        let alert_cache = Arc::new(Mutex::new(HashMap::with_capacity(config.alert_cache_size)));
        let alert_cache_expiry = config.alert_cache_expiry;
        let watchtower_system_name = config.watchtower_system_name.to_string();
        let allowed_alerting_start_time = SystemTime::now() + config.min_duration_from_start_to_err;

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
            allowed_alerting_start_time,
        })
    }

    pub async fn alert(&self, text: String, level: AlertLevel) {
        let now = Instant::now();
        let mut cache = self.alert_cache.lock().await;

        // Retain only non-expired alerts in cache
        cache.retain(|_, &mut expiry| now < expiry);

        // Check if the alert is in the cache and not expired
        let is_in_cache = cache.get(&text).map(|&expiry| now < expiry).unwrap_or(false);

        if !is_in_cache {
            cache.insert(text.clone(), now + self.alert_cache_expiry);

            let severity = match level {
                AlertLevel::Info => "info",
                AlertLevel::Warn => "warning",
                AlertLevel::Error => "critical",
                _ => return,
            };

            if SystemTime::now() >= self.allowed_alerting_start_time && (level == AlertLevel::Warn || level == AlertLevel::Error) {
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

    // This method is for testing purposes to check if an alert is in the cache
    #[cfg(test)]
    pub async fn test_is_alert_in_cache(&self, alert_text: &str) -> bool {
        let cache = self.alert_cache.lock().await;
        cache.contains_key(alert_text)
    }
}

#[cfg(test)]
mod watchtower_alerter_tests {
    use super::*;
    use crate::pagerduty::{PagerDutyClient, MockHttpPoster};
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_watchtower_alerter_initialization() {
        // Create a mock configuration for WatchtowerAlerter
        let config = WatchtowerConfig {
            alert_cache_size: 10,
            alert_cache_expiry: Duration::from_secs(300),
            watchtower_system_name: "TestSystem".to_string(),
            ..Default::default()
        };

        // Create a PagerDutyClient
        let mock_http_poster: MockHttpPoster = MockHttpPoster::new();
        let mock_pagerduty_client = PagerDutyClient::new(
            "test_api_key".to_string(), 
            Arc::new(mock_http_poster),
        );

        // Create the WatchtowerAlerter
        let alerter = WatchtowerAlerter::new(&config, mock_pagerduty_client).unwrap();

        // Assert that the WatchtowerAlerter is initialized correctly
        assert_eq!(alerter.alert_cache.lock().await.len(), 0);
        assert_eq!(alerter.alert_cache_expiry, Duration::from_secs(300));
        assert_eq!(alerter.watchtower_system_name, "TestSystem");
    }

    #[tokio::test]
    async fn test_alert_sending_different_levels() {
        // Create a mock configuration for WatchtowerAlerter
        let config = WatchtowerConfig {
            alert_cache_size: 10,
            alert_cache_expiry: Duration::from_secs(300),
            watchtower_system_name: "TestSystem".to_string(),
            min_duration_from_start_to_err: Duration::from_secs(0),
            ..Default::default()
        };

        // Create a mock HTTP poster for testing
        let mut mock_http_poster: MockHttpPoster = MockHttpPoster::new();

        // Set up expected behavior for the mock HTTP poster
        mock_http_poster.expect_post()
        .withf(|url, payload| url == "https://events.eu.pagerduty.com/v2/enqueue"
            && payload.routing_key == "test_api_key"
            && (payload.payload.severity == "critical" || payload.payload.severity == "warning"))
        .times(2)
        .returning(|_, _| Box::pin(async { Ok(()) }));

        let mock_pagerduty_client = PagerDutyClient::new(
            "test_api_key".to_string(), 
            Arc::new(mock_http_poster),
        );

        // Create the WatchtowerAlerter
        let alerter = WatchtowerAlerter::new(&config, mock_pagerduty_client).unwrap();

        // Critical and warning are expected to be passed on to pagerduty
        alerter.alert(String::from("Critical alert"), AlertLevel::Error).await;
        alerter.alert(String::from("Warning alert"), AlertLevel::Warn).await;

        // Info and None are not passed on to pagerduty
        alerter.alert(String::from("Info alert"), AlertLevel::Info).await;
        alerter.alert(String::from("No alert"), AlertLevel::None).await;

        // Check if all the alerts are in cache
        assert!(alerter.test_is_alert_in_cache("Critical alert").await);
        assert!(alerter.test_is_alert_in_cache("Warning alert").await);
        assert!(alerter.test_is_alert_in_cache("Info alert").await);
        assert!(alerter.test_is_alert_in_cache("No alert").await);
    }


    #[tokio::test]
    async fn test_alert_caching_and_expiry() {
        let config = WatchtowerConfig {
            alert_cache_size: 10,
            alert_cache_expiry: Duration::from_secs(1),
            watchtower_system_name: "TestSystem".to_string(),
            min_duration_from_start_to_err: Duration::from_secs(0),
            ..Default::default()
        };
    
        let mock_pagerduty_client = PagerDutyClient::new(
            "test_api_key".to_string(),
             Arc::new(MockHttpPoster::new()),
        );
    
        let alerter = WatchtowerAlerter::new(&config, mock_pagerduty_client).unwrap();

        alerter.alert(String::from("Test alert INFO"), AlertLevel::Info).await;
    
        // Initially, the alert should be in cache
        assert!(alerter.test_is_alert_in_cache("Test alert INFO").await);
    
        // Wait for longer than the expiry duration
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Send another alert so that the cache clears
        alerter.alert(String::from("Test alert NONE"), AlertLevel::None).await;

        // Now, the alert should be expired and not in cache
        assert!(!alerter.test_is_alert_in_cache("Test alert INFO").await);
        assert!(alerter.test_is_alert_in_cache("Test alert NONE").await);
    }

    #[tokio::test]
    async fn test_no_alerts_sent_before_start_time() {
        let config = WatchtowerConfig {
            alert_cache_size: 10,
            alert_cache_expiry: Duration::from_secs(1),
            watchtower_system_name: "TestSystem".to_string(),
            min_duration_from_start_to_err: Duration::from_secs(300),
            ..Default::default()
        };
    
        let mut mock_http_poster: MockHttpPoster = MockHttpPoster::new();
        mock_http_poster.expect_post()
            .times(0) // Expect no calls as the start time has not elapsed
            .returning(|_, _| Box::pin(async { Ok(()) }));

        let alerter = WatchtowerAlerter::new(
            &config,
             PagerDutyClient::new("test_api_key".to_string(), Arc::new(mock_http_poster)),
            ).unwrap();
    
        // Attempt to send an alert and since no errors are triggered `mock_http_poster` was not executed.
        alerter.alert(String::from("Test alert ERROR"), AlertLevel::Error).await;
        alerter.alert(String::from("Test alert WARNING"), AlertLevel::Warn).await;
    }
}
