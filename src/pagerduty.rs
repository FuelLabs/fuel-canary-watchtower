use reqwest::{Client, Error};
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct PagerDutyClient {
    api_key: String,
    http_client: Client,
}

#[derive(Serialize)]
struct PagerDutyPayload {
    payload: PagerDutyEventPayload,
    routing_key: String,
    event_action: String,
}

#[derive(Serialize)]
struct PagerDutyEventPayload {
    summary: String,
    severity: String,
    source: String,
}

impl PagerDutyClient {
    pub fn new(api_key: String) -> Self {
        PagerDutyClient {
            api_key,
            http_client: Client::new(),
        }
    }

    pub async fn send_alert(&self, severity: String, summary: String, source: String) -> Result<(), Error> {
        let payload = PagerDutyPayload {
            payload: PagerDutyEventPayload {
                summary,
                severity,
                source,
            },
            routing_key: self.api_key.clone(),
            event_action: "trigger".to_string(),
        };

        let response = self.http_client
            .post("https://events.eu.pagerduty.com/v2/enqueue")
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(response.error_for_status().unwrap_err())
        }
    }
}
