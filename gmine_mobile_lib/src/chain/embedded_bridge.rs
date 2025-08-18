/// Embedded Node.js bridge client for EIP-712 signing on Android
/// This communicates with the embedded Node.js bridge running via nodejs-mobile
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use reqwest::Client;
use crate::chain::bridge_client::{SignRequest, SignResponse, MessageData, Coin};

/// Client for communicating with the embedded Node.js bridge
#[derive(Clone)]
pub struct EmbeddedBridgeClient {
    client: Client,
    base_url: String,
    is_bridge_running: bool,
}

impl EmbeddedBridgeClient {
    /// Create a new embedded bridge client
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: "http://localhost:8080".to_string(),
            is_bridge_running: false,
        }
    }

    /// Start the embedded Node.js bridge - assumes Android service is already running
    pub async fn start_bridge(&mut self, _mnemonic: &str) -> Result<()> {
        log::info!("Connecting to embedded Node.js bridge (Android service should already be running)...");
        
        // Wait for bridge to become available - Android service should have started it
        self.wait_for_bridge_ready().await?;
        self.is_bridge_running = true;
        
        log::info!("Embedded Node.js bridge connected successfully");
        Ok(())
    }

    /// Wait for the embedded bridge to become ready
    async fn wait_for_bridge_ready(&self) -> Result<()> {
        let max_attempts = 30; // 30 seconds max
        
        for i in 0..max_attempts {
            if self.health_check().await.unwrap_or(false) {
                log::info!("Embedded bridge is ready");
                return Ok(());
            }
            
            if i < max_attempts - 1 {
                log::info!("Waiting for embedded bridge... attempt {}/{}", i + 1, max_attempts);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        
        Err(anyhow!("Embedded bridge failed to become ready after {} seconds", max_attempts))
    }

    /// Check if the embedded bridge is healthy
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        
        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Sign and broadcast a transaction via the embedded bridge
    pub async fn sign_and_broadcast(
        &self,
        chain_id: String,
        account_number: u64,
        sequence: u64,
        contract_address: &str,
        msg: serde_json::Value,
        funds: Vec<Coin>,
        gas_limit: u64,
    ) -> Result<String> {
        if !self.is_bridge_running {
            return Err(anyhow!("Embedded bridge is not running. Call start_bridge() first."));
        }

        let request_id = uuid::Uuid::new_v4().to_string();
        
        let sign_request = SignRequest {
            chain_id,
            account_number,
            sequence,
            messages: vec![MessageData {
                contract: contract_address.to_string(),
                msg,
                funds,
            }],
            gas_limit,
            gas_price: "500000000inj".to_string(),
            memo: String::new(),
            request_id: request_id.clone(),
        };

        let url = format!("{}/sign-and-broadcast", self.base_url);
        
        log::debug!("Sending sign request to embedded bridge: {:?}", sign_request);
        
        let response = self.client
            .post(&url)
            .json(&sign_request)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Embedded bridge request failed: {}", error_text));
        }

        let sign_response: SignResponse = response.json().await?;
        
        if sign_response.success {
            sign_response.tx_hash
                .ok_or_else(|| anyhow!("Success but no tx_hash returned"))
        } else {
            Err(anyhow!("Signing failed: {}", 
                sign_response.error.unwrap_or_else(|| "Unknown error".to_string())))
        }
    }

    /// Stop the embedded bridge
    pub async fn stop_bridge(&mut self) -> Result<()> {
        log::info!("Stopping embedded Node.js bridge...");
        
        // This will be called via JNI to stop the Android service
        // For now, just mark as not running
        self.is_bridge_running = false;
        
        log::info!("Embedded Node.js bridge stopped");
        Ok(())
    }

    /// Get bridge status
    pub fn is_running(&self) -> bool {
        self.is_bridge_running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = EmbeddedBridgeClient::new();
        assert!(!client.is_running());
        assert_eq!(client.base_url, "http://localhost:8080");
    }
}