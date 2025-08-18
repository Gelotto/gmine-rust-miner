/// Client for communicating with the Go bridge signing service
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use reqwest::Client;

#[derive(Debug, Serialize)]
pub struct SignRequest {
    pub chain_id: String,
    pub account_number: u64,
    pub sequence: u64,
    pub messages: Vec<MessageData>,
    pub gas_limit: u64,
    pub gas_price: String,
    pub memo: String,
    pub request_id: String,
}

#[derive(Debug, Serialize)]
pub struct MessageData {
    pub contract: String,
    pub msg: serde_json::Value,
    pub funds: Vec<Coin>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Coin {
    pub denom: String,
    pub amount: String,
}

#[derive(Debug, Deserialize)]
pub struct SignResponse {
    pub success: bool,
    pub tx_hash: Option<String>,
    pub error: Option<String>,
    pub request_id: String,
}

#[derive(Clone)]
pub struct BridgeClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl BridgeClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url,
            api_key,
        }
    }

    /// Check if the bridge service is healthy
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        
        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Sign and broadcast a transaction
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
        
        let mut req = self.client.post(&url).json(&sign_request);
        
        // Add API key if configured
        if let Some(api_key) = &self.api_key {
            req = req.header("X-API-Key", api_key);
        }

        let response = req.send().await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Bridge request failed: {}", error_text));
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

    /// Wait for the bridge service to become healthy
    pub async fn wait_for_health(&self, max_attempts: u32) -> Result<()> {
        for i in 0..max_attempts {
            if self.health_check().await? {
                log::info!("Bridge service is healthy");
                return Ok(());
            }
            
            if i < max_attempts - 1 {
                log::info!("Waiting for bridge service... attempt {}/{}", i + 1, max_attempts);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        
        Err(anyhow!("Bridge service failed to become healthy after {} attempts", max_attempts))
    }
}