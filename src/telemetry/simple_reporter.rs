use anyhow::Result;
use chrono::Utc;
use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Simplified telemetry data matching the production backend schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleTelemetryData {
    pub wallet_address: String,
    pub miner_instance_id: String,
    pub current_epoch: u64,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashrate_mhs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solutions_found: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reveals_submitted: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_balance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error_message: Option<String>,
}

/// Simple telemetry reporter for the production backend
pub struct SimpleTelemetryReporter {
    client: Client,
    endpoint: String,
    wallet_address: String,
    miner_instance_id: String,
}

impl SimpleTelemetryReporter {
    pub fn new(wallet_address: String, miner_instance_id: String) -> Result<Self> {
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .build()?;
        
        // Use production endpoint
        let endpoint = "https://gmine.gelotto.io/api/telemetry".to_string();
        
        Ok(Self {
            client,
            endpoint,
            wallet_address,
            miner_instance_id,
        })
    }

    /// Send telemetry data to the backend
    pub async fn send_telemetry(
        &self,
        epoch: u64,
        hashrate_mhs: Option<f64>,
        solutions_found: Option<u32>,
        reveals_submitted: Option<u32>,
        gas_balance: Option<String>,
        last_error: Option<String>,
    ) -> Result<()> {
        let data = SimpleTelemetryData {
            wallet_address: self.wallet_address.clone(),
            miner_instance_id: self.miner_instance_id.clone(),
            current_epoch: epoch,
            timestamp: Utc::now().to_rfc3339(),
            hashrate_mhs,
            solutions_found,
            reveals_submitted,
            gas_balance,
            last_error_message: last_error,
        };

        debug!("Sending telemetry: epoch={}, hashrate={:?}", epoch, hashrate_mhs);
        
        let response = self.client
            .post(&self.endpoint)
            .json(&data)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    debug!("Telemetry sent successfully");
                    Ok(())
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    warn!("Telemetry backend returned error {}: {}", status, body);
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
        info!("Testing telemetry connection to {}", self.endpoint);
        
        let health_endpoint = "https://gmine.gelotto.io/api/health";
        
        match self.client.get(health_endpoint).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    info!("Telemetry backend is healthy");
                    Ok(true)
                } else {
                    warn!("Telemetry backend returned status: {}", resp.status());
                    Ok(false)
                }
            }
            Err(e) => {
                warn!("Failed to connect to telemetry backend: {}", e);
                Ok(false)
            }
        }
    }

    /// Quick helper to send periodic updates
    pub async fn send_periodic_update(
        &self,
        epoch: u64,
        hashrate: f64,
        solutions_this_epoch: u32,
        reveals_this_epoch: u32,
    ) -> Result<()> {
        self.send_telemetry(
            epoch,
            Some(hashrate),
            Some(solutions_this_epoch),
            Some(reveals_this_epoch),
            None, // Gas balance will be fetched separately
            None, // No error
        ).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_telemetry_connection() {
        let reporter = SimpleTelemetryReporter::new(
            "inj1test123".to_string(),
            "test-miner-001".to_string(),
        ).unwrap();
        
        let connected = reporter.test_connection().await.unwrap();
        assert!(connected, "Should connect to telemetry backend");
    }

    #[tokio::test]
    async fn test_send_telemetry() {
        let reporter = SimpleTelemetryReporter::new(
            "inj1test123".to_string(),
            "test-miner-001".to_string(),
        ).unwrap();
        
        reporter.send_periodic_update(
            123,
            45.2,
            3,
            2,
        ).await.unwrap();
    }
}