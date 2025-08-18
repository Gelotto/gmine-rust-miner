use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn, error};

use crate::thermal::ThermalManager;
use blake2::{Blake2b512, Digest};
use crate::real_mining::{RealMiningEngine, MiningSolution};
use crate::chain::{MobileChainClient, EpochInfo, NonceRange};
use hex;

/// Mining statistics for mobile UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningStats {
    pub hashrate: f64,
    pub solutions_found: u64,
    pub epoch: u64,
    pub is_mining: bool,
    pub thermal_throttled: bool,
    pub uptime_seconds: u64,
    pub last_solution_time: Option<String>,
    pub total_hashes: u64,
    pub current_difficulty: u8,
    pub rewards_earned: u64,
}

/// Production mobile mining engine with real drillx
pub struct MiningEngine {
    wallet_mnemonic: String,
    thermal_manager: ThermalManager,
    chain_client: MobileChainClient,
    real_engine: Option<RealMiningEngine>,
    
    // Mining state
    is_mining: AtomicBool,
    thread_count: AtomicU32,
    solutions_found: AtomicU64,
    current_epoch: AtomicU64,
    current_difficulty: AtomicU32,
    rewards_earned: AtomicU64,
    
    // Statistics
    start_time: RwLock<Option<Instant>>,
    last_solution_time: RwLock<Option<DateTime<Utc>>>,
    total_hashes: AtomicU64,
    
    // Mobile-specific constraints
    max_threads: u32,
    min_battery_level: f32,
    
    // Current epoch info
    current_epoch_info: RwLock<Option<EpochInfo>>,
    nonce_range: RwLock<Option<NonceRange>>,
}

impl MiningEngine {
    pub async fn new(wallet_mnemonic: String, thermal_manager: ThermalManager) -> Result<Self> {
        // Create chain client for testnet (for now)
        let contract_address = "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y".to_string(); // V2 contract
        let chain_client = MobileChainClient::new(
            wallet_mnemonic.clone(),
            contract_address,
            true, // testnet
        ).await?;
        
        // For mobile, limit threads based on CPU cores but cap at 4
        let cpu_count = num_cpus::get() as u32;
        let max_threads = std::cmp::min(cpu_count, 4);
        
        let wallet_address = chain_client.get_wallet_address();
        info!("Initializing REAL mobile mining engine for {}, max threads: {}", wallet_address, max_threads);
        
        Ok(Self {
            wallet_mnemonic,
            thermal_manager,
            chain_client,
            real_engine: None,
            is_mining: AtomicBool::new(false),
            thread_count: AtomicU32::new(0),
            solutions_found: AtomicU64::new(0),
            current_epoch: AtomicU64::new(0),
            current_difficulty: AtomicU32::new(0),
            rewards_earned: AtomicU64::new(0),
            start_time: RwLock::new(None),
            last_solution_time: RwLock::new(None),
            total_hashes: AtomicU64::new(0),
            max_threads,
            min_battery_level: 0.3,
            current_epoch_info: RwLock::new(None),
            nonce_range: RwLock::new(None),
        })
    }

    pub async fn start_mining(&mut self, requested_threads: u32) -> Result<()> {
        if self.is_mining.load(Ordering::Relaxed) {
            return Err(anyhow!("Mining already started"));
        }

        // Check thermal state before starting
        if self.thermal_manager.is_throttled().await {
            return Err(anyhow!("Device is thermally throttled"));
        }

        // Check battery level (mobile-specific)
        if !self.check_power_conditions().await {
            return Err(anyhow!("Power conditions not met for mining"));
        }

        // Fetch current epoch from chain
        let epoch_info = self.chain_client.get_current_epoch().await?;
        if !epoch_info.is_active {
            return Err(anyhow!("No active epoch for mining"));
        }
        
        // Store epoch info
        *self.current_epoch_info.write() = Some(epoch_info.clone());
        self.current_epoch.store(epoch_info.epoch_number, Ordering::Relaxed);
        self.current_difficulty.store(epoch_info.difficulty as u32, Ordering::Relaxed);
        
        // Calculate nonce range for this miner
        let nonce_range = self.chain_client.calculate_nonce_range(&self.wallet_address);
        *self.nonce_range.write() = Some(nonce_range.clone());
        
        // Parse target hash from hex string
        let target_hash_bytes = hex::decode(&epoch_info.target_hash)
            .map_err(|e| anyhow!("Invalid target hash: {}", e))?;
        let mut target_hash = [0u8; 32];
        target_hash.copy_from_slice(&target_hash_bytes[..32]);

        let thread_count = std::cmp::min(requested_threads, self.max_threads);
        self.thread_count.store(thread_count, Ordering::Relaxed);
        self.is_mining.store(true, Ordering::Relaxed);
        
        // Record start time
        *self.start_time.write() = Some(Instant::now());
        
        info!(
            "Starting REAL mobile mining for epoch {} with {} threads, difficulty: {}",
            epoch_info.epoch_number, thread_count, epoch_info.difficulty
        );
        
        // Create and start real mining engine
        let mut real_engine = RealMiningEngine::new(thread_count as usize);
        real_engine.start_mining(
            target_hash,
            epoch_info.difficulty,
            nonce_range.start,
            nonce_range.end,
        ).await?;
        
        self.real_engine = Some(real_engine);
        
        // Solution monitoring will be handled by Android service calling processSolutions()
        info!("Solution monitoring will be handled by periodic JNI calls from Android service");
        
        Ok(())
    }

    pub async fn stop_mining(&mut self) -> Result<()> {
        if !self.is_mining.load(Ordering::Relaxed) {
            return Ok(()); // Already stopped
        }

        info!("Stopping mobile mining");
        
        // Stop real mining engine
        if let Some(mut engine) = self.real_engine.take() {
            engine.stop_mining().await;
        }
        
        self.is_mining.store(false, Ordering::Relaxed);
        self.thread_count.store(0, Ordering::Relaxed);
        
        Ok(())
    }

    pub async fn get_stats(&self) -> MiningStats {
        let is_mining = self.is_mining.load(Ordering::Relaxed);
        let solutions = self.solutions_found.load(Ordering::Relaxed);
        let epoch = self.current_epoch.load(Ordering::Relaxed);
        let difficulty = self.current_difficulty.load(Ordering::Relaxed) as u8;
        let thermal_throttled = self.thermal_manager.is_throttled().await;
        let rewards = self.rewards_earned.load(Ordering::Relaxed);
        
        let uptime_seconds = if let Some(start) = *self.start_time.read() {
            start.elapsed().as_secs()
        } else {
            0
        };
        
        // Get real hashrate from engine
        let hashrate = if let Some(ref engine) = self.real_engine {
            engine.get_hashrate()
        } else {
            0.0
        };
        
        // Get total hashes
        let total_hashes = if let Some(ref engine) = self.real_engine {
            engine.get_total_hashes()
        } else {
            self.total_hashes.load(Ordering::Relaxed)
        };

        let last_solution_time = self.last_solution_time.read().as_ref().map(|dt| dt.to_rfc3339());

        MiningStats {
            hashrate,
            solutions_found: solutions,
            epoch,
            is_mining,
            thermal_throttled,
            uptime_seconds,
            last_solution_time,
            total_hashes,
            current_difficulty: difficulty,
            rewards_earned: rewards,
        }
    }

    pub async fn is_thermal_throttled(&self) -> bool {
        self.thermal_manager.is_throttled().await
    }

    async fn check_power_conditions(&self) -> bool {
        // Check battery level and charging status from Android
        let battery_ok = crate::BATTERY_STATE.with(|state| {
            if let Some(battery_status) = state.borrow().as_ref() {
                // Allow mining if:
                // 1. Battery >= min_battery_level AND charging, OR
                // 2. Battery >= min_battery_level + 10% buffer if not charging
                let min_level = if battery_status.is_charging {
                    self.min_battery_level
                } else {
                    self.min_battery_level + 0.1 // 10% buffer when not charging
                };
                
                let battery_percent = battery_status.level as f32 / 100.0;
                let battery_ok = battery_percent >= min_level;
                
                info!(
                    "Battery check: {}%, charging: {}, min_required: {:.1}%, ok: {}",
                    battery_status.level,
                    battery_status.is_charging,
                    min_level * 100.0,
                    battery_ok
                );
                
                battery_ok
            } else {
                // No battery data available - allow mining but warn
                warn!("No battery status available - allowing mining");
                true
            }
        });
        
        battery_ok
    }

    /// Check for and process any solutions from the mining engine
    pub async fn check_and_process_solutions(&mut self) -> Result<()> {
        if let Some(ref mut engine) = self.real_engine {
            // Check for solutions
            while let Some(solution) = engine.try_recv_solution() {
                info!(
                    "📋 SOLUTION FOUND! nonce={}, difficulty={}, hash_attempts={}",
                    solution.nonce, solution.difficulty, solution.hash_attempts
                );
                
                // Update statistics
                self.solutions_found.fetch_add(1, Ordering::Relaxed);
                *self.last_solution_time.write() = Some(chrono::Utc::now());
                
                // Get current epoch info
                let current_epoch = self.current_epoch_info.read().clone();
                if let Some(epoch) = current_epoch {
                    // Submit solution based on current epoch phase
                    match Self::submit_solution(&mut self.chain_client, &epoch, solution).await {
                        Ok(tx_hash) => {
                            info!("✅ Solution submitted successfully: {}", tx_hash);
                        }
                        Err(e) => {
                            error!("❌ Failed to submit solution: {}", e);
                            // Continue processing other solutions even if one fails
                        }
                    }
                } else {
                    warn!("No epoch info available - cannot submit solution");
                }
            }
        }
        
        Ok(())
    }
    
    /// Process a found solution (commit/reveal)
    pub async fn process_solution(&self, solution: MiningSolution) -> Result<()> {
        info!(
            "Processing REAL solution: nonce={}, difficulty={}, commitment={}",
            solution.nonce,
            solution.difficulty,
            hex::encode(solution.commitment_hash)
        );
        
        // Update statistics
        self.solutions_found.fetch_add(1, Ordering::Relaxed);
        *self.last_solution_time.write() = Some(Utc::now());
        
        // Get current epoch info
        let epoch_info = self.current_epoch_info.read().clone()
            .ok_or_else(|| anyhow!("No epoch info available"))?;
        
        // Check if we're in reveal window
        if epoch_info.is_in_reveal_window {
            // Submit reveal immediately
            self.chain_client.submit_reveal(
                epoch_info.epoch_number,
                solution.nonce,
                solution.digest,
            ).await?;
        } else {
            // Submit commitment
            self.chain_client.submit_commitment(
                epoch_info.epoch_number,
                solution.commitment_hash,
            ).await?;
        }
        
        Ok(())
    }
    
    /// Submit a solution to the blockchain (static method for use in monitoring task)
    async fn submit_solution(
        chain_client: &mut MobileChainClient, 
        epoch_info: &EpochInfo, 
        solution: MiningSolution
    ) -> Result<String> {
        // Check if we're in reveal window or commit window
        if epoch_info.is_in_reveal_window {
            info!("🔓 Submitting REVEAL for epoch {} (nonce: {})", 
                epoch_info.epoch_number, solution.nonce);
            
            // Submit reveal transaction
            chain_client.submit_reveal(
                epoch_info.epoch_number,
                solution.nonce,
                solution.digest,
            ).await
        } else {
            info!("🔒 Submitting COMMITMENT for epoch {} (commitment: {})", 
                epoch_info.epoch_number, hex::encode(solution.commitment_hash));
            
            // Submit commitment transaction
            chain_client.submit_commitment(
                epoch_info.epoch_number,
                solution.commitment_hash,
            ).await
        }
    }
}

/// Nonce partitioning logic for GMINE mobile mining
/// Each miner gets 1/1000th of nonce space per epoch to prevent conflicts

/// Calculate deterministic nonce range for a miner in a specific epoch
/// This MUST match the contract's calculate_nonce_range function exactly
pub fn calculate_nonce_range(miner_address: &str, epoch_number: u64) -> Result<(u64, u64)> {
    // Hash miner address + epoch to get deterministic partition
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
    
    // Rotate partitions each epoch to prevent grinding attacks
    let epoch_rotation = (epoch_number * 37) % 1000; // Prime number rotation
    let rotated_offset = partition_offset.wrapping_add(epoch_rotation * nonce_space);
    let max_nonce = rotated_offset.wrapping_add(nonce_space);
    
    info!("Blake2b512 nonce range for epoch {} miner {}: {} to {}", 
        epoch_number, miner_address, rotated_offset, max_nonce);
    
    Ok((rotated_offset, max_nonce))
}

/// Calculate mobile-optimized chunk size for efficient mining
pub fn get_mobile_chunk_size() -> u64 {
    // Mobile devices have limited CPU, use smaller chunks than desktop
    10_000
}

/// Calculate next nonce chunk for mining progress
pub fn get_next_nonce_chunk(
    current_nonce: u64,
    chunk_size: u64,
    miner_address: &str,
    epoch_number: u64
) -> Result<(u64, u64)> {
    let (start_nonce, end_nonce) = calculate_nonce_range(miner_address, epoch_number)?;
    
    let chunk_start = current_nonce;
    let chunk_end = (current_nonce + chunk_size).min(end_nonce);
    
    // Wrap around if we've exhausted our range
    let (final_start, final_end) = if chunk_start >= end_nonce {
        (start_nonce, (start_nonce + chunk_size).min(end_nonce))
    } else {
        (chunk_start, chunk_end)
    };
    
    debug!("Next nonce chunk for epoch {}: {} to {}", 
        epoch_number, final_start, final_end);
    
    Ok((final_start, final_end))
}