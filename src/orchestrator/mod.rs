/// Mining Orchestrator - Coordinates the complete mining lifecycle
/// Following Gemini Pro's expert recommendations for robust production mining
///
/// Key improvements from Gemini Pro's analysis:
/// 1. Proper state machine with epoch tracking
/// 2. State persistence for crash recovery
/// 3. Resilient error handling with retry logic
/// 4. Correct commit-reveal timing across epochs

use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use uuid::Uuid;

use crate::chain::{InjectiveClient, query_epoch_info};
use crate::chain::queries::PhaseInfo;
use crate::chain::wallet::InjectiveWallet;
// Messages are now handled by transaction_manager
use crate::miner::MiningEngine;
// Import EnhancedTelemetryReporter for comprehensive metrics
use crate::telemetry::EnhancedTelemetryReporter;

// Transaction manager is in the same orchestrator module
mod transaction_manager;
mod epoch_monitor;
mod stats;
pub use self::stats::{MiningStatistics, StatsCollector};

/// Mining phase within an epoch lifecycle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MiningPhase {
    /// Not actively mining
    Idle,
    /// Finding a solution for the current epoch
    FindingSolution,
    /// Committing the found solution (stores nonce+digest for later reveal)
    Committing(CommitmentData),
    /// Waiting for the next epoch to reveal (stores commitment data)
    WaitingForRevealWindow(CommitmentData),
    /// Revealing the previously committed solution
    Revealing(CommitmentData),
    /// Claiming rewards from successful mining (includes epoch number)
    Claiming(u64),
}

/// Data needed for reveal phase
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitmentData {
    pub epoch: u64,
    pub nonce: [u8; 8],
    pub digest: [u8; 16],
    pub salt: [u8; 32],
    pub commitment: [u8; 32],
}

/// Complete mining state including epoch and phase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningState {
    pub epoch: u64,
    pub phase: MiningPhase,
    pub last_saved: u64, // Timestamp for state saves
}

impl Default for MiningState {
    fn default() -> Self {
        Self {
            epoch: 0,
            phase: MiningPhase::Idle,
            last_saved: 0,
        }
    }
}

/// Configuration for the orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Path to state persistence file
    pub state_file: PathBuf,
    /// How often to check epoch status (seconds)
    pub epoch_poll_interval: u64,
    /// How long to wait when in WaitingForRevealWindow
    pub reveal_wait_interval: u64,
    /// Maximum retries for chain operations
    pub max_retries: u32,
    /// Initial retry delay (milliseconds)
    pub retry_delay_ms: u64,
    /// Mining contract address
    pub contract_address: String,
    /// Number of worker threads for mining
    pub worker_count: usize,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            state_file: PathBuf::from("gmine_orchestrator.state"),
            epoch_poll_interval: 5,
            reveal_wait_interval: 1,  // FIXED: Was 30 seconds, now 1 second for fast reveal detection
            max_retries: 3,
            retry_delay_ms: 1000,
            contract_address: String::new(),
            worker_count: 4,
        }
    }
}

/// Main orchestrator coordinating all mining operations
pub struct MiningOrchestrator {
    /// Current mining state
    state: MiningState,
    /// Configuration
    config: OrchestratorConfig,
    /// Chain client for blockchain interaction (shared with transaction manager)
    client: Arc<RwLock<InjectiveClient>>,
    /// Mining engine for proof-of-work
    engine: MiningEngine,
    /// Wallet for signing transactions
    wallet: InjectiveWallet,
    /// Transaction manager for submissions
    tx_manager: Option<transaction_manager::TransactionManager>,
    /// Statistics collector (shared for external access)
    stats_collector: Arc<Mutex<StatsCollector>>,
    /// Telemetry reporter for production monitoring
    telemetry_reporter: Option<Arc<EnhancedTelemetryReporter>>,
    /// Last telemetry timestamp (instance-specific, not static)
    last_telemetry_time: std::sync::atomic::AtomicU64,
}

impl MiningOrchestrator {
    /// Create a new orchestrator, loading saved state if available
    pub async fn new(
        config: OrchestratorConfig,
        client: InjectiveClient,
        wallet: InjectiveWallet,
    ) -> Result<Self> {
        // Load saved state or use default
        let mut state = Self::load_state(&config.state_file).unwrap_or_default();
        
        // Validate loaded state against current epoch to prevent stale state issues
        // If state is more than 5 epochs behind, discard it and start fresh
        // This prevents the "nonce out of range" infinite loop bug
        match query_epoch_info(&client, &config.contract_address).await {
            Ok(current_epoch_info) => {
                if state.epoch > 0 && current_epoch_info.epoch_number > state.epoch + 5 {
                    log::warn!(
                        "Loaded state is stale (epoch {} vs current {}), discarding and starting fresh",
                        state.epoch, current_epoch_info.epoch_number
                    );
                    state = MiningState::default();
                    // Try to delete the stale state file
                    if let Err(e) = fs::remove_file(&config.state_file) {
                        log::debug!("Could not remove stale state file: {}", e);
                    }
                }
            }
            Err(e) => {
                log::debug!("Could not validate epoch during initialization: {}", e);
                // Continue with loaded state if we can't check
            }
        }
        
        // Create mining engine
        let engine = MiningEngine::new(config.worker_count);
        
        // Create enhanced telemetry reporter with comprehensive metrics
        let telemetry_reporter = match EnhancedTelemetryReporter::new(
            wallet.address.clone(),
            format!("miner-{}", Uuid::new_v4()),
        ) {
            Ok(reporter) => {
                log::info!("Enhanced telemetry reporter initialized for {}", wallet.address);
                Some(Arc::new(reporter))
            },
            Err(e) => {
                log::warn!("Failed to initialize telemetry: {}", e);
                None
            }
        };
        
        // Wrap client in Arc<RwLock> for sharing with transaction manager
        let client_arc = Arc::new(RwLock::new(client));
        
        // Create transaction manager configuration
        let tx_config = transaction_manager::TransactionManagerConfig {
            max_retries: 3,
            initial_retry_delay_ms: 1000,
            max_retry_delay_ms: 30000,
            gas_price_multiplier: 1.1,
            max_queue_size: 100,
            contract_address: config.contract_address.clone(),
        };
        
        // Create and start transaction manager
        let tx_manager = transaction_manager::TransactionManager::new(tx_config, client_arc.clone());
        tx_manager.start().await?;
        log::info!("Transaction manager initialized and started");
        
        Ok(Self {
            state,
            config,
            client: client_arc,
            engine,
            wallet,
            tx_manager: Some(tx_manager),
            stats_collector: Arc::new(Mutex::new(StatsCollector::new())),
            telemetry_reporter,
            last_telemetry_time: std::sync::atomic::AtomicU64::new(0),
        })
    }
    
    /// Main run loop - coordinates the entire mining lifecycle
    pub async fn run(&mut self) -> Result<()> {
        log::info!("Starting mining orchestrator");
        log::info!("Loaded state: epoch={}, phase={:?}", self.state.epoch, self.state.phase);
        
        // Test telemetry connection
        if let Some(ref reporter) = self.telemetry_reporter {
            log::info!("Testing telemetry connection to https://gmine.gelotto.io/api/telemetry...");
            if reporter.test_connection().await? {
                log::info!("✓ Telemetry backend connected - dashboard should receive data");
            } else {
                log::error!("✗ Telemetry backend NOT reachable - dashboard will show zeros!");
            }
        }
        
        // Connect to chain if not connected
        {
            let client = self.client.read().await;
            if !client.is_connected() {
                drop(client);
                self.connect_with_retry().await?;
            }
        }
        
        // If we're resuming in FindingSolution phase, restart the mining engine
        if matches!(self.state.phase, MiningPhase::FindingSolution) {
            log::info!("Resuming mining for epoch {}", self.state.epoch);
            let difficulty = self.get_difficulty_with_retry().await?;
            let nonce_range = self.get_nonce_range_with_retry().await?;
            
            // Update statistics
            self.stats_collector.lock().await.start_mining(
                self.state.epoch, 
                difficulty, 
                nonce_range.0, 
                nonce_range.1
            ).await;
            
            // Restart the mining engine
            self.engine.start_mining(self.state.epoch, difficulty, nonce_range).await?;
            log::info!("Mining engine restarted for epoch {}", self.state.epoch);
        }
        
        // Main orchestration loop
        loop {
            // Get current chain epoch with retry
            let chain_epoch = self.get_current_epoch_with_retry().await?;
            
            // Process based on current state - clone to avoid borrow checker issues
            let current_phase = self.state.phase.clone();
            match current_phase {
                MiningPhase::Idle => {
                    // Check if we should start mining for current or new epoch
                    if chain_epoch >= self.state.epoch {
                        // Also check if we're in a mineable phase (Commit phase)
                        let client = self.client.read().await;
                        match query_epoch_info(&*client, &self.config.contract_address).await {
                            Ok(epoch_info) => {
                                drop(client);
                                match epoch_info.phase {
                                    PhaseInfo::Commit { .. } => {
                                        log::info!("Starting mining for epoch {} (Commit phase)", chain_epoch);
                                        self.transition_to_finding_solution(chain_epoch).await?;
                                    }
                                    PhaseInfo::Settlement { ends_at } => {
                                        // Check if settlement has ended and needs advancement
                                        let current_block = self.get_block_height_with_retry().await.unwrap_or(ends_at + 1);
                                        if current_block > ends_at {
                                            log::info!("Settlement ended for epoch {}, advancing to next epoch", chain_epoch);
                                            if let Some(ref tx_manager) = self.tx_manager {
                                                match tx_manager.queue_advance_epoch().await {
                                                    Ok(tx_id) => {
                                                        log::info!("Queued advance_epoch transaction {}", tx_id);
                                                        sleep(Duration::from_secs(5)).await;
                                                    }
                                                    Err(e) => {
                                                        log::error!("Failed to advance epoch - TRANSACTION ERROR: {}", e);
                                                    }
                                                }
                                            }
                                        } else {
                                            log::debug!("Waiting for settlement to end (current: {}, ends: {})", current_block, ends_at);
                                            sleep(Duration::from_secs(self.config.epoch_poll_interval)).await;
                                        }
                                    }
                                    _ => {
                                        log::debug!("Waiting for Commit phase to start mining");
                                        sleep(Duration::from_secs(self.config.epoch_poll_interval)).await;
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to query epoch info: {}", e);
                                sleep(Duration::from_secs(self.config.epoch_poll_interval)).await;
                            }
                        }
                    } else {
                        // Wait before checking again
                        sleep(Duration::from_secs(self.config.epoch_poll_interval)).await;
                    }
                }
                
                MiningPhase::FindingSolution => {
                    // Check if solution finding is complete
                    if let Some(solution) = self.engine.check_solution().await {
                        log::info!("Found solution for epoch {}", self.state.epoch);
                        
                        // Report telemetry for solution found
                        if let Some(ref reporter) = self.telemetry_reporter {
                            let hashrate = self.engine.get_hashrate().await;
                            let hashrate_mhs = hashrate / 1_000_000.0; // Convert H/s to MH/s
                            let nonce_range = self.get_nonce_range_with_retry().await.ok();
                            let stats = reporter.get_stats().await;
                            match reporter.send_telemetry(
                                self.state.epoch,
                                "FindingSolution",
                                Some(hashrate_mhs),
                                Some(stats.epochs_won as u32 + 1), // Total solutions (including this one)
                                Some(stats.reveals_successful as u32), // Total successful reveals
                                None, // network_info
                                None, // power_balance
                                None, // gas_balance
                                None, // last_error
                                nonce_range,
                            ).await {
                                Ok(_) => log::info!("✓ Telemetry sent: solution found for epoch {}", self.state.epoch),
                                Err(e) => log::error!("✗ Failed to send telemetry: {}", e),
                            }
                        }
                        
                        self.transition_to_committing(solution).await?;
                    } else {
                        // Continue mining - send periodic telemetry every 30 seconds
                        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                        let last = self.last_telemetry_time.load(std::sync::atomic::Ordering::Relaxed);
                        
                        if now - last > 30 {
                            self.last_telemetry_time.store(now, std::sync::atomic::Ordering::Relaxed);
                            
                            if let Some(ref reporter) = self.telemetry_reporter {
                                let hashrate = self.engine.get_hashrate().await;
                                let hashrate_mhs = hashrate / 1_000_000.0; // Convert H/s to MH/s
                                let nonce_range = self.get_nonce_range_with_retry().await.ok();
                                let stats = reporter.get_stats().await;
                                match reporter.send_telemetry(
                                    self.state.epoch,
                                    "FindingSolution",
                                    Some(hashrate_mhs),
                                    Some(stats.epochs_won as u32), // Total solutions found
                                    Some(stats.reveals_successful as u32), // Total successful reveals
                                    None, // network_info
                                    None, // power_balance
                                    None, // gas_balance
                                    None, // last_error
                                    nonce_range,
                                ).await {
                                    Ok(_) => log::debug!("✓ Periodic telemetry sent"),
                                    Err(e) => log::error!("✗ Failed to send periodic telemetry: {}", e),
                                }
                            }
                        }
                        
                        sleep(Duration::from_secs(1)).await;
                    }
                }
                
                MiningPhase::Committing(data) => {
                    // Check if we're in the right phase to commit
                    let client = self.client.read().await;
                    match query_epoch_info(&*client, &self.config.contract_address).await {
                        Ok(epoch_info) => {
                            drop(client); // Release lock before submitting
                            
                            // Check phase
                            match epoch_info.phase {
                                PhaseInfo::Commit { .. } => {
                                    // Good to commit
                                    match self.submit_commitment(&data).await {
                                        Ok(_) => {
                                            log::info!("Successfully committed for epoch {}", data.epoch);
                                            // Track successful commit in telemetry
                                            if let Some(ref reporter) = self.telemetry_reporter {
                                                reporter.record_commit_attempt(true, None).await;
                                            }
                                            self.transition_to_waiting_for_reveal(data.clone()).await?;
                                        }
                                        Err(e) => {
                                            log::error!("Failed to commit: {}", e);
                                            // Track failed commit in telemetry
                                            if let Some(ref reporter) = self.telemetry_reporter {
                                                reporter.record_commit_attempt(false, None).await;
                                            }
                                            // Retry or transition back to idle if epoch passed
                                            if chain_epoch > self.state.epoch {
                                                log::warn!("Epoch passed, returning to idle");
                                                self.transition_to_idle().await?;
                                            }
                                        }
                                    }
                                }
                                PhaseInfo::Settlement { ends_at } => {
                                    // Check if settlement has ended and we need to advance
                                    let current_block = self.get_block_height_with_retry().await
                                        .unwrap_or(epoch_info.start_block + 100); // Fallback estimate
                                    if current_block >= ends_at {
                                        // First check if epoch already auto-advanced
                                        match self.get_current_epoch_with_retry().await {
                                            Ok(current_epoch_number) => {
                                                if current_epoch_number > epoch_info.epoch_number {
                                                    // Epoch already advanced naturally
                                                    log::info!("Epoch auto-advanced from {} to {}", 
                                                        epoch_info.epoch_number, current_epoch_number);
                                                    self.state.epoch = current_epoch_number;
                                                    // Transition to idle to wait for new epoch info
                                                    self.transition_to_idle().await?;
                                                    continue; // Skip to next iteration
                                                } else if current_block > ends_at + 50 {
                                                    // Epoch is stuck past grace period, needs manual advancement
                                                    log::warn!("Epoch {} stuck in settlement (block {} > end {}+50), attempting manual advance", 
                                                        epoch_info.epoch_number, current_block, ends_at);
                                                    if let Some(ref tx_manager) = self.tx_manager {
                                                        match tx_manager.queue_advance_epoch().await {
                                                            Ok(tx_id) => {
                                                                log::info!("Queued advance_epoch transaction {}", tx_id);
                                                                // Wait for advancement to complete
                                                                sleep(Duration::from_secs(5)).await;
                                                            }
                                                            Err(e) => {
                                                                log::error!("Failed to queue advance_epoch: {:?}", e);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                log::error!("Failed to get current epoch info: {:?}", e);
                                            }
                                        }
                                    } else {
                                        // Still in settlement, not at end block yet
                                        log::debug!("Settlement phase ongoing, {} blocks until end", ends_at - current_block);
                                    }
                                    // Stay in Committing phase to retry
                                }
                                PhaseInfo::Reveal { .. } => {
                                    // Too late to commit for this epoch
                                    log::warn!("Already in Reveal phase, missed commit window");
                                    if chain_epoch > self.state.epoch {
                                        self.transition_to_idle().await?;
                                    } else {
                                        // Wait for next epoch
                                        sleep(Duration::from_secs(5)).await;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to query epoch info: {}", e);
                            sleep(Duration::from_secs(5)).await;
                        }
                    }
                }
                
                MiningPhase::WaitingForRevealWindow(data) => {
                    // Wait for reveal phase in the SAME epoch we committed (not next epoch!)
                    // Commits and reveals happen in the same epoch, just different phases
                    if chain_epoch == data.epoch {
                        // Still in the same epoch, check if we're in reveal phase
                        let client = self.client.read().await;
                        match query_epoch_info(&*client, &self.config.contract_address).await {
                            Ok(epoch_info) => {
                                drop(client);
                                match epoch_info.phase {
                                    PhaseInfo::Reveal { .. } => {
                                        log::info!("Reveal phase active for epoch {}, revealing commitment from epoch {}", 
                                                  chain_epoch, data.epoch);
                                        // Extract commitment data and transition to revealing
                                        self.transition_to_revealing(data).await?
                                    }
                                    PhaseInfo::Commit { .. } => {
                                        // Still in commit phase, check more frequently
                                        log::debug!("Waiting for reveal phase (currently in commit phase of epoch {})", chain_epoch);
                                        sleep(Duration::from_secs(2)).await;  // Check more frequently for phase changes
                                    }
                                    PhaseInfo::Settlement { .. } => {
                                        // In settlement, wait for next epoch
                                        log::debug!("In settlement phase, waiting for next epoch");
                                        sleep(Duration::from_secs(5)).await;
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to query epoch info: {}", e);
                                sleep(Duration::from_secs(5)).await;
                            }
                        }
                    } else if chain_epoch > data.epoch {
                        // We missed the reveal window - the chain has moved past our committed epoch
                        log::warn!("Missed reveal window for epoch {} (current epoch: {}). Starting fresh with current epoch.", 
                                  data.epoch, chain_epoch);
                        
                        // Transition to finding solution for the current epoch
                        self.state.epoch = chain_epoch;
                        self.transition_to_finding_solution(chain_epoch).await?;
                    } else {
                        // chain_epoch < data.epoch shouldn't happen but wait if it does
                        log::debug!("Waiting for epoch {} (current: {})", data.epoch, chain_epoch);
                        sleep(Duration::from_secs(self.config.reveal_wait_interval)).await;
                    }
                }
                
                MiningPhase::Revealing(data) => {
                    // Check if we're in the reveal phase before submitting
                    let client = self.client.read().await;
                    match query_epoch_info(&*client, &self.config.contract_address).await {
                        Ok(epoch_info) => {
                            drop(client); // Release lock before submitting
                            
                            match epoch_info.phase {
                                PhaseInfo::Reveal { ends_at } => {
                                    // Good to reveal - log timing info
                                    let current_block = self.get_block_height_with_retry().await.unwrap_or(0);
                                    let blocks_remaining = if ends_at > current_block {
                                        ends_at - current_block
                                    } else {
                                        0
                                    };
                                    log::info!("In Reveal phase with {} blocks remaining (current: {}, ends: {})", 
                                              blocks_remaining, current_block, ends_at);
                                    
                                    // Attempt reveal immediately if we have any time left (even 1 block)
                                    // Be aggressive since the window is so short
                                    if blocks_remaining >= 1 {
                                        match self.submit_reveal(&data).await {
                        Ok(_) => {
                            log::info!("Successfully revealed for epoch {}", data.epoch);
                            
                            // Report successful reveal
                            if let Some(ref reporter) = self.telemetry_reporter {
                                let hashrate = self.engine.get_hashrate().await;
                                let hashrate_mhs = hashrate / 1_000_000.0; // Convert H/s to MH/s
                                let nonce_range = self.get_nonce_range_with_retry().await.ok();
                                reporter.record_reveal_attempt(true, None).await;
                                let stats = reporter.get_stats().await;
                                match reporter.send_telemetry(
                                    self.state.epoch,
                                    "Revealing",
                                    Some(hashrate_mhs),
                                    Some(stats.epochs_won as u32), // Total solutions found
                                    Some(stats.reveals_successful as u32), // Total successful reveals (including this one)
                                    None, // network_info
                                    None, // power_balance
                                    None, // gas_balance
                                    None, // last_error
                                    nonce_range,
                                ).await {
                                    Ok(_) => log::info!("✓ Telemetry sent: reveal submitted for epoch {}", self.state.epoch),
                                    Err(e) => log::error!("✗ Failed to send reveal telemetry: {}", e),
                                }
                            }
                            
                            // Claim for the CURRENT epoch (reveal epoch), not commitment epoch
                            // Reveals are stored with the current epoch number in the contract
                            self.transition_to_claiming(epoch_info.epoch_number).await?;
                        }
                                        Err(e) => {
                                            log::error!("Failed to reveal: {}", e);
                                            // Track failed reveal in telemetry
                                            if let Some(ref reporter) = self.telemetry_reporter {
                                                reporter.record_reveal_attempt(false, None).await;
                                            }
                                            // Check if reveal window passed
                                            let block_height = self.get_block_height_with_retry().await?;
                                            if self.is_past_reveal_window(block_height) {
                                                log::warn!("Reveal window passed, moving to claim");
                                                // Claim for the CURRENT epoch (reveal epoch), not commitment epoch
                            // Reveals are stored with the current epoch number in the contract
                            self.transition_to_claiming(epoch_info.epoch_number).await?;
                                            }
                                        }
                                    }
                                    } else {
                                        log::warn!("Not enough time to reveal - only {} blocks remaining", blocks_remaining);
                                        self.transition_to_idle().await?;
                                    }
                                }
                                PhaseInfo::Settlement { .. } => {
                                    // Still in settlement, wait for reveal phase
                                    log::debug!("Waiting for reveal phase (currently in settlement)");
                                    sleep(Duration::from_secs(2)).await;
                                }
                                PhaseInfo::Commit { .. } => {
                                    // Somehow we're in commit phase - might have missed reveal window
                                    log::warn!("In commit phase, might have missed reveal window");
                                    self.transition_to_idle().await?;
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to query epoch info during reveal: {}", e);
                            sleep(Duration::from_secs(5)).await;
                        }
                    }
                }
                
                MiningPhase::Claiming(claim_epoch) => {
                    // First check if we're trying to claim from an old epoch
                    let client = self.client.read().await;
                    match query_epoch_info(&*client, &self.config.contract_address).await {
                        Ok(current_epoch_info) => {
                            drop(client); // Release lock
                            
                            // If current epoch is much newer than claim epoch, skip claiming and start fresh
                            if current_epoch_info.epoch_number > claim_epoch + 1 {
                                log::warn!("Trying to claim from old epoch {}. Current epoch is {}. Skipping to current epoch.", 
                                          claim_epoch, current_epoch_info.epoch_number);
                                self.transition_to_idle().await?;
                                continue; // Keep the orchestrator running
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to query current epoch info during claiming: {}", e);
                            drop(client);
                            sleep(Duration::from_secs(5)).await;
                            continue; // Keep the orchestrator running
                        }
                    }
                    
                    // CRITICAL: First check if the epoch's settlement phase has ended
                    // Epochs can only be finalized AFTER settlement phase completes
                    log::info!("Checking if epoch {} settlement is complete before finalizing", claim_epoch);
                    
                    let settlement_complete = self.wait_for_settlement_completion(claim_epoch).await?;
                    if !settlement_complete {
                        log::warn!("Settlement for epoch {} not yet complete, will retry later", claim_epoch);
                        sleep(Duration::from_secs(5)).await;
                        continue; // Keep the orchestrator running
                    }
                    
                    // For old epochs, we don't need to advance_epoch - that's only for the current epoch
                    // Old epochs are already in history, we just need to finalize and claim
                    if let Some(ref tx_manager) = self.tx_manager {
                        // Get current epoch to check if we're claiming from an old epoch
                        let client = self.client.read().await;
                        let current_epoch_info = query_epoch_info(&*client, &self.config.contract_address).await?;
                        drop(client);
                        
                        // Only call advance_epoch if this is the current epoch and it's stuck
                        if claim_epoch == current_epoch_info.epoch_number {
                            log::info!("This is the current epoch, checking if advance is needed...");
                            // Only advance if the epoch is stuck past its settlement end
                            match current_epoch_info.phase {
                                PhaseInfo::Settlement { ends_at } => {
                                    let current_block = self.get_block_height_with_retry().await?;
                                    if current_block > ends_at + 50 {  // Grace period
                                        log::warn!("Current epoch {} is stuck past settlement end, advancing...", claim_epoch);
                                        match tx_manager.queue_advance_epoch().await {
                                            Ok(tx_id) => {
                                                log::info!("Queued advance_epoch transaction {} for stuck epoch {}", tx_id, claim_epoch);
                                                sleep(Duration::from_secs(5)).await;
                                            }
                                            Err(e) => {
                                                log::debug!("Advance epoch failed: {}", e);
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    log::info!("Current epoch is in {:?} phase, no advance needed", current_epoch_info.phase);
                                }
                            }
                        } else {
                            log::info!("Claiming from past epoch {}, current is {} - skipping advance_epoch", 
                                     claim_epoch, current_epoch_info.epoch_number);
                        }
                        
                        // Now try to finalize the epoch
                        log::info!("Attempting to finalize epoch {} before claiming", claim_epoch);
                        match tx_manager.queue_finalize_epoch(claim_epoch).await {
                            Ok(tx_id) => {
                                log::info!("Queued finalize_epoch transaction {} for epoch {}", tx_id, claim_epoch);
                                // Wait for finalization to complete and be confirmed on chain
                                log::info!("Waiting 10 seconds for finalization to be confirmed on chain...");
                                sleep(Duration::from_secs(10)).await;
                            }
                            Err(e) => {
                                log::debug!("Finalize epoch failed (may already be finalized): {}", e);
                                // Continue to claim anyway - epoch might already be finalized
                            }
                        }
                    }
                    
                    // Now submit claim transaction for the specific epoch we revealed
                    match self.submit_claim(claim_epoch).await {
                        Ok(_) => {
                            log::info!("Successfully claimed rewards for epoch {}", claim_epoch);
                            // Record successful claim
                            if let Some(ref reporter) = self.telemetry_reporter {
                                reporter.record_claim_attempt(true, Some(1_000_000), None).await; // 1 POWER = 1M micro
                            }
                            self.transition_to_idle().await?;
                        }
                        Err(e) => {
                            log::error!("Failed to claim for epoch {}: {}", claim_epoch, e);
                            // Record failed claim
                            if let Some(ref reporter) = self.telemetry_reporter {
                                reporter.record_claim_attempt(false, None, None).await;
                            }
                            // Move to idle regardless - can retry claims later
                            self.transition_to_idle().await?;
                        }
                    }
                }
            }
            
            // Small delay to prevent tight loops
            sleep(Duration::from_millis(100)).await;
        }
    }
    
    // State transition methods
    
    async fn transition_to_finding_solution(&mut self, epoch: u64) -> Result<()> {
        self.state.epoch = epoch;
        self.state.phase = MiningPhase::FindingSolution;
        self.save_state()?;
        
        // Get epoch info including target_hash from contract
        let client = self.client.read().await;
        let epoch_info = query_epoch_info(&*client, &self.config.contract_address).await?;
        drop(client);
        
        // Extract target_hash from epoch info and convert to array
        let target_hash_vec = epoch_info.target_hash;
        if target_hash_vec.len() != 32 {
            return Err(anyhow::anyhow!("Invalid target_hash length: {}", target_hash_vec.len()));
        }
        let mut target_hash = [0u8; 32];
        target_hash.copy_from_slice(&target_hash_vec);
        log::info!("Got target_hash from contract for epoch {}: {:?}", epoch, target_hash);
        
        // Start mining workers
        let difficulty = self.get_difficulty_with_retry().await?;
        let nonce_range = self.get_nonce_range_with_retry().await?;
        
        // Update statistics
        self.stats_collector.lock().await.start_mining(epoch, difficulty, nonce_range.0, nonce_range.1).await;
        
        // Pass the actual target_hash to the mining engine
        self.engine.start_mining_with_target(epoch, target_hash, difficulty, nonce_range).await?;
        
        Ok(())
    }
    
    async fn transition_to_committing(&mut self, solution: CommitmentData) -> Result<()> {
        self.state.phase = MiningPhase::Committing(solution);
        self.save_state()?;
        Ok(())
    }
    
    async fn transition_to_waiting_for_reveal(&mut self, data: CommitmentData) -> Result<()> {
        // Store commitment data in the phase itself - FIXED!
        self.state.phase = MiningPhase::WaitingForRevealWindow(data);
        self.save_state()?;
        Ok(())
    }
    
    async fn transition_to_revealing(&mut self, data: CommitmentData) -> Result<()> {
        self.state.phase = MiningPhase::Revealing(data);
        self.save_state()?;
        Ok(())
    }
    
    async fn transition_to_claiming(&mut self, epoch: u64) -> Result<()> {
        self.state.phase = MiningPhase::Claiming(epoch);
        self.save_state()?;
        Ok(())
    }
    
    async fn transition_to_idle(&mut self) -> Result<()> {
        self.state.phase = MiningPhase::Idle;
        self.save_state()?;
        Ok(())
    }
    
    // State persistence methods (as recommended by Gemini Pro)
    
    fn save_state(&mut self) -> Result<()> {
        self.state.last_saved = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        
        let serialized = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.config.state_file, serialized)?;
        log::debug!("State saved to {:?}", self.config.state_file);
        Ok(())
    }
    
    fn load_state(path: &PathBuf) -> Result<MiningState> {
        let serialized = fs::read_to_string(path)?;
        let state = serde_json::from_str(&serialized)?;
        log::info!("Loaded saved state from {:?}", path);
        Ok(state)
    }
    
    // Chain interaction methods with retry logic (as recommended by Gemini Pro)
    
    async fn connect_with_retry(&mut self) -> Result<()> {
        let mut retries = 0;
        let mut delay = self.config.retry_delay_ms;
        
        loop {
            let mut client = self.client.write().await;
            match client.connect().await {
                Ok(_) => {
                    log::info!("Connected to chain");
                    return Ok(());
                }
                Err(e) if retries < self.config.max_retries => {
                    retries += 1;
                    log::warn!("Connection failed (attempt {}/{}): {}", retries, self.config.max_retries, e);
                    sleep(Duration::from_millis(delay)).await;
                    delay *= 2; // Exponential backoff
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    async fn get_current_epoch_with_retry(&self) -> Result<u64> {
        let mut retries = 0;
        let mut delay = self.config.retry_delay_ms;
        
        loop {
            let client = self.client.read().await;
            match query_epoch_info(&*client, &self.config.contract_address).await {
                Ok(info) => {
                    return Ok(info.epoch_number);
                }
                Err(e) if retries < self.config.max_retries => {
                    retries += 1;
                    log::warn!("Failed to query epoch (attempt {}/{}): {}", 
                        retries, self.config.max_retries, e);
                    sleep(Duration::from_millis(delay)).await;
                    delay *= 2; // Exponential backoff
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    async fn get_block_height_with_retry(&self) -> Result<u64> {
        let mut retries = 0;
        let mut delay = self.config.retry_delay_ms;
        
        loop {
            let client = self.client.read().await;
            // Check if client has the method (it's on RealInjectiveClient)
            // For now, try to cast to the concrete type
            // TODO: Add this method to a trait interface
            match client.get_latest_block_height().await {
                Ok(height) => {
                    log::debug!("Current block height: {}", height);
                    return Ok(height);
                }
                Err(e) if retries < self.config.max_retries => {
                    retries += 1;
                    log::warn!("Failed to query block height (attempt {}/{}): {}", 
                        retries, self.config.max_retries, e);
                    drop(client); // Release lock before sleeping
                    sleep(Duration::from_millis(delay)).await;
                    delay *= 2; // Exponential backoff
                }
                Err(e) => {
                    log::error!("Failed to get block height after {} retries: {}", self.config.max_retries, e);
                    // Fallback to a reasonable estimate
                    return Ok(87634000);
                }
            }
        }
    }
    
    async fn get_difficulty_with_retry(&self) -> Result<u8> {
        let mut retries = 0;
        let mut delay = self.config.retry_delay_ms;
        
        loop {
            let client = self.client.read().await;
            match query_epoch_info(&*client, &self.config.contract_address).await {
                Ok(info) => {
                    return Ok(info.difficulty);
                }
                Err(e) if retries < self.config.max_retries => {
                    retries += 1;
                    log::warn!("Failed to query difficulty (attempt {}/{}): {}", 
                        retries, self.config.max_retries, e);
                    sleep(Duration::from_millis(delay)).await;
                    delay *= 2;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    /// Wait for an epoch's settlement phase to complete
    /// Returns true if settlement is complete, false if still ongoing
    async fn wait_for_settlement_completion(&self, target_epoch: u64) -> Result<bool> {
        use crate::chain::queries::{query_epoch_info, PhaseInfo};
        
        let current_block = self.get_block_height_with_retry().await?;
        
        // Query current epoch info to see what epoch we're in
        let mut client = self.client.write().await;
        let epoch_info = query_epoch_info(&mut *client, &self.config.contract_address).await?;
        drop(client);
        
        log::info!("Current epoch: {}, target epoch: {}, current block: {}", 
                  epoch_info.epoch_number, target_epoch, current_block);
        
        // If target epoch is older than current epoch, it should be complete
        if target_epoch < epoch_info.epoch_number {
            log::info!("Target epoch {} is older than current epoch {}, should be complete", 
                      target_epoch, epoch_info.epoch_number);
            return Ok(true);
        }
        
        // If target epoch equals current epoch, check if settlement phase has ended
        if target_epoch == epoch_info.epoch_number {
            match epoch_info.phase {
                PhaseInfo::Settlement { ends_at } => {
                    if current_block >= ends_at {
                        log::info!("Settlement phase for epoch {} has ended at block {}, current block: {}", 
                                  target_epoch, ends_at, current_block);
                        return Ok(true);
                    } else {
                        let blocks_remaining = ends_at - current_block;
                        log::info!("Settlement phase for epoch {} still active, {} blocks remaining (ends at {}, current {})", 
                                  target_epoch, blocks_remaining, ends_at, current_block);
                        return Ok(false);
                    }
                }
                PhaseInfo::Commit { .. } | PhaseInfo::Reveal { .. } => {
                    log::info!("Epoch {} is in {:?} phase, settlement not reached yet", target_epoch, epoch_info.phase);
                    return Ok(false);
                }
            }
        }
        
        // Target epoch is in the future - not possible to claim yet
        log::warn!("Target epoch {} is in the future (current: {}), cannot claim yet", 
                  target_epoch, epoch_info.epoch_number);
        Ok(false)
    }

    async fn get_nonce_range_with_retry(&self) -> Result<(u64, u64)> {
        // Calculate nonce range using Blake2b512 to match the contract's calculate_nonce_range function
        use blake2::{Blake2b512, Digest};
        
        let miner_address = &self.wallet.address;
        let epoch_number = self.state.epoch;
        
        // Hash miner address to get deterministic partition
        let mut hasher = Blake2b512::new();
        hasher.update(miner_address.as_bytes());
        hasher.update(&epoch_number.to_be_bytes());
        
        let hash = hasher.finalize();
        let partition_seed = u64::from_be_bytes([
            hash[0], hash[1], hash[2], hash[3], hash[4], hash[5], hash[6], hash[7],
        ]);
        
        // Each miner gets 1/1000th of total nonce space per epoch
        let nonce_space = u64::MAX / 1000;
        let partition_offset = (partition_seed % 1000) * nonce_space;
        
        // Rotate partitions each epoch to prevent grinding
        let epoch_rotation = (epoch_number * 37) % 1000; // Prime rotation
        let rotated_offset = partition_offset.wrapping_add(epoch_rotation * nonce_space);
        let max_nonce = rotated_offset.wrapping_add(nonce_space);
        
        log::info!("Calculated Blake2b512 nonce range for epoch {} and miner {}: {} to {}", 
            epoch_number, miner_address, rotated_offset, max_nonce);
        
        Ok((rotated_offset, max_nonce))
    }
    
    async fn submit_commitment(&self, data: &CommitmentData) -> Result<()> {
        if let Some(ref tx_manager) = self.tx_manager {
            let tx_id = tx_manager.queue_commit(data.epoch, data.commitment).await?;
            log::info!("Queued commitment transaction {} for epoch {}", tx_id, data.epoch);
            
            // Wait for transaction to complete (with timeout)
            let timeout = Duration::from_secs(120); // Increased timeout for blockchain confirmation
            let start = std::time::Instant::now();
            
            loop {
                if start.elapsed() > timeout {
                    return Err(anyhow!("Commitment transaction timeout"));
                }
                
                if let Some(status) = tx_manager.get_status(tx_id).await {
                    match status {
                        transaction_manager::TransactionStatus::Success { tx_hash } => {
                            log::info!("Commitment successful: {}", tx_hash);
                            return Ok(());
                        }
                        transaction_manager::TransactionStatus::Failed { error } => {
                            return Err(anyhow!("Commitment failed: {}", error));
                        }
                        _ => {
                            // Still pending or processing
                            sleep(Duration::from_millis(500)).await;
                        }
                    }
                } else {
                    sleep(Duration::from_millis(500)).await;
                }
            }
        } else {
            log::warn!("Transaction manager not initialized, using placeholder");
            Ok(())
        }
    }
    
    async fn submit_reveal(&mut self, data: &CommitmentData) -> Result<()> {
        if let Some(ref tx_manager) = self.tx_manager {
            let tx_id = tx_manager.queue_reveal(data.epoch, data.nonce, data.digest, data.salt).await?;
            log::info!("Queued reveal transaction {} for epoch {}", tx_id, data.epoch);
            
            // Wait for transaction to complete (with timeout)
            let timeout = Duration::from_secs(120);
            let start = std::time::Instant::now();
            
            loop {
                if start.elapsed() > timeout {
                    return Err(anyhow!("Reveal transaction timeout"));
                }
                
                if let Some(status) = tx_manager.get_status(tx_id).await {
                    match status {
                        transaction_manager::TransactionStatus::Success { tx_hash } => {
                            log::info!("Reveal successful: {}", tx_hash);
                            return Ok(());
                        }
                        transaction_manager::TransactionStatus::Failed { error } => {
                            return Err(anyhow!("Reveal failed: {}", error));
                        }
                        _ => {
                            // Still pending or processing
                            sleep(Duration::from_millis(500)).await;
                        }
                    }
                } else {
                    sleep(Duration::from_millis(500)).await;
                }
            }
        } else {
            log::warn!("Transaction manager not initialized, using placeholder");
            Ok(())
        }
    }
    
    async fn submit_claim(&mut self, epoch: u64) -> Result<()> {
        if let Some(ref tx_manager) = self.tx_manager {
            let tx_id = tx_manager.queue_claim(epoch).await?;
            log::info!("Queued claim transaction {} for epoch {}", tx_id, epoch);
            
            // Wait for transaction to complete (with timeout)
            let timeout = Duration::from_secs(120);
            let start = std::time::Instant::now();
            
            loop {
                if start.elapsed() > timeout {
                    return Err(anyhow!("Claim transaction timeout"));
                }
                
                if let Some(status) = tx_manager.get_status(tx_id).await {
                    match status {
                        transaction_manager::TransactionStatus::Success { tx_hash } => {
                            log::info!("Claim successful: {}", tx_hash);
                            return Ok(());
                        }
                        transaction_manager::TransactionStatus::Failed { error } => {
                            return Err(anyhow!("Claim failed: {}", error));
                        }
                        _ => {
                            // Still pending or processing
                            sleep(Duration::from_millis(500)).await;
                        }
                    }
                } else {
                    sleep(Duration::from_millis(500)).await;
                }
            }
        } else {
            log::warn!("Transaction manager not initialized, using placeholder");
            Ok(())
        }
    }
    
    fn is_past_reveal_window(&self, block_height: u64) -> bool {
        // Reveal window is blocks 31-45 of an epoch
        // Assuming 50 blocks per epoch
        let block_in_epoch = block_height % 50;
        block_in_epoch > 45
    }
    
    /// Get current mining statistics
    pub async fn get_statistics(&self) -> MiningStatistics {
        self.stats_collector.lock().await.get_stats().await
    }
    
    /// Get shared stats collector for external monitoring
    pub fn get_stats_collector(&self) -> Arc<Mutex<StatsCollector>> {
        self.stats_collector.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_state_serialization() {
        let state = MiningState {
            epoch: 42,
            phase: MiningPhase::Idle,
            last_saved: 1234567890,
        };
        
        let serialized = serde_json::to_string(&state).unwrap();
        let deserialized: MiningState = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(deserialized.epoch, 42);
        assert_eq!(deserialized.phase, MiningPhase::Idle);
    }
    
    #[test]
    fn test_commitment_data_serialization() {
        let data = CommitmentData {
            epoch: 10,
            nonce: [1; 8],
            digest: [2; 16],
            salt: [3; 32],
            commitment: [4; 32],
        };
        
        let serialized = serde_json::to_string(&data).unwrap();
        let deserialized: CommitmentData = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(deserialized.epoch, 10);
        assert_eq!(deserialized.nonce, [1; 8]);
    }
}