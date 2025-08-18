use anyhow::Result;
use reqwest::{Client, ClientBuilder};
use std::time::Duration;
use tracing::{debug, warn};

use super::types::*;

/// Handles reporting telemetry data to the backend
pub struct TelemetryReporter {
    client: Client,
    endpoint: String,
}

impl TelemetryReporter {
    pub fn new(endpoint: String, timeout_secs: u64) -> Result<Self> {
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(timeout_secs))
            .build()?;
        
        Ok(Self {
            client,
            endpoint,
        })
    }

    /// Send a batch of events to the telemetry backend
    pub async fn send_batch(&self, events: Vec<MinerEvent>) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        debug!("Sending telemetry batch of {} events", events.len());
        
        let batch = TelemetryBatch {
            events,
            version: "1.0".to_string(),
        };

        let response = self.client
            .post(&self.endpoint)
            .json(&batch)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    debug!("Telemetry batch sent successfully");
                    Ok(())
                } else {
                    warn!("Telemetry backend returned error: {}", resp.status());
                    // Don't fail the miner if telemetry fails
                    Ok(())
                }
            }
            Err(e) => {
                warn!("Failed to send telemetry: {}", e);
                // Don't fail the miner if telemetry fails
                Ok(())
            }
        }
    }

    /// Test connectivity to the telemetry backend
    pub async fn test_connection(&self) -> Result<bool> {
        let health_endpoint = format!("{}/health", self.endpoint.trim_end_matches('/'));
        
        match self.client.get(&health_endpoint).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}