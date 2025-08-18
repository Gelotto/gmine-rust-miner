// rust/src/blockchain_client.rs

use once_cell::sync::OnceCell;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// --- Data Structures (Request & Response Payloads) ---

#[derive(Serialize)]
pub struct CommitRequest<'a> {
    pub wallet_address: &'a str,
    pub epoch: u64,
    pub commitment_hash: &'a str,
}

#[derive(Serialize)]
pub struct RevealRequest<'a> {
    pub wallet_address: &'a str,
    pub epoch: u64,
    pub nonce: u64,
    pub digest: &'a str,
    pub salt: &'a str,
}

#[derive(Serialize)]
pub struct ClaimRequest<'a> {
    pub wallet_address: &'a str,
    pub epoch: u64,
}

#[derive(Deserialize, Debug)]
pub struct EpochInfo {
    pub id: u64,
    pub challenge: String,
    pub difficulty: u32,
    pub phase: String, // "commit", "reveal", or "complete"
    pub ends_at: u64, // timestamp when epoch ends
}

#[derive(Deserialize, Debug)]
pub struct TransactionResponse {
    pub tx_hash: String,
    pub status: String,
    pub gas_used: Option<u64>,
}

// --- Custom Error Type ---
#[derive(Debug)]
pub enum BlockchainClientError {
    /// The client has not been initialized with a base URL.
    NotInitialized,
    /// An error occurred during the HTTP request (e.g., network issue, timeout).
    Network(reqwest::Error),
    /// The API returned a non-successful status code (e.g., 4xx, 5xx).
    ApiError { status: u16, message: String },
    /// Failed to deserialize the response body.
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

// Use OnceCell for a thread-safe, one-time global initialization.
static CLIENT: OnceCell<BlockchainClient> = OnceCell::new();

impl BlockchainClient {
    /// Initializes the global blockchain client. This must be called once, typically
    /// from the JNI `initialize` function.
    pub fn initialize(base_url: String) -> Result<(), &'static str> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30)) // Mobile networks can be slow
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| "Failed to build HTTP client")?;

        let client = BlockchainClient { http_client, base_url };

        CLIENT.set(client).map_err(|_| "Client already initialized")
    }

    /// Gets a reference to the initialized client.
    fn get() -> Result<&'static Self, BlockchainClientError> {
        CLIENT.get().ok_or(BlockchainClientError::NotInitialized)
    }

    /// Fetches the current epoch information from the proxy.
    pub fn get_current_epoch() -> Result<EpochInfo, BlockchainClientError> {
        let client = Self::get()?;
        let url = format!("{}/api/epoch", client.base_url);

        log::debug!("Fetching current epoch from: {}", url);

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

        response
            .json::<EpochInfo>()
            .map_err(BlockchainClientError::Deserialization)
    }

    /// Submits a commitment to the proxy.
    pub fn submit_commit(request: &CommitRequest) -> Result<TransactionResponse, BlockchainClientError> {
        let client = Self::get()?;
        let url = format!("{}/api/commit", client.base_url);
        
        log::info!(
            "Submitting commitment for epoch {} from wallet {}",
            request.epoch,
            request.wallet_address
        );
        
        client.post_json(&url, request)
    }

    /// Submits a solution reveal to the proxy.
    pub fn submit_reveal(request: &RevealRequest) -> Result<TransactionResponse, BlockchainClientError> {
        let client = Self::get()?;
        let url = format!("{}/api/reveal", client.base_url);

        log::info!(
            "Submitting reveal for epoch {} from wallet {}",
            request.epoch,
            request.wallet_address
        );

        client.post_json(&url, request)
    }

    /// Submits a claim request to the proxy.
    pub fn claim_rewards(request: &ClaimRequest) -> Result<TransactionResponse, BlockchainClientError> {
        let client = Self::get()?;
        let url = format!("{}/api/claim", client.base_url);

        log::info!(
            "Claiming rewards for epoch {} from wallet {}",
            request.epoch,
            request.wallet_address
        );

        client.post_json(&url, request)
    }
    
    // Helper function to reduce boilerplate for POST requests.
    fn post_json<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<R, BlockchainClientError> {
        let response = self
            .http_client
            .post(url)
            .json(body)
            .send()
            .map_err(BlockchainClientError::Network)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().unwrap_or_else(|_| "Unknown error".to_string());
            log::error!("API request failed: {} - {}", status, message);
            return Err(BlockchainClientError::ApiError { status, message });
        }

        response
            .json::<R>()
            .map_err(BlockchainClientError::Deserialization)
    }

    /// Checks if the client is initialized and the server is reachable
    pub fn health_check() -> Result<bool, BlockchainClientError> {
        let client = Self::get()?;
        let url = format!("{}/health", client.base_url);

        match client.http_client.get(&url).send() {
            Ok(response) => Ok(response.status().is_success()),
            Err(e) => {
                log::warn!("Health check failed: {}", e);
                Err(BlockchainClientError::Network(e))
            }
        }
    }
}

// --- Public convenience functions ---

/// Initialize the blockchain client with the proxy URL
pub fn init_blockchain_client(proxy_url: &str) -> Result<(), String> {
    BlockchainClient::initialize(proxy_url.to_string())
}

/// Get current epoch information
pub fn get_current_epoch() -> Result<EpochInfo, String> {
    BlockchainClient::get_current_epoch()
        .map_err(|e| format!("Failed to get epoch: {}", e))
}

/// Submit a mining commitment
pub fn submit_commitment(
    wallet: &str,
    epoch: u64,
    commitment: &str,
) -> Result<String, String> {
    let request = CommitRequest {
        wallet_address: wallet,
        epoch,
        commitment_hash: commitment,
    };
    
    BlockchainClient::submit_commit(&request)
        .map(|resp| resp.tx_hash)
        .map_err(|e| format!("Failed to submit commitment: {}", e))
}

/// Submit a mining reveal
pub fn submit_reveal(
    wallet: &str,
    epoch: u64,
    nonce: u64,
    digest: &str,
    salt: &str,
) -> Result<String, String> {
    let request = RevealRequest {
        wallet_address: wallet,
        epoch,
        nonce,
        digest,
        salt,
    };
    
    BlockchainClient::submit_reveal(&request)
        .map(|resp| resp.tx_hash)
        .map_err(|e| format!("Failed to submit reveal: {}", e))
}