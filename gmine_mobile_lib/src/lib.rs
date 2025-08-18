use jni::JNIEnv;
use jni::objects::{JClass, JString, JObject};
use jni::sys::{jboolean, jint, jlong, jstring, jdouble, JNI_VERSION_1_6};
use std::sync::Mutex;
use blake2::{Blake2b512, Digest};
use drillx;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::collections::VecDeque;
use serde_json::json;

pub mod wallet;
pub mod mobile_wallet;
// pub mod mobile_tx_builder; // Commented out - has dependency issues
pub mod types;
pub mod blockchain;
pub mod eip712;
pub mod web3_extension;
pub mod transaction;
pub mod tx_proto;
// pub mod wasmx;  // Using msg_execute_contract_compat instead
pub mod msg_execute_contract_compat;

#[cfg(test)]
mod test_eip712;

#[cfg(test)]
mod test_transaction;

#[cfg(test)]
mod test_real_account;

#[cfg(test)]
mod test_transaction_debug;

#[cfg(test)]
mod test_tx_proto;

#[cfg(test)]
mod test_proto_debug;

use crate::mobile_wallet::MobileWallet as Wallet;
use crate::types::*;
use crate::blockchain::BlockchainClient;
use crate::eip712::Eip712Signer;

// Activity log entry
#[derive(serde::Serialize, Clone)]
struct ActivityLog {
    timestamp: u64,
    level: String,
    message: String,
    worker: Option<u32>,
    difficulty: Option<u8>,
    nonce: Option<String>,
    hashrate: Option<u64>,
}

// Battery information from Android
#[derive(Debug, Clone, serde::Serialize)]
struct BatteryInfo {
    is_charging: bool,
    level: f32,
}

// Thermal information from Android
#[derive(Debug, Clone, serde::Serialize)]
struct ThermalInfo {
    temperature: f32,
    is_throttled: bool,
}

// Enhanced mining state with real blockchain integration
static MINING_STATE: Mutex<Option<MiningState>> = Mutex::new(None);

struct MiningState {
    is_mining: Arc<AtomicBool>,
    solutions_found: Arc<AtomicU64>,
    hashrate: Arc<AtomicU64>,
    epoch: u64,
    wallet: Wallet,
    blockchain_client: BlockchainClient,
    signer: Eip712Signer,
    threads: Vec<thread::JoinHandle<()>>,
    current_challenge: Option<MiningChallenge>,
    pending_solutions: Arc<Mutex<VecDeque<Solution>>>,
    start_time: Instant,
    last_commit_hash: Option<String>,
    activity_logs: Arc<Mutex<VecDeque<ActivityLog>>>,
    battery_info: Option<BatteryInfo>,
    thermal_info: Option<ThermalInfo>,
}

// Called when the library is loaded
#[no_mangle]
pub extern "system" fn JNI_OnLoad(_vm: jni::JavaVM, _: *mut std::os::raw::c_void) -> jint {
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("GMINE")
            .with_max_level(log::LevelFilter::Debug),
    );
    log::info!("GMINE Mobile native library loaded with REAL mining");
    JNI_VERSION_1_6
}

// Initialize with mnemonic
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_initialize(
    mut env: JNIEnv,
    _class: JClass,
    mnemonic: JString,
) -> jboolean {
    let mnemonic_str: String = match env.get_string(&mnemonic) {
        Ok(s) => s.into(),
        Err(_) => {
            log::error!("Failed to get mnemonic string from JNI");
            return 0;
        }
    };
    
    log::info!("MiningEngine::initializeNative called with REAL blockchain integration");
    
    // Create real wallet from mnemonic
    let wallet = match Wallet::from_mnemonic_no_passphrase(&mnemonic_str) {
        Ok(w) => w,
        Err(e) => {
            log::error!("Failed to create wallet: {}", e);
            return 0;
        }
    };
    
    log::info!("Wallet address: {}", wallet.address);
    
    // Create blockchain client
    let blockchain_client = BlockchainClient::new();
    
    // Create EIP-712 signer with compressed public key (33 bytes)
    let compressed_key = match wallet.public_key_compressed() {
        Ok(key) => key,
        Err(e) => {
            log::error!("Failed to get compressed public key: {}", e);
            return 0;
        }
    };
    let signer = match Eip712Signer::new(wallet.private_key_bytes(), &compressed_key) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to create signer: {}", e);
            return 0;
        }
    };
    
    match MINING_STATE.lock() {
        Ok(mut state) => {
            *state = Some(MiningState {
        is_mining: Arc::new(AtomicBool::new(false)),
        solutions_found: Arc::new(AtomicU64::new(0)),
        hashrate: Arc::new(AtomicU64::new(0)),
        epoch: 0, // Will be updated from blockchain
        wallet,
        blockchain_client,
        signer,
        threads: Vec::new(),
        current_challenge: None,
        pending_solutions: Arc::new(Mutex::new(VecDeque::new())),
        start_time: Instant::now(),
        last_commit_hash: None,
        activity_logs: Arc::new(Mutex::new(VecDeque::new())),
        battery_info: None,
        thermal_info: None,
    });
            1 // true
        }
        Err(e) => {
            log::error!("Failed to acquire MINING_STATE lock: {:?}", e);
            0 // false
        }
    }
}

// Helper function to add activity log - safe version that doesn't deadlock
fn add_activity_log_direct(activity_logs: &Arc<Mutex<VecDeque<ActivityLog>>>, level: &str, message: String, worker: Option<u32>, difficulty: Option<u8>, nonce: Option<u64>) {
    if let Ok(mut logs) = activity_logs.lock() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        
        let log = ActivityLog {
            timestamp,
            level: level.to_string(),
            message,
            worker,
            difficulty,
            nonce: nonce.map(|n| format!("{}", n)),
            hashrate: None,
        };
        
        logs.push_front(log);
        
        // Keep only last 100 logs
        while logs.len() > 100 {
            logs.pop_back();
        }
    }
}

// Helper function to add activity log
fn add_activity_log(level: &str, message: String, worker: Option<u32>, difficulty: Option<u8>, nonce: Option<u64>) {
    if let Ok(state) = MINING_STATE.lock() {
        if let Some(mining_state) = state.as_ref() {
            add_activity_log_direct(&mining_state.activity_logs, level, message, worker, difficulty, nonce);
        }
    } else {
        log::error!("Failed to acquire MINING_STATE lock for logging");
    }
}

// Start REAL mining
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_startMining(
    _env: JNIEnv,
    _class: JClass,
    thread_count: jint,
) -> jboolean {
    log::info!("Starting REAL mining with {} threads", thread_count);
    
    match MINING_STATE.lock() {
        Ok(mut state) => {
            if let Some(mining_state) = state.as_mut() {
        if mining_state.is_mining.load(Ordering::Relaxed) {
            log::warn!("Mining already running");
            return 0;
        }
        
        // Fetch current challenge from blockchain
        log::info!("Fetching mining challenge for wallet: {}", mining_state.wallet.address);
        match mining_state.blockchain_client.get_mining_challenge(&mining_state.wallet.address) {
            Ok(challenge) => {
                log::info!("Successfully got challenge for epoch {}, difficulty: {}, nonce range: {}-{}", 
                    challenge.epoch, challenge.difficulty, challenge.nonce_start, challenge.nonce_end);
                
                // Add to activity log - use direct version since we're holding the lock
                add_activity_log_direct(
                    &mining_state.activity_logs,
                    "info",
                    format!("Mining started for epoch {} (difficulty: {})", challenge.epoch, challenge.difficulty),
                    None,
                    Some(challenge.difficulty),
                    None
                );
                
                mining_state.current_challenge = Some(challenge.clone());
                mining_state.epoch = challenge.epoch;
                
                mining_state.is_mining.store(true, Ordering::Relaxed);
                
                log::info!("About to create {} mining threads", thread_count);
                
                // Create real mining threads with blockchain challenge
                for i in 0..thread_count {
                    let is_mining = mining_state.is_mining.clone();
                    let solutions_found = mining_state.solutions_found.clone();
                    let hashrate = mining_state.hashrate.clone();
                    // CRITICAL FIX: Use the SAME Arc reference that MINING_STATE holds
                    // This ensures JNI calls see the same queue instance
                    let pending_solutions = Arc::clone(&mining_state.pending_solutions);
                    let activity_logs = mining_state.activity_logs.clone();
                    let challenge = challenge.clone();
                    
                    let handle = thread::spawn(move || {
                        mine_worker(
                            i as usize, 
                            is_mining, 
                            solutions_found, 
                            hashrate,
                            pending_solutions,
                            activity_logs,
                            challenge
                        );
                    });
                    
                    mining_state.threads.push(handle);
                }
                
                log::info!("All {} mining threads created, returning from startMining", thread_count);
                return 1;
            }
            Err(e) => {
                log::error!("Failed to get mining challenge from blockchain: {:?}", e);
                log::error!("Wallet address: {}", mining_state.wallet.address);
                log::error!("This likely means the blockchain connection failed");
                return 0;
            }
            }
            } else {
                0 // false
            }
        }
        Err(e) => {
            log::error!("Failed to acquire MINING_STATE lock: {:?}", e);
            0 // false
        }
    }
}

// Real mining worker
fn mine_worker(
    id: usize,
    is_mining: Arc<AtomicBool>,
    solutions_found: Arc<AtomicU64>,
    hashrate: Arc<AtomicU64>,
    pending_solutions: Arc<Mutex<VecDeque<Solution>>>,
    activity_logs: Arc<Mutex<VecDeque<ActivityLog>>>,
    challenge: MiningChallenge,
) {
    log::info!("Mining worker {} started for epoch {}", id, challenge.epoch);
    
    // Log worker start
    if let Ok(mut logs) = activity_logs.lock() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        
        logs.push_front(ActivityLog {
            timestamp,
            level: "info".to_string(),
            message: format!("Worker {} started", id),
            worker: Some(id as u32),
            difficulty: None,
            nonce: None,
            hashrate: None,
        });
    }
    
    // Start from assigned nonce range
    let mut nonce = challenge.nonce_start + (id as u64 * 1000);
    let mut hashes = 0u64;
    let mut last_update = Instant::now();
    
    // Pre-allocate memory for equix
    let mut solver_memory = equix::SolverMemory::new();
    
    while is_mining.load(Ordering::Relaxed) && nonce <= challenge.nonce_end {
        let nonce_bytes = nonce.to_le_bytes();
        
        // Log mining progress every 10000 hashes
        if hashes % 10000 == 0 && hashes > 0 {
            log::info!(
                "Worker {}: Mining epoch {} | Nonce: {} | Hashes: {} | Looking for difficulty >= {}",
                id, challenge.epoch, nonce, hashes, challenge.difficulty
            );
        }
        
        // REAL MINING: Use drillx to generate hash
        match drillx::hash_with_memory(&mut solver_memory, &challenge.challenge, &nonce_bytes) {
            Ok(hash) => {
                let hash_difficulty = hash.difficulty() as u8;
                
                if hash_difficulty >= challenge.difficulty {
                    log::info!(
                        "⛏️ SOLUTION FOUND! Worker {} | Nonce: {} | Difficulty: {} | Epoch: {}",
                        id, nonce, hash_difficulty, challenge.epoch
                    );
                    
                    // Log solution to activity feed
                    if let Ok(mut logs) = activity_logs.lock() {
                        let timestamp = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        
                        logs.push_front(ActivityLog {
                            timestamp,
                            level: "solution".to_string(),
                            message: format!("Solution found! Difficulty: {}", hash_difficulty),
                            worker: Some(id as u32),
                            difficulty: Some(hash_difficulty),
                            nonce: Some(nonce.to_string()),
                            hashrate: None,
                        });
                    }
                    
                    // Store solution for submission
                    let solution = Solution {
                        nonce,
                        hash: hash.h.to_vec(),  // drillx Hash has h field for the hash bytes
                        difficulty: hash_difficulty,
                        epoch: challenge.epoch,
                    };
                    
                    if let Ok(mut solutions) = pending_solutions.lock() {
                        let queue_size_before = solutions.len();
                        solutions.push_back(solution);
                        log::info!("Added solution to queue. Queue size: {} -> {}", queue_size_before, solutions.len());
                    } else {
                        log::error!("Failed to lock pending_solutions queue in worker!");
                    }
                    
                    let new_count = solutions_found.fetch_add(1, Ordering::Relaxed) + 1;
                    log::info!("✅ Solution found! Total solutions: {}", new_count);
                }
            }
            Err(e) => {
                log::error!("Mining error: {:?}", e);
            }
        }
        
        nonce += 1;
        hashes += 1;
        
        // Update hashrate every second
        if last_update.elapsed() >= Duration::from_secs(1) {
            let thread_count = thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4) as u64;
            let current_hashrate = hashes * thread_count;
            hashrate.store(current_hashrate, Ordering::Relaxed);
            
            // Log hashrate like a real miner
            log::info!(
                "Worker {} | Hashrate: {} H/s | Epoch: {} | Range: {}-{}",
                id, current_hashrate, challenge.epoch, challenge.nonce_start, challenge.nonce_end
            );
            
            // Log hashrate to activity feed
            if let Ok(mut logs) = activity_logs.lock() {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                
                logs.push_front(ActivityLog {
                    timestamp,
                    level: "info".to_string(),
                    message: format!("Hashrate: {} H/s", current_hashrate),
                    worker: Some(id as u32),
                    difficulty: None,
                    nonce: None,
                    hashrate: Some(current_hashrate),
                });
            }
            
            last_update = Instant::now();
            hashes = 0;
        }
    }
    
    log::info!("Mining worker {} stopped", id);
}

// Stop mining
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_stopMining(
    _env: JNIEnv,
    _class: JClass,
) {
    log::info!("Stopping mining");
    
    if let Ok(mut state) = MINING_STATE.lock() {
        if let Some(mining_state) = state.as_mut() {
        mining_state.is_mining.store(false, Ordering::Relaxed);
        
        // Wait for threads to finish
        while let Some(handle) = mining_state.threads.pop() {
            let _ = handle.join();
        }
        }
    } else {
        log::error!("Failed to acquire MINING_STATE lock to stop mining");
    }
}

// Get mining stats
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_getMiningStats(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let stats = match MINING_STATE.lock() {
        Ok(state) => {
            if let Some(mining_state) = state.as_ref() {
        let is_mining = mining_state.is_mining.load(Ordering::Relaxed);
        let hashrate = mining_state.hashrate.load(Ordering::Relaxed);
        let solutions = mining_state.solutions_found.load(Ordering::Relaxed);
        let uptime = mining_state.start_time.elapsed().as_secs();
        
        log::info!("getMiningStats: solutions_found = {}, is_mining = {}, hashrate = {}", 
                   solutions, is_mining, hashrate);
        
        MiningStats {
            is_mining,
            hashrate,
            solutions_found: solutions,
            uptime_seconds: uptime,
            epoch: mining_state.epoch,
            last_solution_time: None,
            real_mining: true,
            wallet_address: Some(mining_state.wallet.address.clone()),
            current_challenge: mining_state.current_challenge.as_ref()
                .map(|c| hex::encode(&c.challenge)),
            difficulty: mining_state.current_challenge.as_ref()
                .map(|c| c.difficulty),
        }
            } else {
                MiningStats {
            is_mining: false,
            hashrate: 0,
            solutions_found: 0,
            uptime_seconds: 0,
            epoch: 0,
            last_solution_time: None,
            real_mining: true,
            wallet_address: None,
            current_challenge: None,
            difficulty: None,
        }
            }
        }
        Err(e) => {
            log::error!("Failed to acquire MINING_STATE lock: {:?}", e);
            MiningStats {
                is_mining: false,
                hashrate: 0,
                solutions_found: 0,
                uptime_seconds: 0,
                epoch: 0,
                last_solution_time: None,
                real_mining: true,
                wallet_address: None,
                current_challenge: None,
                difficulty: None,
            }
        }
    };
    
    let stats_json = serde_json::to_string(&stats).unwrap_or_else(|_| "{}".to_string());
    
    match env.new_string(stats_json) {
        Ok(jstr) => jstr.into_raw(),
        Err(e) => {
            log::error!("Failed to create Java string: {:?}", e);
            std::ptr::null_mut()
        }
    }
}

// Get activity logs
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_getActivityLogs(
    env: JNIEnv,
    _class: JClass,
    max_count: jint,
) -> jstring {
    let logs: Vec<ActivityLog> = match MINING_STATE.lock() {
        Ok(state) => {
            if let Some(mining_state) = state.as_ref() {
        if let Ok(activity_logs) = mining_state.activity_logs.lock() {
            activity_logs.iter()
                .take(max_count as usize)
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
            } else {
                Vec::new()
            }
        }
        Err(e) => {
            log::error!("Failed to acquire MINING_STATE lock: {:?}", e);
            Vec::new()
        }
    };
    
    let logs_json = serde_json::to_string(&logs).unwrap_or_else(|_| "[]".to_string());
    
    match env.new_string(logs_json) {
        Ok(jstr) => jstr.into_raw(),
        Err(e) => {
            log::error!("Failed to create Java string: {:?}", e);
            std::ptr::null_mut()
        }
    }
}

// Process solutions - submit to blockchain
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_processMiningSolutions(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    log::info!("processMiningSolutions JNI called");
    let solutions_json = process_pending_solutions();
    
    match env.new_string(solutions_json) {
        Ok(jstr) => jstr.into_raw(),
        Err(e) => {
            log::error!("Failed to create Java string: {:?}", e);
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_processSolutions(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    let _ = process_pending_solutions(); // Ignore return value for backward compatibility
    1 // true
}


// Internal function to process and submit solutions
fn process_pending_solutions() -> String {
    log::info!("process_pending_solutions called");
    match MINING_STATE.lock() {
        Ok(mut state) => {
            if let Some(mining_state) = state.as_mut() {
        // Get pending solutions
        let solutions_to_process = {
            let mut solutions = match mining_state.pending_solutions.lock() {
                Ok(s) => s,
                Err(_) => {
                    log::error!("Failed to lock pending_solutions");
                    return "[]".to_string();
                },
            };
            
            log::info!("Pending solutions queue size: {}", solutions.len());
            
            let mut to_process = Vec::new();
            while let Some(solution) = solutions.pop_front() {
                to_process.push(solution);
                if to_process.len() >= 5 { // Process up to 5 at a time
                    break;
                }
            }
            to_process
        };
        
        // Create JSON array of processed solutions
        let mut processed_solutions = Vec::new();
        
        log::info!("Processing {} solutions", solutions_to_process.len());
        
        // Process each solution
        for (idx, solution) in solutions_to_process.iter().enumerate() {
            log::info!("Processing solution {}/{} for epoch {} with nonce {}", 
                      idx + 1, solutions_to_process.len(), solution.epoch, solution.nonce);
            
            // For now, we'll log the submission
            // In a full implementation, this would:
            // 1. Create commitment hash
            // 2. Submit commit transaction
            // 3. Wait for commit phase
            // 4. Submit reveal transaction
            
            // Create commitment - must match contract's format:
            // Blake2b512(wallet_address || nonce_bytes || hash_digest || epoch_bytes)
            let mut hasher = Blake2b512::new();
            hasher.update(&mining_state.wallet.address.as_bytes());
            hasher.update(&solution.nonce.to_le_bytes());
            hasher.update(&solution.hash); // The drillx hash digest
            hasher.update(&solution.epoch.to_le_bytes());
            let commitment = hasher.finalize();
            let commitment_hex = hex::encode(&commitment);
            
            log::info!("Submitting commitment: {} for nonce: {}", commitment_hex, solution.nonce);
            
            // Submit via EIP-712 signed transaction
            let msg_data = json!({
                "commitment": commitment_hex.clone()
            });
            
            match mining_state.blockchain_client.get_account_info(&mining_state.wallet.address) {
                Ok((account_number, sequence)) => {
                    match mining_state.signer.sign_transaction("commit", &msg_data, &mining_state.wallet.address, account_number, sequence, None, "") {
                        Ok(signing_result) => {
                            if let Some(signature) = signing_result.signature {
                                if let Some(pub_key) = signing_result.pub_key {
                                    // Use the new submit_commitment method that constructs full transaction
                                    match mining_state.blockchain_client.submit_commitment(
                                        &commitment_hex,
                                        &mining_state.wallet.address,
                                        &signature,
                                        &pub_key,
                                        account_number,
                                        sequence
                                    ) {
                                        Ok(tx_hash) => {
                                            log::info!("✅ Commitment submitted! TX: {}", tx_hash);
                                            // Store for later reveal
                                            mining_state.last_commit_hash = Some(commitment_hex.clone());
                                            
                                            // Add success log to activity
                                            if let Ok(mut logs) = mining_state.activity_logs.lock() {
                                                logs.push_front(ActivityLog {
                                                    timestamp: SystemTime::now()
                                                        .duration_since(UNIX_EPOCH)
                                                        .unwrap_or_default()
                                                        .as_millis() as u64,
                                                    level: "success".to_string(),
                                                    message: format!("Commitment submitted! TX: {}", &tx_hash[..8]),
                                                    worker: None,
                                                    difficulty: None,
                                                    nonce: None,
                                                    hashrate: None,
                                                });
                                            }
                                        }
                                        Err(e) => {
                                            log::error!("Failed to submit commitment: {:?}", e);
                                            
                                            // Add error log to activity feed
                                            if let Ok(mut logs) = mining_state.activity_logs.lock() {
                                                logs.push_front(ActivityLog {
                                                    timestamp: SystemTime::now()
                                                        .duration_since(UNIX_EPOCH)
                                                        .unwrap_or_default()
                                                        .as_millis() as u64,
                                                    level: "error".to_string(),
                                                    message: format!("❌ Blockchain submission failed: {}", e),
                                                    worker: None,
                                                    difficulty: Some(solution.difficulty),
                                                    nonce: Some(format!("{:x}", solution.nonce)),
                                                    hashrate: None,
                                                });
                                            }
                                        }
                                    }
                                } else {
                                    log::error!("No public key in signing result");
                                    
                                    // Add error log to activity feed
                                    if let Ok(mut logs) = mining_state.activity_logs.lock() {
                                        logs.push_front(ActivityLog {
                                            timestamp: SystemTime::now()
                                                .duration_since(UNIX_EPOCH)
                                                .unwrap_or_default()
                                                .as_millis() as u64,
                                            level: "error".to_string(),
                                            message: "❌ Signing error: No public key returned".to_string(),
                                            worker: None,
                                            difficulty: Some(solution.difficulty),
                                            nonce: Some(format!("{:x}", solution.nonce)),
                                            hashrate: None,
                                        });
                                    }
                                }
                            } else {
                                log::error!("No signature in signing result");
                                
                                // Add error log to activity feed
                                if let Ok(mut logs) = mining_state.activity_logs.lock() {
                                    logs.push_front(ActivityLog {
                                        timestamp: SystemTime::now()
                                            .duration_since(UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis() as u64,
                                        level: "error".to_string(),
                                        message: "❌ Signing error: No signature returned".to_string(),
                                        worker: None,
                                        difficulty: Some(solution.difficulty),
                                        nonce: Some(format!("{:x}", solution.nonce)),
                                        hashrate: None,
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to sign commitment: {:?}", e);
                            
                            // Add error log to activity feed
                            if let Ok(mut logs) = mining_state.activity_logs.lock() {
                                logs.push_front(ActivityLog {
                                    timestamp: SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as u64,
                                    level: "error".to_string(),
                                    message: format!("❌ Failed to sign transaction: {}", e),
                                    worker: None,
                                    difficulty: Some(solution.difficulty),
                                    nonce: Some(format!("{:x}", solution.nonce)),
                                    hashrate: None,
                                });
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to get account info: {:?}", e);
                    
                    // Add error log to activity feed
                    if let Ok(mut logs) = mining_state.activity_logs.lock() {
                        logs.push_front(ActivityLog {
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64,
                            level: "error".to_string(),
                            message: format!("❌ Failed to get account info: {}", e),
                            worker: None,
                            difficulty: Some(solution.difficulty),
                            nonce: Some(format!("{:x}", solution.nonce)),
                            hashrate: None,
                        });
                    }
                }
            }
            
            // Add to processed solutions
            processed_solutions.push(serde_json::json!({
                "nonce": solution.nonce,
                "difficulty": solution.difficulty,
                "epoch": solution.epoch,
                "commitment": commitment_hex,
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
            }));
        }
        
        // Return JSON string of processed solutions
        serde_json::to_string(&processed_solutions).unwrap_or_else(|_| "[]".to_string())
            } else {
                "[]".to_string()
            }
        }
        Err(e) => {
            log::error!("Failed to acquire MINING_STATE lock: {:?}", e);
            "[]".to_string()
        }
    }
}

// Cleanup
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_cleanup(
    _env: JNIEnv,
    _class: JClass,
) {
    log::info!("cleanup called");
    if let Ok(mut state) = MINING_STATE.lock() {
        *state = None;
    } else {
        log::error!("Failed to acquire MINING_STATE lock for cleanup");
    }
}

// Battery state update from Android
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_updateBatteryState(
    _env: JNIEnv,
    _class: JClass,
    battery_level: jint,
    is_charging: jboolean,
) {
    let new_info = BatteryInfo {
        is_charging: is_charging != 0,
        level: battery_level as f32,
    };
    
    if let Ok(mut state) = MINING_STATE.lock() {
        if let Some(mining_state) = state.as_mut() {
            mining_state.battery_info = Some(new_info.clone());
            log::debug!("Battery status updated: level={}%, charging={}", new_info.level, new_info.is_charging);
        }
    } else {
        log::error!("updateBatteryStatus: Failed to acquire MINING_STATE lock");
    }
}

#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_sendTelemetry(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    log::debug!("sendTelemetry called");
    1 // true
}

#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_mining_MiningEngine_isThermalThrottled(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    if let Ok(state) = MINING_STATE.lock() {
        if let Some(mining_state) = state.as_ref() {
            if let Some(thermal_info) = &mining_state.thermal_info {
                return if thermal_info.is_throttled { 1 } else { 0 };
            }
        }
    }
    0 // default to not throttled if we can't get the state
}

// Thermal state update from Android thermal manager
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_thermal_AndroidThermalManager_00024Companion_updateThermalState(
    _env: JNIEnv,
    _class: JClass,
    temperature: f64,
    is_throttled: jboolean,
) {
    let new_info = ThermalInfo {
        temperature: temperature as f32,
        is_throttled: is_throttled != 0,
    };
    
    if let Ok(mut state) = MINING_STATE.lock() {
        if let Some(mining_state) = state.as_mut() {
            mining_state.thermal_info = Some(new_info.clone());
            log::debug!("Thermal state updated: temp={}°C, throttled={}", new_info.temperature, new_info.is_throttled);
            
            // Log warning if thermal throttling is active
            if new_info.is_throttled {
                log::warn!("Device is thermally throttled at {}°C", new_info.temperature);
            }
        }
    } else {
        log::error!("updateThermalState: Failed to acquire MINING_STATE lock");
    }
}

// Bridge manager functions
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_bridge_BridgeManager_nativeStartBridgeService(
    _env: JNIEnv,
    _class: JClass,
    _context: JObject,
    _mnemonic: JString,
) -> jboolean {
    log::info!("nativeStartBridgeService called");
    1 // true
}

#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_bridge_BridgeManager_nativeStopBridgeService(
    _env: JNIEnv,
    _class: JClass,
    _context: JObject,
) -> jboolean {
    log::info!("nativeStopBridgeService called");
    1 // true
}

#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_bridge_BridgeManager_checkBridgeStatus(
    _env: JNIEnv,
    _class: JClass,
    _context: JObject,
) -> jboolean {
    1 // true - always running
}

// Wallet manager functions
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_wallet_WalletManager_generateMnemonic(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    match Wallet::generate_mnemonic() {
        Ok(mnemonic) => {
            log::info!("Generated new mnemonic");
            match env.new_string(mnemonic) {
                Ok(jstr) => jstr.into_raw(),
                Err(e) => {
                    log::error!("Failed to create Java string: {:?}", e);
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            log::error!("Failed to generate mnemonic: {}", e);
            match env.new_string("") {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            }
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_wallet_WalletManager_validateMnemonic(
    mut env: JNIEnv,
    _class: JClass,
    mnemonic: JString,
) -> jboolean {
    let mnemonic_str: String = match env.get_string(&mnemonic) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };
    
    if Wallet::validate_mnemonic(&mnemonic_str) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_wallet_WalletManager_deriveAddress(
    mut env: JNIEnv,
    _class: JClass,
    mnemonic: JString,
) -> jstring {
    let mnemonic_str: String = match env.get_string(&mnemonic) {
        Ok(s) => s.into(),
        Err(_) => {
            return env.new_string("")
                .expect("Couldn't create java string!")
                .into_raw();
        }
    };
    
    match Wallet::from_mnemonic_no_passphrase(&mnemonic_str) {
        Ok(wallet) => {
            log::info!("Derived real address: {}", wallet.address);
            match env.new_string(&wallet.address) {
                Ok(jstr) => jstr.into_raw(),
                Err(e) => {
                    log::error!("Failed to create Java string: {:?}", e);
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            log::error!("Failed to derive address: {}", e);
            match env.new_string("") {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            }
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_wallet_WalletManager_deriveBech32Address(
    env: JNIEnv,
    _class: JClass,
    mnemonic: JString,
) -> jstring {
    // Same as deriveAddress - both return bech32 format
    Java_io_gelotto_gmine_wallet_WalletManager_deriveAddress(env, _class, mnemonic)
}

// EIP-712 signing function - NEW PROPER IMPLEMENTATION
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_bridge_EipBridgeService_signTransactionNativeV2(
    mut env: JNIEnv,
    _class: JClass,
    mnemonic: JString,
    msg_type: JString,
    msg_data_json: JString,
    account_number: jlong,
    sequence: jlong,
    _chain_id: JString,
    fee_json: JString,
    memo: JString,
) -> jstring {
    // Parse all parameters
    let mnemonic_str: String = match env.get_string(&mnemonic) {
        Ok(s) => s.into(),
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Invalid mnemonic: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    let msg_type_str: String = match env.get_string(&msg_type) {
        Ok(s) => s.into(),
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Invalid msg_type: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    let msg_data_str: String = match env.get_string(&msg_data_json) {
        Ok(s) => s.into(),
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Invalid msg_data: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    let fee_str: String = match env.get_string(&fee_json) {
        Ok(s) => s.into(),
        Err(_) => String::new(),
    };
    
    let memo_str: String = match env.get_string(&memo) {
        Ok(s) => s.into(),
        Err(_) => String::new(),
    };
    
    // Create wallet from mnemonic
    let wallet = match Wallet::from_mnemonic_no_passphrase(&mnemonic_str) {
        Ok(w) => w,
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Invalid mnemonic: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    // Create proper EIP-712 signer with compressed public key
    let compressed_key = match wallet.public_key_compressed() {
        Ok(key) => key,
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Failed to get compressed public key: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    let signer = match crate::eip712::Eip712Signer::new(wallet.private_key_bytes(), &compressed_key) {
        Ok(s) => s,
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Failed to create signer: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    // Parse message data
    let msg_data: serde_json::Value = match serde_json::from_str(&msg_data_str) {
        Ok(d) => d,
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Invalid JSON in msg_data: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    // Parse fee if provided
    let fee = if !fee_str.is_empty() {
        match serde_json::from_str::<Fee>(&fee_str) {
            Ok(f) => Some(f),
            Err(_) => None,
        }
    } else {
        None
    };
    
    // Sign the transaction with sender address
    match signer.sign_transaction(
        &msg_type_str,
        &msg_data,
        &wallet.address,
        account_number as u64,
        sequence as u64,
        fee,
        &memo_str,
    ) {
        Ok(result) => {
            let json = serde_json::to_string(&result).unwrap_or_else(|_| 
                r#"{"success":false,"error":"Failed to serialize result"}"#.to_string()
            );
            match env.new_string(json) {
                Ok(jstr) => jstr.into_raw(),
                Err(e) => {
                    log::error!("Failed to create Java string: {:?}", e);
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Signing failed: {}"}}"#, e);
            match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            }
        }
    }
}

// EIP-712 signing function - ORIGINAL BROKEN implementation (kept for compatibility)
#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_bridge_EipBridgeService_signTransactionNative(
    mut env: JNIEnv,
    _class: JClass,
    mnemonic: JString,
    msg_type: JString,
    msg_data_json: JString,
    account_number: jlong,
    sequence: jlong,
    _chain_id: JString,
    fee_json: JString,
    memo: JString,
) -> jstring {
    // Parse all parameters
    let mnemonic_str: String = match env.get_string(&mnemonic) {
        Ok(s) => s.into(),
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Invalid mnemonic: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    let msg_type_str: String = match env.get_string(&msg_type) {
        Ok(s) => s.into(),
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Invalid msg_type: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    let msg_data_str: String = match env.get_string(&msg_data_json) {
        Ok(s) => s.into(),
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Invalid msg_data: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    let fee_str: String = match env.get_string(&fee_json) {
        Ok(s) => s.into(),
        Err(_) => "".to_string(), // Optional
    };
    
    let memo_str: String = match env.get_string(&memo) {
        Ok(s) => s.into(),
        Err(_) => "".to_string(), // Optional
    };
    
    // Create wallet from mnemonic
    let wallet = match Wallet::from_mnemonic_no_passphrase(&mnemonic_str) {
        Ok(w) => w,
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Failed to create wallet: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    // Create signer with compressed public key
    let compressed_key = match wallet.public_key_compressed() {
        Ok(key) => key,
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Failed to get compressed public key: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    let signer = match Eip712Signer::new(wallet.private_key_bytes(), &compressed_key) {
        Ok(s) => s,
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Failed to create signer: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    // Parse message data
    let msg_data: serde_json::Value = match serde_json::from_str(&msg_data_str) {
        Ok(d) => d,
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Invalid JSON in msg_data: {}"}}"#, e);
            return match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            };
        }
    };
    
    // Parse fee if provided
    let fee = if !fee_str.is_empty() {
        match serde_json::from_str::<Fee>(&fee_str) {
            Ok(f) => Some(f),
            Err(_) => None,
        }
    } else {
        None
    };
    
    // Sign the transaction with sender address
    match signer.sign_transaction(
        &msg_type_str,
        &msg_data,
        &wallet.address,
        account_number as u64,
        sequence as u64,
        fee,
        &memo_str,
    ) {
        Ok(result) => {
            let json = serde_json::to_string(&result).unwrap_or_else(|_| 
                r#"{"success":false,"error":"Failed to serialize result"}"#.to_string()
            );
            match env.new_string(json) {
                Ok(jstr) => jstr.into_raw(),
                Err(e) => {
                    log::error!("Failed to create Java string: {:?}", e);
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            let err = format!(r#"{{"success":false,"error":"Signing failed: {}"}}"#, e);
            match env.new_string(err) {
                Ok(jstr) => jstr.into_raw(),
                Err(_) => std::ptr::null_mut()
            }
        }
    }
}

// Thread-local storage for thermal state updates from Android
thread_local! {
    static THERMAL_STATE: std::cell::RefCell<Option<ThermalUpdate>> = std::cell::RefCell::new(None);
}

struct ThermalUpdate {
    temperature: f32,
    is_critical: bool,
}

#[no_mangle]
pub extern "system" fn Java_io_gelotto_gmine_thermal_AndroidThermalManager_updateThermalState(
    _env: JNIEnv,
    _class: JClass,
    temperature: jdouble,
    is_critical: jboolean,
) {
    // Update thread-local thermal state
    THERMAL_STATE.with(|state| {
        *state.borrow_mut() = Some(ThermalUpdate {
            temperature: temperature as f32,
            is_critical: is_critical != 0,
        });
    });
    
    log::debug!("Updated thermal state: {:.1}°C, critical: {}", temperature, is_critical != 0);
}