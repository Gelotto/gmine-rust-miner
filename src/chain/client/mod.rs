use anyhow::Result;
use tonic::transport::{Channel, Endpoint};
use std::time::Duration;
use crate::chain::proto::{MsgExecuteContract, Coin};
use crate::chain::wallet::InjectiveWallet;
use serde_json::Value;

/// Configuration for the Injective gRPC client
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// gRPC endpoint URL (e.g., "https://testnet.sentry.chain.grpc.injective.network:443")
    pub grpc_endpoint: String,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    /// Request timeout in seconds
    pub request_timeout: u64,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Chain ID (e.g., "injective-888" for testnet)
    pub chain_id: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            grpc_endpoint: "https://testnet.sentry.chain.grpc.injective.network:443".to_string(),
            connection_timeout: 10,
            request_timeout: 30,
            max_retries: 3,
            chain_id: "injective-888".to_string(),
        }
    }
}

/// gRPC client for interacting with Injective blockchain
pub struct InjectiveClient {
    config: ClientConfig,
    channel: Option<Channel>,
    wallet: Option<InjectiveWallet>,
}

impl InjectiveClient {
    /// Create a new client with the given configuration
    pub fn new(config: ClientConfig) -> Self {
        Self {
            config,
            channel: None,
            wallet: None,
        }
    }
    
    /// Create a new client with default testnet configuration
    pub fn new_testnet() -> Self {
        Self::new(ClientConfig::default())
    }
    
    /// Set the wallet for this client
    pub fn set_wallet(&mut self, wallet: InjectiveWallet) {
        self.wallet = Some(wallet);
    }
    
    /// Connect to the gRPC endpoint
    pub async fn connect(&mut self) -> Result<()> {
        let endpoint = Endpoint::from_shared(self.config.grpc_endpoint.clone())?
            .timeout(Duration::from_secs(self.config.request_timeout))
            .connect_timeout(Duration::from_secs(self.config.connection_timeout));
        
        let channel = endpoint.connect().await?;
        self.channel = Some(channel);
        
        Ok(())
    }
    
    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        self.channel.is_some()
    }
    
    /// Get the channel for making gRPC calls
    pub fn channel(&self) -> Result<&Channel> {
        self.channel
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not connected. Call connect() first."))
    }
    
    /// Query account information (placeholder - will be implemented with protos)
    pub async fn query_account(&self, _address: &str) -> Result<AccountInfo> {
        // TODO: Implement once protos are compiled
        Ok(AccountInfo {
            address: String::new(),
            sequence: 0,
            account_number: 0,
        })
    }
    
    /// Simulate a transaction (placeholder - will be implemented with protos)
    pub async fn simulate_tx(&self, _tx_bytes: Vec<u8>) -> Result<SimulateResponse> {
        // TODO: Implement once protos are compiled
        Ok(SimulateResponse {
            gas_used: 0,
            gas_wanted: 0,
        })
    }
    
    /// Broadcast a transaction (placeholder - will be implemented with protos)
    pub async fn broadcast_tx(&self, _tx_bytes: Vec<u8>) -> Result<BroadcastResponse> {
        // TODO: Implement once protos are compiled
        Ok(BroadcastResponse {
            tx_hash: String::new(),
            code: 0,
            raw_log: String::new(),
        })
    }
    
    /// Execute a contract message on the Injective blockchain
    pub async fn execute_contract(
        &self,
        contract_address: &str,
        msg: Value,
        funds: Vec<Coin>,
    ) -> Result<String> {
        let wallet = self.wallet.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Wallet not set. Call set_wallet() first."))?;
        
        // Create MsgExecuteContract
        let _execute_msg = MsgExecuteContract {
            sender: wallet.address.clone(),
            contract: contract_address.to_string(),
            msg: serde_json::to_vec(&msg)?,
            funds,
        };
        
        // TODO: Build full transaction with AuthInfo, sign, and broadcast
        // For now, return placeholder
        Ok(format!("tx_hash_placeholder_{}", contract_address))
    }
}

/// Account information from the chain
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub address: String,
    pub sequence: u64,
    pub account_number: u64,
}

/// Response from transaction simulation
#[derive(Debug, Clone)]
pub struct SimulateResponse {
    pub gas_used: u64,
    pub gas_wanted: u64,
}

/// Response from transaction broadcast
#[derive(Debug, Clone)]
pub struct BroadcastResponse {
    pub tx_hash: String,
    pub code: u32,
    pub raw_log: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_client_creation() {
        let client = InjectiveClient::new_testnet();
        assert!(!client.is_connected());
        assert_eq!(client.config.chain_id, "injective-888");
    }
    
    #[tokio::test]
    async fn test_client_config() {
        let config = ClientConfig {
            grpc_endpoint: "https://example.com:443".to_string(),
            connection_timeout: 5,
            request_timeout: 15,
            max_retries: 5,
            chain_id: "test-chain".to_string(),
        };
        
        let client = InjectiveClient::new(config.clone());
        assert_eq!(client.config.grpc_endpoint, "https://example.com:443");
        assert_eq!(client.config.max_retries, 5);
    }
}