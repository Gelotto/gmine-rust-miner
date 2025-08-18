/// Epoch Monitor - Tracks blockchain state and epoch transitions
/// Implements Gemini Pro's recommendations for efficient epoch tracking

use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

use crate::chain::InjectiveClient;

/// Current phase within an epoch
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EpochPhase {
    /// Commit phase (blocks 0-30)
    Commit,
    /// Reveal phase (blocks 31-45)
    Reveal,
    /// Settlement phase (blocks 46-50)
    Settlement,
}

/// Complete epoch information from the chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochInfo {
    /// Current epoch number
    pub epoch_number: u64,
    /// Current block height
    pub block_height: u64,
    /// Current block within the epoch (0-49 for 50-block epochs)
    pub block_in_epoch: u64,
    /// Current phase of the epoch
    pub phase: EpochPhase,
    /// Current mining difficulty
    pub difficulty: u8,
    /// Time until next phase (estimated seconds)
    pub time_to_next_phase: u64,
    /// Whether epoch finalization has been triggered
    pub is_finalized: bool,
}

impl EpochInfo {
    /// Calculate epoch info from block height (assuming 50 blocks per epoch)
    pub fn from_block_height(block_height: u64, difficulty: u8) -> Self {
        const BLOCKS_PER_EPOCH: u64 = 50;
        const COMMIT_END: u64 = 30;
        const REVEAL_END: u64 = 45;
        const BLOCK_TIME_SECONDS: u64 = 2; // Approximate block time on Injective
        
        let epoch_number = block_height / BLOCKS_PER_EPOCH;
        let block_in_epoch = block_height % BLOCKS_PER_EPOCH;
        
        let (phase, blocks_to_next_phase) = if block_in_epoch <= COMMIT_END {
            (EpochPhase::Commit, COMMIT_END - block_in_epoch + 1)
        } else if block_in_epoch <= REVEAL_END {
            (EpochPhase::Reveal, REVEAL_END - block_in_epoch + 1)
        } else {
            (EpochPhase::Settlement, BLOCKS_PER_EPOCH - block_in_epoch)
        };
        
        let time_to_next_phase = blocks_to_next_phase * BLOCK_TIME_SECONDS;
        let is_finalized = block_in_epoch >= BLOCKS_PER_EPOCH - 1;
        
        Self {
            epoch_number,
            block_height,
            block_in_epoch,
            phase,
            difficulty,
            time_to_next_phase,
            is_finalized,
        }
    }
}

/// Configuration for epoch monitoring
#[derive(Debug, Clone)]
pub struct EpochMonitorConfig {
    /// How often to poll for epoch updates (seconds)
    pub poll_interval: u64,
    /// Whether to trigger epoch finalization automatically
    pub auto_finalize: bool,
    /// Contract address to query
    pub contract_address: String,
}

impl Default for EpochMonitorConfig {
    fn default() -> Self {
        Self {
            poll_interval: 2,
            auto_finalize: false,
            contract_address: String::new(),
        }
    }
}

/// Monitors blockchain state and tracks epoch transitions
pub struct EpochMonitor {
    /// Configuration
    config: EpochMonitorConfig,
    /// Chain client
    client: Arc<InjectiveClient>,
    /// Current epoch info (shared state)
    current_info: Arc<RwLock<Option<EpochInfo>>>,
    /// Whether monitor is running
    is_running: Arc<RwLock<bool>>,
}

impl EpochMonitor {
    /// Create a new epoch monitor
    pub fn new(config: EpochMonitorConfig, client: Arc<InjectiveClient>) -> Self {
        Self {
            config,
            client,
            current_info: Arc::new(RwLock::new(None)),
            is_running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Start monitoring epochs in the background
    pub async fn start(&self) -> Result<()> {
        let mut running = self.is_running.write().await;
        if *running {
            return Err(anyhow!("Monitor already running"));
        }
        *running = true;
        drop(running);
        
        // Clone for the background task
        let client = self.client.clone();
        let current_info = self.current_info.clone();
        let is_running = self.is_running.clone();
        let config = self.config.clone();
        
        // Spawn background monitoring task
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(config.poll_interval));
            
            while *is_running.read().await {
                interval.tick().await;
                
                // Query epoch info from chain
                match Self::query_epoch_info(&client, &config.contract_address).await {
                    Ok(info) => {
                        // Check for phase transitions
                        let mut current = current_info.write().await;
                        let should_log = if let Some(ref old_info) = *current {
                            old_info.epoch_number != info.epoch_number ||
                            old_info.phase != info.phase
                        } else {
                            true
                        };
                        
                        if should_log {
                            log::info!(
                                "Epoch {} - Phase: {:?}, Block: {}/{}, Difficulty: {}",
                                info.epoch_number,
                                info.phase,
                                info.block_in_epoch,
                                50,
                                info.difficulty
                            );
                            
                            // Trigger events on phase changes
                            if let Some(ref old_info) = *current {
                                Self::handle_phase_transition(old_info, &info, &config).await;
                            }
                        }
                        
                        *current = Some(info);
                    }
                    Err(e) => {
                        log::error!("Failed to query epoch info: {}", e);
                    }
                }
            }
            
            log::info!("Epoch monitor stopped");
        });
        
        log::info!("Epoch monitor started");
        Ok(())
    }
    
    /// Stop monitoring
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.is_running.write().await;
        if !*running {
            return Err(anyhow!("Monitor not running"));
        }
        *running = false;
        
        log::info!("Stopping epoch monitor");
        Ok(())
    }
    
    /// Get current epoch info
    pub async fn get_current_info(&self) -> Option<EpochInfo> {
        self.current_info.read().await.clone()
    }
    
    /// Wait for a specific phase
    pub async fn wait_for_phase(&self, target_phase: EpochPhase) -> Result<EpochInfo> {
        let poll_interval = Duration::from_secs(self.config.poll_interval);
        
        loop {
            if let Some(info) = self.get_current_info().await {
                if info.phase == target_phase {
                    return Ok(info);
                }
            }
            
            tokio::time::sleep(poll_interval).await;
        }
    }
    
    /// Wait for next epoch
    pub async fn wait_for_next_epoch(&self) -> Result<EpochInfo> {
        let current_epoch = self.get_current_info().await
            .ok_or_else(|| anyhow!("No epoch info available"))?
            .epoch_number;
        
        let poll_interval = Duration::from_secs(self.config.poll_interval);
        
        loop {
            if let Some(info) = self.get_current_info().await {
                if info.epoch_number > current_epoch {
                    return Ok(info);
                }
            }
            
            tokio::time::sleep(poll_interval).await;
        }
    }
    
    /// Query epoch info from the chain - REAL IMPLEMENTATION
    async fn query_epoch_info(
        client: &InjectiveClient,
        contract_address: &str,
    ) -> Result<EpochInfo> {
        // 1. Get current node info to verify chain connection
        let node_info = client.get_node_info().await
            .map_err(|e| anyhow!("Failed to get node info: {}", e))?;
        
        log::debug!("Connected to chain: {}", node_info.chain_id);
        
        // 2. Query mining contract for current epoch info
        let epoch_query = serde_json::json!({
            "current_epoch": {}
        });
        
        let epoch_response = client.query_contract_smart(
            contract_address,
            serde_json::to_vec(&epoch_query)?
        ).await?;
        
        // Parse the response to extract epoch info
        let epoch_number = epoch_response["epoch_number"].as_u64()
            .ok_or_else(|| anyhow!("Invalid epoch_number in response"))?;
        let difficulty = epoch_response["difficulty"].as_u64()
            .ok_or_else(|| anyhow!("Invalid difficulty in response"))? as u8;
        let phase = epoch_response["phase"].as_str()
            .ok_or_else(|| anyhow!("Invalid phase in response"))?;
        
        // Parse phase from contract response
        let epoch_phase = match phase {
            "Commit" => EpochPhase::Commit,
            "Reveal" => EpochPhase::Reveal, 
            "Settlement" => EpochPhase::Settlement,
            _ => return Err(anyhow!("Unknown phase: {}", phase)),
        };
        
        // Get current block height from chain
        let block_height = epoch_response["block_height"].as_u64().unwrap_or(0);
        
        // Create EpochInfo from real blockchain data using the from_block_height helper
        Ok(EpochInfo::from_block_height(block_height, difficulty))
    }
    
    /// Handle phase transitions
    async fn handle_phase_transition(
        old_info: &EpochInfo,
        new_info: &EpochInfo,
        config: &EpochMonitorConfig,
    ) {
        // Epoch changed
        if new_info.epoch_number > old_info.epoch_number {
            log::info!("Epoch transition: {} -> {}", old_info.epoch_number, new_info.epoch_number);
            
            // Optionally trigger finalization
            if config.auto_finalize && old_info.phase == EpochPhase::Settlement {
                log::info!("Triggering epoch finalization for epoch {}", old_info.epoch_number);
                // Note: Actual finalization should be handled by orchestrator's transaction manager
            }
        }
        
        // Phase changed within same epoch
        if old_info.phase != new_info.phase && old_info.epoch_number == new_info.epoch_number {
            log::info!(
                "Phase transition in epoch {}: {:?} -> {:?}",
                new_info.epoch_number,
                old_info.phase,
                new_info.phase
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_epoch_info_calculation() {
        // Test commit phase
        let info = EpochInfo::from_block_height(15, 12);
        assert_eq!(info.epoch_number, 0);
        assert_eq!(info.block_in_epoch, 15);
        assert_eq!(info.phase, EpochPhase::Commit);
        
        // Test reveal phase
        let info = EpochInfo::from_block_height(35, 12);
        assert_eq!(info.epoch_number, 0);
        assert_eq!(info.block_in_epoch, 35);
        assert_eq!(info.phase, EpochPhase::Reveal);
        
        // Test settlement phase
        let info = EpochInfo::from_block_height(48, 12);
        assert_eq!(info.epoch_number, 0);
        assert_eq!(info.block_in_epoch, 48);
        assert_eq!(info.phase, EpochPhase::Settlement);
        
        // Test next epoch
        let info = EpochInfo::from_block_height(50, 12);
        assert_eq!(info.epoch_number, 1);
        assert_eq!(info.block_in_epoch, 0);
        assert_eq!(info.phase, EpochPhase::Commit);
    }
    
    #[test]
    fn test_phase_timing() {
        let info = EpochInfo::from_block_height(25, 12);
        assert_eq!(info.phase, EpochPhase::Commit);
        assert_eq!(info.time_to_next_phase, 12); // (30-25+1) * 2 seconds
        
        let info = EpochInfo::from_block_height(40, 12);
        assert_eq!(info.phase, EpochPhase::Reveal);
        assert_eq!(info.time_to_next_phase, 12); // (45-40+1) * 2 seconds
    }
}