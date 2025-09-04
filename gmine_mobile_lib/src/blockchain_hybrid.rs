// Hybrid blockchain client for GMINE Mobile
// Uses Injective LCD for queries and local Node.js bridge for signing

use once_cell::sync::OnceCell;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use base64;
use base64::Engine;

// Configuration
const MINING_CONTRACT: &str = "inj1h2rq8q2ly6mwgwv4jcd5qpjvfqwvwee5v9n032"; // V3.4 contract with JIT History fix
const BRIDGE_PORT: u16 = 7777; // Local Node.js bridge port

// --- Data Structures ---

#[derive(Serialize)]
pub struct InitRequest<'a> {
    pub mnemonic: &'a str,
}

#[derive(Serialize)]
pub struct CommitRequest<'a> {
    pub commitment_hash: &'a str,
}

#[derive(Serialize)]
pub struct RevealRequest<'a> {
    pub nonce: u64,
    pub digest: &'a str,
    pub salt: &'a str,
}

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
    pub gas_used: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub struct BridgeResponse {
    pub success: Option<bool>,
    pub tx_hash: Option<String>,
    pub gas_used: Option<u64>,
    pub error: Option<String>,
    pub address: Option<String>,
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
    lcd_url: String,
    bridge_url: String,
    mnemonic: String,
}

static CLIENT: OnceCell<BlockchainClient> = OnceCell::new();

impl BlockchainClient {
    pub fn initialize(lcd_url: String, mnemonic: String) -> Result<(), &'static str> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| "Failed to build HTTP client")?;

        let bridge_url = format!("http://127.0.0.1:{}", BRIDGE_PORT);
        
        let client = BlockchainClient { 
            http_client, 
            lcd_url,
            bridge_url,
            mnemonic,
        };
        
        CLIENT.set(client).map_err(|_| "Client already initialized")
    }

    fn get() -> Result<&'static Self, BlockchainClientError> {
        CLIENT.get().ok_or(BlockchainClientError::NotInitialized)
    }

    /// Initialize the bridge with the wallet mnemonic
    pub fn init_bridge() -> Result<String, BlockchainClientError> {
        let client = Self::get()?;
        
        let request = InitRequest {
            mnemonic: &client.mnemonic,
        };
        
        let url = format!("{}/init", client.bridge_url);
        let response = client
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .map_err(|e| BlockchainClientError::Network(e))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().unwrap_or_else(|_| "Bridge init failed".to_string());
            return Err(BlockchainClientError::ApiError { status, message });
        }

        let result: BridgeResponse = response
            .json()
            .map_err(|e| BlockchainClientError::Network(e))?;

        match result.address {
            Some(addr) => Ok(addr),
            None => Err(BlockchainClientError::ApiError {
                status: 500,
                message: result.error.unwrap_or_else(|| "Unknown error".to_string()),
            }),
        }
    }

    /// Query current epoch from Injective LCD
    pub fn get_current_epoch() -> Result<EpochInfo, BlockchainClientError> {
        let client = Self::get()?;
        
        // Create query message
        let query_msg = serde_json::json!({
            "current_epoch": {}
        });
        
        // Base64 encode the query
        let query_data = base64::engine::general_purpose::STANDARD.encode(query_msg.to_string());
        
        // LCD endpoint for smart contract queries
        let url = format!(
            "{}/cosmwasm/wasm/v1/contract/{}/smart/{}",
            client.lcd_url,
            MINING_CONTRACT,
            query_data
        );

        log::debug!("Querying epoch from LCD: {}", url);

        let response = client
            .http_client
            .get(&url)
            .send()
            .map_err(|e| BlockchainClientError::Network(e))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().unwrap_or_else(|_| "Unknown error".to_string());
            log::error!("Failed to get epoch: {} - {}", status, message);
            return Err(BlockchainClientError::ApiError { status, message });
        }

        let result: serde_json::Value = response
            .json()
            .map_err(|e| BlockchainClientError::Network(e))?;

        // Extract data from LCD response wrapper
        let epoch_data = &result["data"];
        
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
            .ok_or_else(|| BlockchainClientError::ApiError {
                status: 500,
                message: "Missing target_hash in response".to_string()
            })?;
        
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

    /// Submit commitment via local bridge
    pub fn submit_commit(commitment_hash: &str) -> Result<TransactionResponse, BlockchainClientError> {
        let client = Self::get()?;
        
        let request = CommitRequest { commitment_hash };
        
        let url = format!("{}/commit", client.bridge_url);
        log::info!("Submitting commitment via bridge");
        
        let response = client
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .map_err(|e| BlockchainClientError::Network(e))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().unwrap_or_else(|_| "Commit failed".to_string());
            return Err(BlockchainClientError::ApiError { status, message });
        }

        let result: BridgeResponse = response
            .json()
            .map_err(|e| BlockchainClientError::Network(e))?;

        match (result.tx_hash, result.error) {
            (Some(hash), _) => Ok(TransactionResponse {
                tx_hash: hash,
                gas_used: result.gas_used,
            }),
            (None, Some(error)) => Err(BlockchainClientError::ApiError {
                status: 500,
                message: error,
            }),
            _ => Err(BlockchainClientError::ApiError {
                status: 500,
                message: "Unknown error".to_string(),
            }),
        }
    }

    /// Submit reveal via local bridge
    pub fn submit_reveal(nonce: u64, digest: &str, salt: &str) -> Result<TransactionResponse, BlockchainClientError> {
        let client = Self::get()?;
        
        let request = RevealRequest { nonce, digest, salt };
        
        let url = format!("{}/reveal", client.bridge_url);
        log::info!("Submitting reveal via bridge");
        
        let response = client
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .map_err(|e| BlockchainClientError::Network(e))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().unwrap_or_else(|_| "Reveal failed".to_string());
            return Err(BlockchainClientError::ApiError { status, message });
        }

        let result: BridgeResponse = response
            .json()
            .map_err(|e| BlockchainClientError::Network(e))?;

        match (result.tx_hash, result.error) {
            (Some(hash), _) => Ok(TransactionResponse {
                tx_hash: hash,
                gas_used: result.gas_used,
            }),
            (None, Some(error)) => Err(BlockchainClientError::ApiError {
                status: 500,
                message: error,
            }),
            _ => Err(BlockchainClientError::ApiError {
                status: 500,
                message: "Unknown error".to_string(),
            }),
        }
    }
}

// --- Public convenience functions ---

pub fn init_blockchain_client(lcd_url: &str, mnemonic: &str) -> Result<(), String> {
    BlockchainClient::initialize(lcd_url.to_string(), mnemonic.to_string())?;
    
    // Wait a bit for bridge to be ready
    std::thread::sleep(Duration::from_secs(2));
    
    // Initialize the bridge
    match BlockchainClient::init_bridge() {
        Ok(address) => {
            log::info!("Bridge initialized for wallet: {}", address);
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to initialize bridge: {}", e);
            Err(format!("Bridge init failed: {}", e))
        }
    }
}

pub fn get_current_epoch() -> Result<EpochInfo, String> {
    BlockchainClient::get_current_epoch()
        .map_err(|e| format!("Failed to get epoch: {}", e))
}

pub fn submit_commitment(
    _wallet: &str, // Not needed - bridge knows the wallet
    _epoch: u64,   // Not needed - contract tracks epoch
    commitment: &str,
) -> Result<String, String> {
    BlockchainClient::submit_commit(commitment)
        .map(|resp| resp.tx_hash)
        .map_err(|e| format!("Failed to submit commitment: {}", e))
}

pub fn submit_reveal(
    _wallet: &str,
    _epoch: u64,
    nonce: u64,
    digest: &str,
    salt: &str,
) -> Result<String, String> {
    BlockchainClient::submit_reveal(nonce, digest, salt)
        .map(|resp| resp.tx_hash)
        .map_err(|e| format!("Failed to submit reveal: {}", e))
}