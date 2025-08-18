/// Transaction Manager - Handles all chain interactions with retry logic
/// Implements Gemini Pro's recommendations for robust transaction handling

use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

use crate::chain::InjectiveClient;
// Messages are created inline as JSON

/// Transaction status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// Transaction is queued
    Pending,
    /// Transaction is being processed
    Processing,
    /// Transaction succeeded
    Success { tx_hash: String },
    /// Transaction failed after all retries
    Failed { error: String },
}

/// Transaction types supported by the manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionType {
    /// Commit solution transaction
    Commit { 
        epoch: u64,
        commitment: [u8; 32],
    },
    /// Reveal solution transaction
    Reveal {
        epoch: u64,
        nonce: [u8; 8],
        digest: [u8; 16],
        salt: [u8; 32],
    },
    /// Claim reward transaction
    Claim {
        epoch: u64,
    },
    /// Finalize epoch (permissionless) - for historical epochs
    FinalizeEpoch {
        epoch: u64,
    },
    /// Advance epoch (permissionless) - moves current to next epoch
    AdvanceEpoch,
}

/// A transaction in the queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedTransaction {
    /// Unique transaction ID
    pub id: u64,
    /// Transaction type and data
    pub tx_type: TransactionType,
    /// Current status
    pub status: TransactionStatus,
    /// Number of retry attempts made
    pub retry_count: u32,
    /// Timestamp when queued
    pub queued_at: u64,
    /// Estimated gas required
    pub gas_estimate: Option<u64>,
}

/// Configuration for transaction manager
#[derive(Debug, Clone)]
pub struct TransactionManagerConfig {
    /// Maximum retries per transaction
    pub max_retries: u32,
    /// Initial retry delay (milliseconds)
    pub initial_retry_delay_ms: u64,
    /// Maximum retry delay (milliseconds)
    pub max_retry_delay_ms: u64,
    /// Gas price multiplier for retries (1.1 = 10% increase)
    pub gas_price_multiplier: f64,
    /// Maximum transactions in queue
    pub max_queue_size: usize,
    /// Contract address
    pub contract_address: String,
}

impl Default for TransactionManagerConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_retry_delay_ms: 1000,
            max_retry_delay_ms: 30000,
            gas_price_multiplier: 1.1,
            max_queue_size: 100,
            contract_address: String::new(),
        }
    }
}

/// Manages transaction queue and submission with retry logic
pub struct TransactionManager {
    /// Configuration
    config: TransactionManagerConfig,
    /// Chain client
    client: Arc<RwLock<InjectiveClient>>,
    /// Transaction queue
    queue: Arc<RwLock<VecDeque<QueuedTransaction>>>,
    /// Next transaction ID
    next_id: Arc<RwLock<u64>>,
    /// Whether manager is running
    is_running: Arc<RwLock<bool>>,
    /// Completed transactions (for status tracking)
    completed: Arc<RwLock<std::collections::HashMap<u64, TransactionStatus>>>,
}

impl TransactionManager {
    /// Create a new transaction manager
    pub fn new(config: TransactionManagerConfig, client: Arc<RwLock<InjectiveClient>>) -> Self {
        Self {
            config,
            client,
            queue: Arc::new(RwLock::new(VecDeque::new())),
            next_id: Arc::new(RwLock::new(1)),
            is_running: Arc::new(RwLock::new(false)),
            completed: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
    
    /// Start processing transactions in the background
    pub async fn start(&self) -> Result<()> {
        let mut running = self.is_running.write().await;
        if *running {
            return Err(anyhow!("Transaction manager already running"));
        }
        *running = true;
        drop(running);
        
        // Clone for background task
        let client = self.client.clone();
        let queue = self.queue.clone();
        let completed = self.completed.clone();
        let is_running = self.is_running.clone();
        let config = self.config.clone();
        
        // Spawn background processing task
        tokio::spawn(async move {
            log::info!("Transaction manager started");
            
            while *is_running.read().await {
                // Process next transaction in queue
                let mut queue_guard = queue.write().await;
                if let Some(mut tx) = queue_guard.pop_front() {
                    drop(queue_guard); // Release lock while processing
                    
                    // Process transaction
                    tx.status = TransactionStatus::Processing;
                    log::info!("Processing transaction {}: {:?}", tx.id, tx.tx_type);
                    
                    match Self::process_transaction(&client, &config, &mut tx).await {
                        Ok(tx_hash) => {
                            let status = TransactionStatus::Success { tx_hash: tx_hash.clone() };
                            tx.status = status.clone();
                            log::info!("Transaction {} succeeded: {}", tx.id, tx_hash);
                            
                            // Store in completed map for status tracking
                            let mut completed_guard = completed.write().await;
                            completed_guard.insert(tx.id, status);
                        }
                        Err(e) => {
                            // Enhanced error logging to understand failures
                            log::error!("Transaction {} ({:?}) failed: {}", tx.id, tx.tx_type, e);
                            
                            // Log specific error details
                            if e.to_string().contains("Wrong phase") {
                                log::error!("TIMING ERROR: Transaction arrived too late - phase already changed!");
                            } else if e.to_string().contains("gas") {
                                log::error!("GAS ERROR: Insufficient gas or gas estimation failed");
                            } else if e.to_string().contains("sequence") {
                                log::error!("SEQUENCE ERROR: Account sequence mismatch - need to refresh");
                            }
                            
                            // Special handling for time-critical reveal transactions
                            let (max_retries, retry_delay) = match &tx.tx_type {
                                TransactionType::Reveal { .. } => {
                                    // Reveals are time-critical - minimal retries with short delays
                                    // The reveal window is only 15 blocks (~30 seconds)
                                    log::warn!("Reveal transaction failed - using fast retry logic");
                                    (1u32, 500u64) // Only 1 retry with 500ms delay
                                }
                                _ => {
                                    // Use normal retry logic for other transactions
                                    let delay = Self::calculate_retry_delay(
                                        tx.retry_count + 1,
                                        config.initial_retry_delay_ms,
                                        config.max_retry_delay_ms,
                                    );
                                    (config.max_retries, delay)
                                }
                            };
                            
                            // Check if we should retry
                            if tx.retry_count < max_retries {
                                tx.retry_count += 1;
                                tx.status = TransactionStatus::Pending;
                                
                                log::info!(
                                    "Retrying transaction {} (attempt {}/{}) after {}ms",
                                    tx.id,
                                    tx.retry_count,
                                    max_retries,
                                    retry_delay
                                );
                                
                                sleep(Duration::from_millis(retry_delay)).await;
                                
                                // Re-queue for retry
                                let mut queue_guard = queue.write().await;
                                queue_guard.push_back(tx);
                            } else {
                                let status = TransactionStatus::Failed { 
                                    error: format!("Failed after {} retries: {}", max_retries, e) 
                                };
                                tx.status = status.clone();
                                log::error!("Transaction {} failed permanently", tx.id);
                                
                                // Store in completed map for status tracking
                                let mut completed_guard = completed.write().await;
                                completed_guard.insert(tx.id, status);
                            }
                        }
                    }
                } else {
                    // No transactions to process, wait a bit
                    sleep(Duration::from_millis(100)).await;
                }
            }
            
            log::info!("Transaction manager stopped");
        });
        
        Ok(())
    }
    
    /// Stop processing transactions
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.is_running.write().await;
        if !*running {
            return Err(anyhow!("Transaction manager not running"));
        }
        *running = false;
        
        log::info!("Stopping transaction manager");
        Ok(())
    }
    
    /// Queue a commit transaction
    pub async fn queue_commit(&self, epoch: u64, commitment: [u8; 32]) -> Result<u64> {
        let tx_type = TransactionType::Commit { epoch, commitment };
        self.queue_transaction(tx_type).await
    }
    
    /// Queue a reveal transaction
    pub async fn queue_reveal(
        &self,
        epoch: u64,
        nonce: [u8; 8],
        digest: [u8; 16],
        salt: [u8; 32],
    ) -> Result<u64> {
        let tx_type = TransactionType::Reveal { epoch, nonce, digest, salt };
        self.queue_transaction(tx_type).await
    }
    
    /// Queue a claim transaction
    pub async fn queue_claim(&self, epoch: u64) -> Result<u64> {
        let tx_type = TransactionType::Claim { epoch };
        self.queue_transaction(tx_type).await
    }
    
    /// Queue a finalize epoch transaction (for historical epochs)
    pub async fn queue_finalize_epoch(&self, epoch: u64) -> Result<u64> {
        let tx_type = TransactionType::FinalizeEpoch { epoch };
        self.queue_transaction(tx_type).await
    }
    
    /// Queue an advance epoch transaction (to move current epoch to next)
    pub async fn queue_advance_epoch(&self) -> Result<u64> {
        let tx_type = TransactionType::AdvanceEpoch;
        self.queue_transaction(tx_type).await
    }
    
    /// Get transaction status by ID
    pub async fn get_status(&self, id: u64) -> Option<TransactionStatus> {
        // First check completed transactions
        {
            let completed = self.completed.read().await;
            if let Some(status) = completed.get(&id) {
                return Some(status.clone());
            }
        }
        
        // Then check active queue
        let queue = self.queue.read().await;
        queue.iter()
            .find(|tx| tx.id == id)
            .map(|tx| tx.status.clone())
    }
    
    /// Get all queued transactions
    pub async fn get_queue(&self) -> Vec<QueuedTransaction> {
        self.queue.read().await.iter().cloned().collect()
    }
    
    /// Clear all pending transactions
    pub async fn clear_queue(&self) -> Result<()> {
        let mut queue = self.queue.write().await;
        queue.clear();
        log::info!("Transaction queue cleared");
        Ok(())
    }
    
    // Internal methods
    
    /// Queue a transaction
    async fn queue_transaction(&self, tx_type: TransactionType) -> Result<u64> {
        let mut queue = self.queue.write().await;
        
        // Check queue size
        if queue.len() >= self.config.max_queue_size {
            return Err(anyhow!("Transaction queue full"));
        }
        
        // Generate ID
        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;
        drop(next_id);
        
        // Create queued transaction
        let tx = QueuedTransaction {
            id,
            tx_type: tx_type.clone(),
            status: TransactionStatus::Pending,
            retry_count: 0,
            queued_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
            gas_estimate: None,
        };
        
        queue.push_back(tx);
        log::debug!("Queued transaction {}: {:?}", id, tx_type);
        
        Ok(id)
    }
    
    /// Process a single transaction
    async fn process_transaction(
        client: &Arc<RwLock<InjectiveClient>>,
        config: &TransactionManagerConfig,
        tx: &mut QueuedTransaction,
    ) -> Result<String> {
        let mut client = client.write().await;
        
        // Ensure client is connected
        if !client.is_connected() {
            client.connect().await?;
        }
        
        // Build and submit transaction based on type
        let tx_hash = match &tx.tx_type {
            TransactionType::Commit { commitment, .. } => {
                // Create the message wrapped in the correct enum variant
                let msg = serde_json::json!({
                    "commit_solution": {
                        "commitment": commitment.to_vec()
                    }
                });
                
                // Commits are time-critical (15 block window) - skip gas simulation!
                log::warn!("Commit transaction - CRITICAL TIME WINDOW - SKIPPING GAS SIMULATION");
                
                let start = std::time::Instant::now();
                let result = client.execute_contract_fast(
                    &config.contract_address,
                    msg,
                    vec![],
                    250_000,  // Fixed gas limit for commits
                ).await?;
                
                let elapsed = start.elapsed();
                log::info!("Commit transaction submitted in {:?} (no gas simulation)", elapsed);
                result
            }
            
            TransactionType::Reveal { nonce, digest, salt, .. } => {
                // Create the message wrapped in the correct enum variant
                // The contract expects nonce as [u8; 8], not u64
                let msg = serde_json::json!({
                    "reveal_solution": {
                        "nonce": nonce.to_vec(),  // Send as byte array
                        "digest": digest.to_vec(),
                        "salt": salt.to_vec()
                    }
                });
                
                // For reveals, we need to be FAST - skip gas simulation!
                // Use execute_contract_fast with fixed 300k gas
                log::warn!("Reveal transaction - CRITICAL TIME WINDOW - SKIPPING GAS SIMULATION");
                
                let start = std::time::Instant::now();
                let result = client.execute_contract_fast(
                    &config.contract_address,
                    msg,
                    vec![],
                    300_000,  // Fixed gas limit for reveals
                ).await?;
                
                let elapsed = start.elapsed();
                log::info!("Reveal transaction submitted in {:?} (no gas simulation)", elapsed);
                result
            }
            
            TransactionType::Claim { epoch } => {
                // Create the message wrapped in the correct enum variant
                let msg = serde_json::json!({
                    "claim_reward": {
                        "epoch_number": *epoch
                    }
                });
                
                // Claims need more gas due to token minting
                client.execute_contract_fast(
                    &config.contract_address,
                    msg,
                    vec![],
                    400_000,  // Increased gas limit for claims
                ).await?
            }
            
            TransactionType::FinalizeEpoch { epoch } => {
                // Create the message wrapped in the correct enum variant
                let msg = serde_json::json!({
                    "finalize_epoch": {
                        "epoch_number": *epoch
                    }
                });
                
                // Use fast path for all transactions - skip gas simulation!
                client.execute_contract_fast(
                    &config.contract_address,
                    msg,
                    vec![],
                    250_000,  // Fixed gas limit
                ).await?
            }
            
            TransactionType::AdvanceEpoch => {
                // Create the message wrapped in the correct enum variant
                let msg = serde_json::json!({
                    "advance_epoch": {}
                });
                
                // Use fast path for all transactions - skip gas simulation!
                client.execute_contract_fast(
                    &config.contract_address,
                    msg,
                    vec![],
                    250_000,  // Fixed gas limit
                ).await?
            }
        };
        
        Ok(tx_hash)
    }
    
    /// Calculate retry delay with exponential backoff
    fn calculate_retry_delay(retry_count: u32, initial_ms: u64, max_ms: u64) -> u64 {
        let delay = initial_ms * 2u64.pow(retry_count - 1);
        delay.min(max_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_retry_delay_calculation() {
        // Test exponential backoff
        assert_eq!(TransactionManager::calculate_retry_delay(1, 1000, 30000), 1000);
        assert_eq!(TransactionManager::calculate_retry_delay(2, 1000, 30000), 2000);
        assert_eq!(TransactionManager::calculate_retry_delay(3, 1000, 30000), 4000);
        assert_eq!(TransactionManager::calculate_retry_delay(4, 1000, 30000), 8000);
        assert_eq!(TransactionManager::calculate_retry_delay(5, 1000, 30000), 16000);
        assert_eq!(TransactionManager::calculate_retry_delay(6, 1000, 30000), 30000); // Capped at max
    }
    
    #[tokio::test]
    async fn test_transaction_queue() {
        let client = Arc::new(RwLock::new(
            InjectiveClient::new_testnet(
                crate::chain::wallet::InjectiveWallet::from_mnemonic_no_passphrase(
                    "test test test test test test test test test test test junk"
                ).unwrap()
            )
        ));
        
        let config = TransactionManagerConfig::default();
        let manager = TransactionManager::new(config, client);
        
        // Queue transactions
        let id1 = manager.queue_commit(1, [0; 32]).await.unwrap();
        let id2 = manager.queue_reveal(1, [1; 8], [2; 16], [3; 32]).await.unwrap();
        let id3 = manager.queue_claim(1).await.unwrap();
        
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
        
        // Check queue
        let queue = manager.get_queue().await;
        assert_eq!(queue.len(), 3);
        
        // Clear queue
        manager.clear_queue().await.unwrap();
        let queue = manager.get_queue().await;
        assert_eq!(queue.len(), 0);
    }
}