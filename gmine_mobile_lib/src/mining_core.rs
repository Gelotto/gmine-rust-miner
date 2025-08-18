// rust/src/mining_core.rs

use blake2::{Blake2b512, Digest};
use drillx;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::Duration;

// Our own Solution struct for internal use
#[derive(Debug, Clone)]
struct Solution {
    nonce: u64,
    digest: [u8; 16],
    difficulty: u8,
}

// --- DTO for FFI Serialization ---
#[derive(Serialize)]
struct SerializableSolution {
    nonce: u64,
    digest: Vec<u8>,  // Convert array to vec for JSON
    difficulty: u8,
}

// Conversion from our internal Solution type to FFI DTO
impl From<&Solution> for SerializableSolution {
    fn from(solution: &Solution) -> Self {
        Self {
            nonce: solution.nonce,
            digest: solution.digest.to_vec(),
            difficulty: solution.difficulty,
        }
    }
}

// --- Global State ---
struct MiningState {
    threads: Vec<thread::JoinHandle<()>>,
    stop_signal: Arc<AtomicBool>,
    hash_counter: Arc<AtomicU64>, // Add hash counter
}

static MINING_STATE: Lazy<Mutex<Option<MiningState>>> = Lazy::new(|| Mutex::new(None));
static SOLUTION_QUEUE: Lazy<Mutex<Vec<Solution>>> = Lazy::new(|| Mutex::new(Vec::new()));

// --- Public JNI-callable functions ---

/// Starts the mining process.
///
/// Returns:
///  0: Success
/// -1: Invalid wallet string (not valid UTF-8)
/// -2: Mining is already in progress
/// -3: Internal state is corrupted (mutex poisoned)
#[no_mangle]
pub extern "C" fn start_mining(
    wallet: *const c_char,
    epoch: u64,
    difficulty: u32,
    threads_count: u32,
    challenge: *const u8,  // 32-byte challenge array
) -> i32 {
    // REFINEMENT: Add robust error handling for mutex poisoning.
    let mut state_guard = match MINING_STATE.lock() {
        Ok(guard) => guard,
        Err(_) => return -3, // Mutex poisoned
    };

    if state_guard.is_some() {
        return -2; // Already mining
    }

    let wallet_cstr = unsafe { CStr::from_ptr(wallet) };
    let wallet_address = match wallet_cstr.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return -1, // Invalid UTF-8
    };

    // Extract challenge bytes
    let challenge_bytes: [u8; 32] = if challenge.is_null() {
        log::error!("Challenge pointer is null");
        return -1;
    } else {
        let mut bytes = [0u8; 32];
        unsafe {
            std::ptr::copy_nonoverlapping(challenge, bytes.as_mut_ptr(), 32);
        }
        bytes
    };

    let num_threads = threads_count.min(4).max(1);
    let stop_signal = Arc::new(AtomicBool::new(false));
    let hash_counter = Arc::new(AtomicU64::new(0));
    let (solution_sender, solution_receiver) = mpsc::channel();

    let mut handles = Vec::new();
    let (nonce_start, nonce_end) = calculate_nonce_range(&wallet_address, epoch);
    let nonce_range_per_thread = (nonce_end - nonce_start + 1) / num_threads as u64;

    for i in 0..num_threads {
        let thread_nonce_start = nonce_start + i as u64 * nonce_range_per_thread;
        let thread_nonce_end = (thread_nonce_start + nonce_range_per_thread - 1).min(nonce_end);

        if thread_nonce_start > nonce_end {
            continue; // Avoid spawning threads for empty ranges
        }

        let stop_signal_clone = stop_signal.clone();
        let solution_sender_clone = solution_sender.clone();
        let hash_counter_clone = hash_counter.clone();
        let challenge_clone = challenge_bytes.clone();

        let handle = thread::spawn(move || {
            mine_worker(
                stop_signal_clone,
                hash_counter_clone,
                solution_sender_clone,
                challenge_clone,
                difficulty,
                thread_nonce_start,
                thread_nonce_end,
            );
        });
        handles.push(handle);
    }

    // Solution collector thread
    thread::spawn(move || {
        for solution in solution_receiver {
            if let Ok(mut queue) = SOLUTION_QUEUE.lock() {
                queue.push(solution);
            } else {
                // If the solution queue is poisoned, we can't continue. Stop collecting.
                break;
            }
        }
    });

    *state_guard = Some(MiningState {
        threads: handles,
        stop_signal,
        hash_counter,
    });

    0 // Success
}

#[no_mangle]
pub extern "C" fn stop_mining() {
    if let Ok(mut state_guard) = MINING_STATE.lock() {
        if let Some(state) = state_guard.take() {
            state.stop_signal.store(true, Ordering::Relaxed);
            for handle in state.threads {
                handle.join().ok(); // .ok() ignores panics in worker threads
            }
        }
    }
    // If lock fails, we can't do anything, but we don't want to panic.
}

#[no_mangle]
pub extern "C" fn get_solutions() -> *mut c_char {
    // REFINEMENT: Use a DTO for serialization and handle empty queue efficiently.
    let mut queue_guard = match SOLUTION_QUEUE.lock() {
        Ok(guard) => guard,
        Err(_) => {
            // If queue is poisoned, return an empty array and log the error.
            return CString::new("[]").unwrap().into_raw();
        }
    };

    if queue_guard.is_empty() {
        return CString::new("[]").unwrap().into_raw();
    }

    let serializable_solutions: Vec<SerializableSolution> =
        queue_guard.iter().map(SerializableSolution::from).collect();
    
    queue_guard.clear();
    
    // Drop the lock before serializing
    drop(queue_guard);

    let solutions_json =
        serde_json::to_string(&serializable_solutions).unwrap_or_else(|_| "[]".to_string());

    CString::new(solutions_json).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        // Reclaim ownership of the CString and drop it
        let _ = CString::from_raw(s);
    }
}

#[no_mangle]
pub extern "C" fn get_hash_counter() -> u64 {
    match MINING_STATE.lock() {
        Ok(guard) => {
            if let Some(ref state) = *guard {
                state.hash_counter.load(Ordering::Relaxed)
            } else {
                0
            }
        }
        Err(_) => 0,
    }
}

// --- Core Logic ---

fn calculate_nonce_range(wallet_address: &str, epoch: u64) -> (u64, u64) {
    let mut hasher = Blake2b512::new();
    hasher.update(wallet_address.as_bytes());
    hasher.update(epoch.to_le_bytes());
    let hash = hasher.finalize();

    let hash_bytes = &hash[0..8];
    let partition_index = u64::from_le_bytes(hash_bytes.try_into().unwrap()) % 1000;

    let partition_size = u64::MAX / 1000;
    let nonce_start = partition_index * partition_size;
    let nonce_end = nonce_start.saturating_add(partition_size).saturating_sub(1);

    (nonce_start, nonce_end)
}

fn mine_worker(
    stop_signal: Arc<AtomicBool>,
    hash_counter: Arc<AtomicU64>,
    solution_sender: mpsc::Sender<Solution>,
    challenge: [u8; 32],
    difficulty: u32,
    nonce_start: u64,
    nonce_end: u64,
) {
    for nonce in nonce_start..=nonce_end {
        // Increment hash counter for each attempt
        hash_counter.fetch_add(1, Ordering::Relaxed);
        
        // Check stop signal every N iterations to reduce overhead
        if nonce % 1024 == 0 && stop_signal.load(Ordering::Relaxed) {
            break;
        }

        // Convert nonce to little-endian bytes
        let nonce_bytes = nonce.to_le_bytes();
        
        // Use drillx::hash to generate hash
        match drillx::hash(&challenge, &nonce_bytes) {
            Ok(hash) => {
                let hash_difficulty = hash.difficulty() as u32;
                if hash_difficulty >= difficulty {
                    // Found a valid solution!
                    let solution = Solution {
                        nonce,
                        digest: hash.d,
                        difficulty: hash_difficulty as u8,
                    };
                    
                    if solution_sender.send(solution).is_err() {
                        // Receiver has been dropped, so stop mining.
                        break;
                    }
                }
            }
            Err(_) => {
                // Hash generation failed, skip this nonce
                continue;
            }
        }
        
        // Yield CPU to prevent thermal throttling on mobile devices
        if nonce % 256 == 0 {
            thread::sleep(Duration::from_micros(100));
        }
    }
}