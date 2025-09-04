// Direct blockchain client for Injective LCD API
// This replaces the proxy-based approach with direct LCD calls

use once_cell::sync::OnceCell;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use base64;

// Contract configuration
const MINING_CONTRACT: &str = "inj1h2rq8q2ly6mwgwv4jcd5qpjvfqwvwee5v9n032"; // V3.4 contract with JIT History fix

// --- Data Structures ---

#[derive(Deserialize, Debug)]
pub struct EpochInfo {
    pub id: u64,
    pub challenge: String,
    pub difficulty: u32,
    pub phase: String,
    pub ends_at: u64,
}

#[derive(Deserialize, Debug)]
pub struct TransactionResponse {
    pub tx_hash: String,
    pub status: String,
    pub gas_used: Option<u64>,
}

// LCD Query Response wrapper
#[derive(Deserialize, Debug)]
struct QueryResponse {
    data: serde_json::Value,
}

// --- Custom Error Type ---
#[derive(Debug)]
pub enum BlockchainClientError {
    NotInitialized,
    Network(reqwest::Error),
    ApiError { status: u16, message: String },
    Deserialization(serde_json::Error),
}

impl std::fmt::Display for BlockchainClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockchainClientError::NotInitialized => write!(f, "Blockchain client not initialized"),
            BlockchainClientError::Network(e) => write!(f, "Network error: {}", e),
            BlockchainClientError::ApiError { status, message } => {
                write!(f, "API error ({}): {}", status, message)
            }
            BlockchainClientError::Deserialization(e) => write!(f, "Deserialization error: {}", e),
        }
    }
}

// --- Blockchain Client ---
pub struct BlockchainClient {
    http_client: Client,
    base_url: String,
}

static CLIENT: OnceCell<BlockchainClient> = OnceCell::new();

impl BlockchainClient {
    pub fn initialize(base_url: String) -> Result<(), &'static str> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| "Failed to build HTTP client")?;

        let client = BlockchainClient { http_client, base_url };
        CLIENT.set(client).map_err(|_| "Client already initialized")
    }

    fn get() -> Result<&'static Self, BlockchainClientError> {
        CLIENT.get().ok_or(BlockchainClientError::NotInitialized)
    }

    /// Query current epoch from contract using LCD API
    pub fn get_current_epoch() -> Result<EpochInfo, BlockchainClientError> {
        let client = Self::get()?;
        
        // Create query message
        let query_msg = serde_json::json!({
            "current_epoch": {}
        });
        
        // Base64 encode the query
        let query_data = base64::encode(query_msg.to_string());
        
        // LCD endpoint for smart contract queries
        let url = format!(
            "{}/cosmwasm/wasm/v1/contract/{}/smart/{}",
            client.base_url,
            MINING_CONTRACT,
            query_data
        );

        log::debug!("Querying epoch from: {}", url);

        let response = client
            .http_client
            .get(&url)
            .send()
            .map_err(BlockchainClientError::Network)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().unwrap_or_else(|_| "Unknown error".to_string());
            log::error!("Failed to get epoch: {} - {}", status, message);
            return Err(BlockchainClientError::ApiError { status, message });
        }

        let result: serde_json::Value = response
            .json()
            .map_err(BlockchainClientError::Deserialization)?;

        // Extract data from LCD response wrapper
        let epoch_data = result["data"].clone();
        
        // Parse epoch phase
        let phase = if epoch_data["phase"]["commit"].is_object() {
            "commit"
        } else if epoch_data["phase"]["reveal"].is_object() {
            "reveal"
        } else {
            "settlement"
        };

        // Convert target_hash array to hex string
        let target_hash_array = epoch_data["target_hash"].as_array()
            .ok_or_else(|| BlockchainClientError::Deserialization(
                serde_json::Error::custom("Missing target_hash")
            ))?;
        
        let target_hash_bytes: Vec<u8> = target_hash_array
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u8))
            .collect();
        
        let challenge = format!("0x{}", hex::encode(&target_hash_bytes));

        Ok(EpochInfo {
            id: epoch_data["epoch_number"].as_u64().unwrap_or(0),
            challenge,
            difficulty: epoch_data["difficulty"].as_u64().unwrap_or(8) as u32,
            phase: phase.to_string(),
            ends_at: 0, // Would need block height query to calculate
        })
    }

    /// Note: Transaction submission would still need the EIP-712 bridge
    /// This is just a placeholder to maintain the same interface
    pub fn submit_commitment(
        _wallet: &str,
        _epoch: u64,
        _commitment: &str,
    ) -> Result<String, String> {
        Err("Transaction submission requires EIP-712 bridge".to_string())
    }

    pub fn submit_reveal(
        _wallet: &str,
        _epoch: u64,
        _nonce: u64,
        _digest: &str,
        _salt: &str,
    ) -> Result<String, String> {
        Err("Transaction submission requires EIP-712 bridge".to_string())
    }
}

// --- Public convenience functions ---

pub fn init_blockchain_client(lcd_url: &str) -> Result<(), String> {
    BlockchainClient::initialize(lcd_url.to_string())
}

pub fn get_current_epoch() -> Result<EpochInfo, String> {
    BlockchainClient::get_current_epoch()
        .map_err(|e| format!("Failed to get epoch: {}", e))
}

pub fn submit_commitment(
    wallet: &str,
    epoch: u64,
    commitment: &str,
) -> Result<String, String> {
    BlockchainClient::submit_commitment(wallet, epoch, commitment)
}

pub fn submit_reveal(
    wallet: &str,
    epoch: u64,
    nonce: u64,
    digest: &str,
    salt: &str,
) -> Result<String, String> {
    BlockchainClient::submit_reveal(wallet, epoch, nonce, digest, salt)
}