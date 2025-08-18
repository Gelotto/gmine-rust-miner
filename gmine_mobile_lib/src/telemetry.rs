use anyhow::Result;
use chrono::Utc;
use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::mining::MiningStats;

/// Telemetry data structure matching the production backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileTelemetryData {
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
    // Mobile-specific fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thermal_throttled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_level: Option<f32>,
}

/// Telemetry reporter for mobile mining
pub struct TelemetryReporter {
    client: Client,
    endpoint: String,
    wallet_address: String,
    miner_instance_id: String,
    device_type: String,
}

impl TelemetryReporter {
    pub async fn new(wallet_address: String) -> Result<Self> {
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(15)) // Longer timeout for mobile networks
            .build()?;
        
        // Use production telemetry endpoint
        let endpoint = "https://gmine.gelotto.io/api/telemetry".to_string();
        
        // Generate unique instance ID for this mobile session
        let miner_instance_id = format!("mobile-{}", Uuid::new_v4().to_string()[..8].to_string());
        
        let device_type = Self::get_device_type();
        
        Ok(Self {
            client,
            endpoint,
            wallet_address,
            miner_instance_id,
            device_type,
        })
    }

    /// Send mining statistics as telemetry
    pub async fn send_stats(&self, stats: &MiningStats) -> Result<()> {
        let data = MobileTelemetryData {
            wallet_address: self.wallet_address.clone(),
            miner_instance_id: self.miner_instance_id.clone(),
            current_epoch: stats.epoch,
            timestamp: Utc::now().to_rfc3339(),
            hashrate_mhs: Some(stats.hashrate),
            solutions_found: Some(stats.solutions_found as u32),
            reveals_submitted: None, // Mobile mining doesn't track reveals separately yet
            gas_balance: None, // TODO: Implement mobile gas balance checking
            last_error_message: None,
            device_type: Some(self.device_type.clone()),
            thermal_throttled: Some(stats.thermal_throttled),
            battery_level: Self::get_battery_level().await,
        };

        debug!("Sending mobile telemetry: epoch={}, hashrate={:.2} MH/s", 
               stats.epoch, stats.hashrate);
        
        let response = self.client
            .post(&self.endpoint)
            .json(&data)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    debug!("Mobile telemetry sent successfully");
                    Ok(())
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    warn!("Telemetry backend returned error {}: {}", status, body);
                    // Don't fail mobile mining if telemetry fails
                    Ok(())
                }
            }
            Err(e) => {
                warn!("Failed to send mobile telemetry: {}", e);
                // Don't fail mobile mining if telemetry fails
                Ok(())
            }
        }
    }

    /// Test connection to telemetry backend
    pub async fn test_connection(&self) -> Result<bool> {
        let health_endpoint = "https://gmine.gelotto.io/api/health";
        
        match self.client.get(health_endpoint).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Send error telemetry from mobile
    pub async fn send_error(&self, error_message: &str, epoch: u64) -> Result<()> {
        let data = MobileTelemetryData {
            wallet_address: self.wallet_address.clone(),
            miner_instance_id: self.miner_instance_id.clone(),
            current_epoch: epoch,
            timestamp: Utc::now().to_rfc3339(),
            hashrate_mhs: None,
            solutions_found: None,
            reveals_submitted: None,
            gas_balance: None,
            last_error_message: Some(error_message.to_string()),
            device_type: Some(self.device_type.clone()),
            thermal_throttled: None,
            battery_level: Self::get_battery_level().await,
        };

        let _ = self.client
            .post(&self.endpoint)
            .json(&data)
            .send()
            .await;

        Ok(())
    }

    fn get_device_type() -> String {
        // In a real implementation, we would determine the actual device model
        #[cfg(target_os = "android")]
        {
            "Android Mobile".to_string()
        }
        #[cfg(not(target_os = "android"))]
        {
            "Mobile Device".to_string()
        }
    }

    async fn get_battery_level() -> Option<f32> {
        // In a real implementation, we would use Android APIs to get battery level
        // For now, return None to indicate unavailable
        None
    }
}

impl Clone for TelemetryReporter {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            endpoint: self.endpoint.clone(),
            wallet_address: self.wallet_address.clone(),
            miner_instance_id: self.miner_instance_id.clone(),
            device_type: self.device_type.clone(),
        }
    }
}