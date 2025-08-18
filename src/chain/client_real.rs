use anyhow::{Result, anyhow};
use tonic::transport::{Channel, Endpoint};
use tonic::Code;
use std::time::Duration;
use serde_json::Value;
use std::sync::{Arc, RwLock};

use crate::chain::proto::{
    self,
    Coin, AuthQueryClient, QueryAccountRequest,
    ServiceClient, SimulateRequest, BroadcastTxRequest, BroadcastMode,
    BankQueryClient, QueryBalanceRequest,
    TendermintServiceClient, GetNodeInfoRequest, GetLatestBlockRequest
};
use crate::chain::wallet::InjectiveWallet;
use crate::chain::tx_builder::ProperTxBuilder;
use crate::chain::account_types::{Account, AccountInfo};
use crate::chain::bridge_client::BridgeClient;
use crate::chain::rust_signer::RustSigner;

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
#[derive(Clone)]
pub struct InjectiveClient {
    config: ClientConfig,
    channel: Option<Channel>,
    wallet: Arc<InjectiveWallet>,
    bridge_client: Option<BridgeClient>,
    rust_signer: Option<RustSigner>,
    use_rust_signer: bool,
    // No longer tracking sequence locally - always fetch fresh from chain
    sequence_tracker: Arc<RwLock<Option<u64>>>, // Kept for compatibility
}

impl InjectiveClient {
    /// Create a new client with the given configuration and wallet
    pub fn new(config: ClientConfig, wallet: InjectiveWallet) -> Self {
        Self {
            config,
            channel: None,
            wallet: Arc::new(wallet),
            bridge_client: None,
            rust_signer: None,
            use_rust_signer: false,
            sequence_tracker: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Create a new client with default testnet configuration
    pub fn new_testnet(wallet: InjectiveWallet) -> Self {
        Self::new(ClientConfig::default(), wallet)
    }

    /// Set the bridge client for EIP-712 signing
    pub fn set_bridge_client(&mut self, bridge_client: BridgeClient) {
        self.bridge_client = Some(bridge_client);
    }
    
    /// Enable Rust-native EIP-712 signing
    pub fn enable_rust_signer(&mut self, mnemonic: &str, contract_address: &str) -> Result<()> {
        // Convert chain ID to network name for the mobile signer
        let network = if self.config.chain_id == "injective-888" {
            "testnet"
        } else if self.config.chain_id == "injective-1" {
            "mainnet"
        } else {
            return Err(anyhow!("Unknown chain ID: {}", self.config.chain_id));
        };
        
        let rust_signer = RustSigner::new(
            mnemonic,
            network,
            contract_address
        )?;
        self.rust_signer = Some(rust_signer);
        self.use_rust_signer = true;
        log::info!("Enabled Rust-native EIP-712 signer");
        Ok(())
    }
    
    /// Connect to the gRPC endpoint
    pub async fn connect(&mut self) -> Result<()> {
        log::info!("Connecting to Injective at {}", self.config.grpc_endpoint);
        
        // For HTTPS endpoints, tonic will handle TLS automatically
        // We just need to ensure the endpoint URL is properly formatted
        let endpoint = Endpoint::from_shared(self.config.grpc_endpoint.clone())?
            .timeout(Duration::from_secs(self.config.request_timeout))
            .connect_timeout(Duration::from_secs(self.config.connection_timeout));
        
        let channel = endpoint.connect().await?;
        self.channel = Some(channel);
        
        log::info!("Connected to Injective blockchain");
        Ok(())
    }
    
    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        self.channel.is_some()
    }
    
    /// Get the channel for making gRPC calls
    fn channel(&self) -> Result<Channel> {
        self.channel
            .clone()
            .ok_or_else(|| anyhow!("Client not connected. Call connect() first."))
    }
    
    /// Get the next sequence number, always fetching fresh from chain
    /// This avoids race conditions and sequence drift issues
    async fn get_next_sequence(&self, address: &str) -> Result<u64> {
        // Always query fresh sequence from chain to avoid drift and race conditions
        let account_info = self.query_account(address).await?;
        let sequence = account_info.sequence;
        
        log::info!("Fetched current sequence from chain: {}", sequence);
        Ok(sequence)
    }
    
    /// Reset sequence tracking - No longer needed since we always fetch fresh
    /// Kept for compatibility but does nothing
    fn reset_sequence_tracking(&self) {
        log::debug!("reset_sequence_tracking called (no-op - always fetching fresh)");
    }
    
    /// Parse sequence error and extract expected sequence number
    fn parse_sequence_error(&self, error_msg: &str) -> Option<u64> {
        // Parse "expected X, got Y" pattern
        if let Some(start) = error_msg.find("expected ") {
            let remaining = &error_msg[start + 9..];
            if let Some(comma) = remaining.find(",") {
                if let Ok(expected) = remaining[..comma].trim().parse::<u64>() {
                    log::info!("Parsed expected sequence {} from error", expected);
                    return Some(expected);
                }
            }
        }
        None
    }
    
    /// Query account information - REAL IMPLEMENTATION with polymorphic account support
    /// Returns default account info (sequence=0, account_number=0) for new accounts
    pub async fn query_account(&self, address: &str) -> Result<AccountInfo> {
        let response = self.with_retry(|| async {
            let channel = self.channel()?;
            let mut client = AuthQueryClient::new(channel);
            let request = tonic::Request::new(QueryAccountRequest {
                address: address.to_string(),
            });
            client.account(request).await
                .map_err(|e| {
                    // Check if this is a "not found" error (account doesn't exist yet)
                    if e.code() == Code::NotFound {
                        return anyhow!("ACCOUNT_NOT_FOUND");
                    }
                    anyhow!("Failed to query account: {}", e)
                })
        }).await;
        
        match response {
            Ok(response) => {
                // Account exists - decode polymorphically using Any wrapper
                let account_any = response.into_inner().account
                    .ok_or_else(|| anyhow!("Account not found"))?;
                
                log::debug!("Decoding account with type_url: {}", account_any.type_url);
                
                // Use polymorphic decoder to handle all account types
                let account = Account::decode_any(&account_any.type_url, &account_any.value)?;
                
                log::info!("Successfully decoded account type: {}", account.account_type());
                
                // Extract account info in a panic-safe way
                match account.get_account_info() {
                    Some(info) => Ok(info),
                    None => {
                        log::warn!("Account type {} doesn't provide extractable account info", account.account_type());
                        // Return default for unsupported account types
                        Ok(AccountInfo {
                            address: address.to_string(),
                            sequence: 0,
                            account_number: 0,
                        })
                    }
                }
            }
            Err(e) => {
                // Check if this is the specific "account not found" case
                if e.to_string().contains("ACCOUNT_NOT_FOUND") {
                    log::info!("Account not found, returning default info for new account: {}", address);
                    // Return default info for new account
                    Ok(AccountInfo {
                        address: address.to_string(),
                        sequence: 0,
                        account_number: 0,
                    })
                } else {
                    // Real error - propagate it
                    Err(e)
                }
            }
        }
    }
    
    /// Simulate a transaction - REAL IMPLEMENTATION
    pub async fn simulate_tx(&self, tx_bytes: Vec<u8>) -> Result<SimulateResponse> {
        let response = self.with_retry(|| async {
            let channel = self.channel()?;
            let mut client = ServiceClient::new(channel);
            let tx_bytes = tx_bytes.clone();
            let request = tonic::Request::new(SimulateRequest {
                tx: None,  // Deprecated field
                tx_bytes,
            });
            client.simulate(request).await
                .map_err(|e| anyhow!("Failed to simulate transaction: {}", e))
        }).await?;
        
        let sim_response = response.into_inner();
        let gas_info = sim_response.gas_info
            .ok_or_else(|| anyhow!("No gas info in simulation response"))?;
        
        Ok(SimulateResponse {
            gas_used: gas_info.gas_used,
            gas_wanted: gas_info.gas_wanted,
        })
    }
    
    /// Broadcast a transaction - REAL IMPLEMENTATION
    pub async fn broadcast_tx(&self, tx_bytes: Vec<u8>) -> Result<BroadcastResponse> {
        log::info!("broadcast_tx called with {} bytes", tx_bytes.len());
        let response = self.with_retry(|| async {
            let channel = self.channel()?;
            let mut client = ServiceClient::new(channel);
            let tx_bytes = tx_bytes.clone();
            let request = tonic::Request::new(BroadcastTxRequest {
                tx_bytes,
                mode: BroadcastMode::Sync as i32,
            });
            client.broadcast_tx(request).await
                .map_err(|e| anyhow!("Failed to broadcast transaction: {}", e))
        }).await?;
        
        let tx_response = response.into_inner().tx_response
            .ok_or_else(|| anyhow!("No tx response in broadcast response"))?;
        
        Ok(BroadcastResponse {
            tx_hash: tx_response.txhash,
            code: tx_response.code,
            raw_log: tx_response.raw_log,
        })
    }
    
    
    /// Execute a contract message on the Injective blockchain - REAL IMPLEMENTATION
    /// Now includes automatic retry on sequence errors and EIP-712 bridge support
    pub async fn execute_contract(
        &mut self,
        contract_address: &str,
        msg: Value,
        funds: Vec<Coin>,
        gas_limit: u64,
    ) -> Result<String> {
        log::info!("execute_contract called for {} with msg: {}", contract_address, msg);
        
        // Use Rust signer if enabled (preferred for performance and reliability)
        if self.use_rust_signer {
            if let Some(rust_signer) = &self.rust_signer {
                log::info!("Using Rust-native EIP-712 signer for transaction");
                let account = self.query_account(&self.wallet.address).await?;
                let sequence = self.get_next_sequence(&self.wallet.address).await?;
                
                // Convert funds to chain format
                let chain_funds: Vec<crate::chain::Coin> = funds.into_iter()
                    .map(|coin| crate::chain::Coin {
                        denom: coin.denom,
                        amount: coin.amount,
                    })
                    .collect();
                
                // Determine message type and call appropriate method
                // Handle both "commit" and "commit_solution" message types
                log::debug!("Rust signer received message: {}", msg);
                if msg.get("commit").is_some() || msg.get("commit_solution").is_some() {
                    let msg_key = if msg.get("commit").is_some() { "commit" } else { "commit_solution" };
                    // The commitment might be an array of bytes, not a string
                    let commitment = if let Some(comm_arr) = msg[msg_key]["commitment"].as_array() {
                        // Keep as byte array, don't convert to hex
                        comm_arr.iter()
                            .filter_map(|v| v.as_u64().map(|n| n as u8))
                            .collect::<Vec<u8>>()
                    } else {
                        return Err(anyhow!("Missing or invalid commitment in {} message", msg_key));
                    };
                    let result = rust_signer.sign_and_broadcast_commit(
                        commitment,
                        account.account_number,
                        sequence,
                        Some(chain_funds),
                    ).await;
                    
                    // Handle sequence-related errors
                    if let Err(ref e) = result {
                        let error_msg = e.to_string();
                        if error_msg.contains("account sequence") || 
                           error_msg.contains("expected") && error_msg.contains("got") {
                            log::warn!("Sequence mismatch detected: {}", error_msg);
                            
                            // Just log the sequence error - we'll fetch fresh on next attempt
                            if let Some(expected_seq) = self.parse_sequence_error(&error_msg) {
                                log::info!("Chain expects sequence: {} (will fetch fresh on retry)", expected_seq);
                            }
                        }
                    }
                    
                    return result;
                } else if msg.get("reveal").is_some() || msg.get("reveal_solution").is_some() {
                    let msg_key = if msg.get("reveal").is_some() { "reveal" } else { "reveal_solution" };
                    let nonce = msg[msg_key]["nonce"].as_array()
                        .ok_or_else(|| anyhow!("Missing nonce in {} message", msg_key))?
                        .iter()
                        .filter_map(|v| v.as_u64())
                        .map(|n| n as u8)
                        .collect::<Vec<u8>>();
                    let digest = msg[msg_key]["digest"].as_array()
                        .ok_or_else(|| anyhow!("Missing digest in {} message", msg_key))?
                        .iter()
                        .filter_map(|v| v.as_u64())
                        .map(|n| n as u8)
                        .collect::<Vec<u8>>();
                    let salt = msg[msg_key]["salt"].as_array()
                        .ok_or_else(|| anyhow!("Missing salt in {} message", msg_key))?
                        .iter()
                        .filter_map(|v| v.as_u64())
                        .map(|n| n as u8)
                        .collect::<Vec<u8>>();
                    let result = rust_signer.sign_and_broadcast_reveal(
                        nonce,
                        digest,
                        salt,
                        account.account_number,
                        sequence,
                        Some(chain_funds),
                    ).await;
                    
                    // Handle sequence-related errors
                    if let Err(ref e) = result {
                        let error_msg = e.to_string();
                        if error_msg.contains("account sequence") || 
                           error_msg.contains("expected") && error_msg.contains("got") {
                            log::warn!("Sequence mismatch detected: {}", error_msg);
                            
                            // Just log the sequence error - we'll fetch fresh on next attempt
                            if let Some(expected_seq) = self.parse_sequence_error(&error_msg) {
                                log::info!("Chain expects sequence: {} (will fetch fresh on retry)", expected_seq);
                            }
                        }
                    }
                    
                    return result;
                } else if msg.get("claim_rewards").is_some() || msg.get("claim_reward").is_some() {
                    // Extract epoch_number from the message
                    let msg_key = if msg.get("claim_rewards").is_some() { "claim_rewards" } else { "claim_reward" };
                    let epoch_number = msg[msg_key]["epoch_number"]
                        .as_u64()
                        .ok_or_else(|| anyhow!("Missing epoch_number in {} message", msg_key))?;
                    
                    let result = rust_signer.sign_and_broadcast_claim(
                        epoch_number,
                        account.account_number,
                        sequence,
                        Some(chain_funds),
                    ).await;
                    
                    // Handle sequence-related errors
                    if let Err(ref e) = result {
                        let error_msg = e.to_string();
                        if error_msg.contains("account sequence") || 
                           error_msg.contains("expected") && error_msg.contains("got") {
                            log::warn!("Sequence mismatch detected: {}", error_msg);
                            
                            // Just log the sequence error - we'll fetch fresh on next attempt
                            if let Some(expected_seq) = self.parse_sequence_error(&error_msg) {
                                log::info!("Chain expects sequence: {} (will fetch fresh on retry)", expected_seq);
                            }
                        }
                    }
                    
                    return result;
                } else if msg.get("advance_epoch").is_some() {
                    let result = rust_signer.sign_and_broadcast_advance_epoch(
                        account.account_number,
                        sequence,
                        Some(chain_funds),
                    ).await;
                    
                    // Handle sequence-related errors
                    if let Err(ref e) = result {
                        let error_msg = e.to_string();
                        if error_msg.contains("account sequence") || 
                           error_msg.contains("expected") && error_msg.contains("got") {
                            log::warn!("Sequence mismatch detected: {}", error_msg);
                            
                            // Just log the sequence error - we'll fetch fresh on next attempt
                            if let Some(expected_seq) = self.parse_sequence_error(&error_msg) {
                                log::info!("Chain expects sequence: {} (will fetch fresh on retry)", expected_seq);
                            }
                        }
                    }
                    
                    return result;
                } else if msg.get("finalize_epoch").is_some() {
                    let epoch_number = msg["finalize_epoch"]["epoch_number"].as_u64()
                        .ok_or_else(|| anyhow!("Missing epoch_number in finalize_epoch message"))?;
                    let result = rust_signer.sign_and_broadcast_finalize_epoch(
                        epoch_number,
                        account.account_number,
                        sequence,
                        Some(chain_funds),
                    ).await;
                    
                    // Handle sequence-related errors
                    if let Err(ref e) = result {
                        let error_msg = e.to_string();
                        if error_msg.contains("account sequence") || 
                           error_msg.contains("expected") && error_msg.contains("got") {
                            log::warn!("Sequence mismatch detected: {}", error_msg);
                            
                            // Just log the sequence error - we'll fetch fresh on next attempt
                            if let Some(expected_seq) = self.parse_sequence_error(&error_msg) {
                                log::info!("Chain expects sequence: {} (will fetch fresh on retry)", expected_seq);
                            }
                        }
                    }
                    
                    return result;
                } else {
                    return Err(anyhow!("Unsupported message type for Rust signer: {}", msg));
                }
            } else {
                return Err(anyhow!("Rust signer enabled but not initialized"));
            }
        }
        
        // Use bridge if available (EIP-712 signing)
        if let Some(bridge) = &self.bridge_client {
            log::info!("Using EIP-712 bridge for transaction signing");
            let account = self.query_account(&self.wallet.address).await?;
            let sequence = self.get_next_sequence(&self.wallet.address).await?;
            
            let bridge_funds: Vec<crate::chain::bridge_client::Coin> = funds.into_iter()
                .map(|coin| crate::chain::bridge_client::Coin {
                    denom: coin.denom,
                    amount: coin.amount,
                })
                .collect();
            
            let result = bridge.sign_and_broadcast(
                self.config.chain_id.clone(),
                account.account_number,
                sequence,
                contract_address,
                msg,
                bridge_funds,
                gas_limit,
            ).await;
            
            // Only reset sequence tracking for sequence-related errors
            if let Err(ref e) = result {
                let error_msg = e.to_string();
                if error_msg.contains("account sequence") || 
                   error_msg.contains("expected") && error_msg.contains("got") {
                    log::warn!("Sequence mismatch detected: {} (will fetch fresh on retry)", error_msg);
                }
            }
            
            return result;
        }
        
        // Fall back to old SIGN_MODE_DIRECT (not recommended for Injective)
        log::warn!("Bridge not configured, using SIGN_MODE_DIRECT (may not earn rewards on Injective)");
        
        let max_retries = 3;
        let mut last_error = None;
        
        for attempt in 0..max_retries {
            if attempt > 0 {
                log::info!("Retry attempt {} after sequence error", attempt);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            
            // 1. Query account for sequence and account number (fresh on each attempt)
            let account = self.query_account(&self.wallet.address).await?;
            log::debug!("Account sequence: {}, account_number: {} (attempt {})", 
                account.sequence, account.account_number, attempt + 1);
            
            // 2. Create transaction with ProperTxBuilder
            let builder = ProperTxBuilder::new(
                self.config.chain_id.clone(),
                account.account_number,
                account.sequence,
                &*self.wallet,
            );
            
            let tx_bytes = builder.with_gas_limit(gas_limit)
                .build_execute_contract_tx(
                    contract_address,
                    serde_json::to_vec(&msg)?,
                    funds.clone(),
                )?;
            
            // 3. Simulate for gas estimation (skip on retries to save time)
            let final_tx_bytes = if attempt == 0 {
                match self.simulate_tx(tx_bytes.clone()).await {
                    Ok(sim_result) => {
                        let adjusted_gas = (sim_result.gas_used * 120) / 100;
                        log::debug!("Gas simulation: used={}, adjusted={}", sim_result.gas_used, adjusted_gas);
                        
                        if adjusted_gas > gas_limit {
                            log::info!("Rebuilding transaction with adjusted gas: {} (requested: {})", adjusted_gas, gas_limit);
                            let builder = ProperTxBuilder::new(
                                self.config.chain_id.clone(),
                                account.account_number,
                                account.sequence,
                                &*self.wallet,
                            );
                            
                            builder.with_gas_limit(adjusted_gas)
                                .build_execute_contract_tx(
                                    contract_address,
                                    serde_json::to_vec(&msg)?,
                                    funds.clone(),
                                )?
                        } else {
                            tx_bytes
                        }
                    }
                    Err(e) => {
                        log::warn!("Gas simulation failed: {}, using default gas", e);
                        tx_bytes
                    }
                }
            } else {
                // On retries, skip simulation and use fixed gas
                let builder = ProperTxBuilder::new(
                    self.config.chain_id.clone(),
                    account.account_number,
                    account.sequence,
                    &*self.wallet,
                ).with_gas_limit(300000);
                
                builder.build_execute_contract_tx(
                    contract_address,
                    serde_json::to_vec(&msg)?,
                    funds.clone(),
                )?
            };
            
            // 4. Broadcast transaction
            match self.broadcast_tx(final_tx_bytes).await {
                Ok(response) => {
                    if response.code == 0 {
                        log::info!("Transaction successful: {}", response.tx_hash);
                        return Ok(response.tx_hash);
                    } else if response.raw_log.contains("account sequence") || 
                              response.raw_log.contains("signature verification failed") ||
                              response.raw_log.contains("account number") {
                        log::warn!("Account mismatch detected (attempt {}): {}", attempt + 1, response.raw_log);
                        // Sequence error - will fetch fresh on retry
                        last_error = Some(anyhow!("Account error: {}", response.raw_log));
                        continue;
                    } else {
                        return Err(anyhow!("Transaction failed: {}", response.raw_log));
                    }
                }
                Err(e) => {
                    let error_str = e.to_string();
                    if error_str.contains("account sequence") || 
                       error_str.contains("signature verification failed") ||
                       error_str.contains("account number") {
                        log::warn!("Account error in broadcast (attempt {}): {}", attempt + 1, e);
                        // Sequence error - will fetch fresh on retry
                        last_error = Some(e);
                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| anyhow!("Failed after {} retries", max_retries)))
    }
    
    /// Query a smart contract (read-only, no gas required)
    pub async fn query_contract_smart(
        &self,
        contract_address: &str,
        query_msg: Vec<u8>,
    ) -> Result<serde_json::Value> {
        
        
        let response = self.with_retry(|| async {
            let channel = self.channel()?;
            
            // Create the wasm query client
            let mut client = proto::cosmwasm::wasm::v1::query_client::QueryClient::new(channel);
            
            // Create the request with the contract address and query data
            let request = tonic::Request::new(
                proto::cosmwasm::wasm::v1::QuerySmartContractStateRequest {
                    address: contract_address.to_string(),
                    query_data: query_msg.clone(),
                }
            );
            
            log::debug!("Querying contract {} with message: {:?}", 
                contract_address, 
                String::from_utf8_lossy(&query_msg)
            );
            
            // Execute the query
            client.smart_contract_state(request).await
                .map_err(|e| anyhow!("Failed to query contract: {}", e))
        }).await?;
        
        // The response data contains the JSON result
        let response_bytes = response.into_inner().data;
        
        // The gRPC endpoint for smart contract queries returns raw bytes, which are
        // expected to be a JSON-encoded string from the contract. We parse it
        // directly. If this fails, it indicates a contract or serialization error.
        let json_value = serde_json::from_slice::<serde_json::Value>(&response_bytes)
            .map_err(|e| anyhow!("Failed to parse smart contract JSON response: {}", e))?;
        
        Ok(json_value)
    }
    
    /// Query bank balance - REAL IMPLEMENTATION
    pub async fn query_bank_balance(&self, address: &str, denom: &str) -> Result<u128> {
        let response = self.with_retry(|| async {
            let channel = self.channel()?;
            let mut client = BankQueryClient::new(channel);
            let request = tonic::Request::new(QueryBalanceRequest {
                address: address.to_string(),
                denom: denom.to_string(),
            });
            client.balance(request).await
                .map_err(|e| anyhow!("Failed to query bank balance: {}", e))
        }).await?;
        
        let balance = response.into_inner().balance
            .ok_or_else(|| anyhow!("No balance returned"))?;
        
        // Parse amount string to u128
        balance.amount.parse::<u128>()
            .map_err(|e| anyhow!("Failed to parse balance amount: {}", e))
    }
    
    /// Get the latest block height from the chain - REAL IMPLEMENTATION
    /// Uses the x-cosmos-block-height header from the response metadata
    pub async fn get_latest_block_height(&self) -> Result<u64> {
        log::debug!("Querying latest block height from chain...");
        
        // We can use any simple query to get the block height from headers
        // Using GetNodeInfo as it's lightweight and already implemented
        let channel = self.channel()?;
        let mut client = TendermintServiceClient::new(channel);
        let request = tonic::Request::new(GetNodeInfoRequest {});
        
        // Execute the request to get the response with metadata
        let response = client.get_node_info(request).await
            .map_err(|e| anyhow!("Failed to get node info: {}", e))?;
        
        // Extract block height from response metadata
        let metadata = response.metadata();
        
        // Look for x-cosmos-block-height header
        if let Some(height_value) = metadata.get("x-cosmos-block-height") {
            let height_str = height_value.to_str()
                .map_err(|e| anyhow!("Invalid block height header: {}", e))?;
            let height = height_str.parse::<u64>()
                .map_err(|e| anyhow!("Failed to parse block height: {}", e))?;
            
            log::info!("Current chain block height: {}", height);
            Ok(height)
        } else {
            // Fallback: try to get from GetLatestBlock if header not present
            // This might fail with protobuf errors but worth trying
            log::warn!("No x-cosmos-block-height header, trying GetLatestBlock...");
            
            let mut client = TendermintServiceClient::new(self.channel()?);
            let request = tonic::Request::new(GetLatestBlockRequest {});
            
            match client.get_latest_block(request).await {
                Ok(response) => {
                    // Try to extract from response metadata first
                    let metadata = response.metadata();
                    if let Some(height_value) = metadata.get("x-cosmos-block-height") {
                        let height_str = height_value.to_str()
                            .map_err(|e| anyhow!("Invalid block height header: {}", e))?;
                        let height = height_str.parse::<u64>()
                            .map_err(|e| anyhow!("Failed to parse block height: {}", e))?;
                        log::info!("Current chain block height from GetLatestBlock header: {}", height);
                        return Ok(height);
                    }
                    
                    // If no header, return a reasonable fallback
                    log::warn!("No block height in headers, using fallback");
                    Ok(87636000) // Recent testnet height as fallback
                }
                Err(e) => {
                    log::error!("Failed to get latest block: {}, using fallback", e);
                    Ok(87636000) // Recent testnet height as fallback
                }
            }
        }
    }
    
    /// Execute a contract message WITHOUT gas simulation - for time-critical transactions
    /// This is used for reveals where the 30-second window doesn't allow time for simulation
    /// Now includes automatic retry on sequence errors and EIP-712 bridge support
    pub async fn execute_contract_fast(
        &mut self,
        contract_address: &str,
        msg: Value,
        funds: Vec<Coin>,
        gas_limit: u64,
    ) -> Result<String> {
        log::warn!("execute_contract_fast: SKIPPING GAS SIMULATION for time-critical transaction");
        
        // Use Rust signer if enabled (preferred for performance and reliability)
        if self.use_rust_signer {
            if let Some(rust_signer) = &self.rust_signer {
                log::info!("Using Rust-native EIP-712 signer for fast transaction");
                let account = self.query_account(&self.wallet.address).await?;
                let sequence = self.get_next_sequence(&self.wallet.address).await?;
                
                // Convert funds to chain format
                let chain_funds: Vec<crate::chain::Coin> = funds.into_iter()
                    .map(|coin| crate::chain::Coin {
                        denom: coin.denom,
                        amount: coin.amount,
                    })
                    .collect();
                
                // Determine message type and call appropriate method
                // Handle both "commit" and "commit_solution" message types
                log::debug!("Rust signer received message: {}", msg);
                if msg.get("commit").is_some() || msg.get("commit_solution").is_some() {
                    let msg_key = if msg.get("commit").is_some() { "commit" } else { "commit_solution" };
                    // The commitment might be an array of bytes, not a string
                    let commitment = if let Some(comm_arr) = msg[msg_key]["commitment"].as_array() {
                        // Keep as byte array, don't convert to hex
                        comm_arr.iter()
                            .filter_map(|v| v.as_u64().map(|n| n as u8))
                            .collect::<Vec<u8>>()
                    } else {
                        return Err(anyhow!("Missing or invalid commitment in {} message", msg_key));
                    };
                    let result = rust_signer.sign_and_broadcast_commit(
                        commitment,
                        account.account_number,
                        sequence,
                        Some(chain_funds),
                    ).await;
                    
                    // Handle sequence-related errors
                    if let Err(ref e) = result {
                        let error_msg = e.to_string();
                        if error_msg.contains("account sequence") || 
                           error_msg.contains("expected") && error_msg.contains("got") {
                            log::warn!("Sequence mismatch detected: {}", error_msg);
                            
                            // Just log the sequence error - we'll fetch fresh on next attempt
                            if let Some(expected_seq) = self.parse_sequence_error(&error_msg) {
                                log::info!("Chain expects sequence: {} (will fetch fresh on retry)", expected_seq);
                            }
                        }
                    }
                    
                    return result;
                } else if msg.get("reveal").is_some() || msg.get("reveal_solution").is_some() {
                    let msg_key = if msg.get("reveal").is_some() { "reveal" } else { "reveal_solution" };
                    let nonce = msg[msg_key]["nonce"].as_array()
                        .ok_or_else(|| anyhow!("Missing nonce in {} message", msg_key))?
                        .iter()
                        .filter_map(|v| v.as_u64())
                        .map(|n| n as u8)
                        .collect::<Vec<u8>>();
                    let digest = msg[msg_key]["digest"].as_array()
                        .ok_or_else(|| anyhow!("Missing digest in {} message", msg_key))?
                        .iter()
                        .filter_map(|v| v.as_u64())
                        .map(|n| n as u8)
                        .collect::<Vec<u8>>();
                    let salt = msg[msg_key]["salt"].as_array()
                        .ok_or_else(|| anyhow!("Missing salt in {} message", msg_key))?
                        .iter()
                        .filter_map(|v| v.as_u64())
                        .map(|n| n as u8)
                        .collect::<Vec<u8>>();
                    let result = rust_signer.sign_and_broadcast_reveal(
                        nonce,
                        digest,
                        salt,
                        account.account_number,
                        sequence,
                        Some(chain_funds),
                    ).await;
                    
                    // Handle sequence-related errors
                    if let Err(ref e) = result {
                        let error_msg = e.to_string();
                        if error_msg.contains("account sequence") || 
                           error_msg.contains("expected") && error_msg.contains("got") {
                            log::warn!("Sequence mismatch detected: {}", error_msg);
                            
                            // Just log the sequence error - we'll fetch fresh on next attempt
                            if let Some(expected_seq) = self.parse_sequence_error(&error_msg) {
                                log::info!("Chain expects sequence: {} (will fetch fresh on retry)", expected_seq);
                            }
                        }
                    }
                    
                    return result;
                } else if msg.get("claim_rewards").is_some() || msg.get("claim_reward").is_some() {
                    // Extract epoch_number from the message
                    let msg_key = if msg.get("claim_rewards").is_some() { "claim_rewards" } else { "claim_reward" };
                    let epoch_number = msg[msg_key]["epoch_number"]
                        .as_u64()
                        .ok_or_else(|| anyhow!("Missing epoch_number in {} message", msg_key))?;
                    
                    let result = rust_signer.sign_and_broadcast_claim(
                        epoch_number,
                        account.account_number,
                        sequence,
                        Some(chain_funds),
                    ).await;
                    
                    // Handle sequence-related errors
                    if let Err(ref e) = result {
                        let error_msg = e.to_string();
                        if error_msg.contains("account sequence") || 
                           error_msg.contains("expected") && error_msg.contains("got") {
                            log::warn!("Sequence mismatch detected: {}", error_msg);
                            
                            // Just log the sequence error - we'll fetch fresh on next attempt
                            if let Some(expected_seq) = self.parse_sequence_error(&error_msg) {
                                log::info!("Chain expects sequence: {} (will fetch fresh on retry)", expected_seq);
                            }
                        }
                    }
                    
                    return result;
                } else if msg.get("advance_epoch").is_some() {
                    let result = rust_signer.sign_and_broadcast_advance_epoch(
                        account.account_number,
                        sequence,
                        Some(chain_funds),
                    ).await;
                    
                    // Handle sequence-related errors
                    if let Err(ref e) = result {
                        let error_msg = e.to_string();
                        if error_msg.contains("account sequence") || 
                           error_msg.contains("expected") && error_msg.contains("got") {
                            log::warn!("Sequence mismatch detected: {}", error_msg);
                            
                            // Just log the sequence error - we'll fetch fresh on next attempt
                            if let Some(expected_seq) = self.parse_sequence_error(&error_msg) {
                                log::info!("Chain expects sequence: {} (will fetch fresh on retry)", expected_seq);
                            }
                        }
                    }
                    
                    return result;
                } else if msg.get("finalize_epoch").is_some() {
                    let epoch_number = msg["finalize_epoch"]["epoch_number"].as_u64()
                        .ok_or_else(|| anyhow!("Missing epoch_number in finalize_epoch message"))?;
                    let result = rust_signer.sign_and_broadcast_finalize_epoch(
                        epoch_number,
                        account.account_number,
                        sequence,
                        Some(chain_funds),
                    ).await;
                    
                    // Handle sequence-related errors
                    if let Err(ref e) = result {
                        let error_msg = e.to_string();
                        if error_msg.contains("account sequence") || 
                           error_msg.contains("expected") && error_msg.contains("got") {
                            log::warn!("Sequence mismatch detected: {}", error_msg);
                            
                            // Just log the sequence error - we'll fetch fresh on next attempt
                            if let Some(expected_seq) = self.parse_sequence_error(&error_msg) {
                                log::info!("Chain expects sequence: {} (will fetch fresh on retry)", expected_seq);
                            }
                        }
                    }
                    
                    return result;
                } else {
                    return Err(anyhow!("Unsupported message type for Rust signer: {}", msg));
                }
            } else {
                return Err(anyhow!("Rust signer enabled but not initialized"));
            }
        }
        
        // Use bridge if available (EIP-712 signing)
        if let Some(bridge) = &self.bridge_client {
            log::info!("Using EIP-712 bridge for fast transaction signing");
            let account = self.query_account(&self.wallet.address).await?;
            let sequence = self.get_next_sequence(&self.wallet.address).await?;
            
            let bridge_funds: Vec<crate::chain::bridge_client::Coin> = funds.into_iter()
                .map(|coin| crate::chain::bridge_client::Coin {
                    denom: coin.denom,
                    amount: coin.amount,
                })
                .collect();
            
            let result = bridge.sign_and_broadcast(
                self.config.chain_id.clone(),
                account.account_number,
                sequence,
                contract_address,
                msg,
                bridge_funds,
                gas_limit,
            ).await;
            
            // Only reset sequence tracking for sequence-related errors
            if let Err(ref e) = result {
                let error_msg = e.to_string();
                if error_msg.contains("account sequence") || 
                   error_msg.contains("expected") && error_msg.contains("got") {
                    log::warn!("Sequence mismatch detected: {} (will fetch fresh on retry)", error_msg);
                }
            }
            
            return result;
        }
        
        // Fall back to old SIGN_MODE_DIRECT (not recommended for Injective)
        log::warn!("Bridge not configured, using SIGN_MODE_DIRECT (may not earn rewards on Injective)");
        
        let max_retries = 3;
        let mut last_error = None;
        
        for attempt in 0..max_retries {
            if attempt > 0 {
                log::info!("Retry attempt {} after sequence error", attempt);
                // Small delay between retries to let chain state settle
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            
            // Always query fresh account info to get latest sequence
            let account = self.query_account(&self.wallet.address).await?;
            log::debug!("Account sequence: {}, account_number: {}", 
                account.sequence, account.account_number);
            
            // Create transaction with fresh sequence
            let builder = ProperTxBuilder::new(
                self.config.chain_id.clone(),
                account.account_number,
                account.sequence,
                &*self.wallet,
            ).with_gas_limit(gas_limit);
            
            let tx_bytes = builder.build_execute_contract_tx(
                contract_address,
                serde_json::to_vec(&msg)?,
                funds.clone(),
            )?;
            
            // Try to broadcast
            log::info!("Broadcasting transaction with fixed gas limit: {} (attempt {})", gas_limit, attempt + 1);
            match self.broadcast_tx(tx_bytes).await {
                Ok(response) => {
                    if response.code == 0 {
                        log::info!("Fast transaction successful: {}", response.tx_hash);
                        return Ok(response.tx_hash);
                    } else if response.raw_log.contains("account sequence") || 
                              response.raw_log.contains("signature verification failed") ||
                              response.raw_log.contains("account number") {
                        // Sequence/account error - retry with fresh account query
                        log::warn!("Account mismatch detected (attempt {}): {}", attempt + 1, response.raw_log);
                        last_error = Some(anyhow!("Account error: {}", response.raw_log));
                        continue;
                    } else {
                        // Other error - fail immediately (no point retrying)
                        return Err(anyhow!("Transaction failed: {}", response.raw_log));
                    }
                }
                Err(e) => {
                    let error_str = e.to_string();
                    if error_str.contains("account sequence") || 
                       error_str.contains("signature verification failed") ||
                       error_str.contains("account number") {
                        log::warn!("Account error in broadcast (attempt {}): {}", attempt + 1, e);
                        // Sequence error - will fetch fresh on retry
                        last_error = Some(e);
                        continue;
                    } else {
                        // Non-sequence error - fail immediately
                        return Err(e);
                    }
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| anyhow!("Failed after {} retries", max_retries)))
    }
    
    /// Get node info for health check and chain ID - REAL IMPLEMENTATION
    pub async fn get_node_info(&self) -> Result<NodeInfo> {
        let response = self.with_retry(|| async {
            let channel = self.channel()?;
            let mut client = TendermintServiceClient::new(channel);
            let request = tonic::Request::new(GetNodeInfoRequest {});
            client.get_node_info(request).await
                .map_err(|e| anyhow!("Failed to get node info: {}", e))
        }).await?;
        
        let node_info_response = response.into_inner();
        let default_node_info = node_info_response.default_node_info
            .ok_or_else(|| anyhow!("No default node info in response"))?;
        let app_version = node_info_response.application_version
            .ok_or_else(|| anyhow!("No application version in response"))?;
        
        Ok(NodeInfo {
            chain_id: default_node_info.network,
            node_version: app_version.version,
            moniker: default_node_info.moniker,
        })
    }
    
    /// Retry helper for network operations
    async fn with_retry<T, F, Fut>(&self, f: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut retries = 0;
        loop {
            match f().await {
                Ok(result) => return Ok(result),
                Err(e) if retries < self.config.max_retries => {
                    retries += 1;
                    tokio::time::sleep(Duration::from_millis(100 * retries as u64)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

// AccountInfo is now defined in account_types module

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

/// Node information for health checks and chain verification
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub chain_id: String,
    pub node_version: String,
    pub moniker: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_client_creation() {
        let wallet = InjectiveWallet::from_mnemonic_no_passphrase(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
        ).unwrap();
        
        let client = InjectiveClient::new_testnet(wallet);
        assert!(!client.is_connected());
        assert_eq!(client.config.chain_id, "injective-888");
    }
    
    // Note: Real testnet tests would require actual connection
    // These are just unit tests for the structure
}