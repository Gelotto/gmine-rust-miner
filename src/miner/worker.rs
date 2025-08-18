use drillx;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info};

use super::solution::Solution;

// Constants for performance tuning
const HASH_COUNTER_BATCH_SIZE: u64 = 10;

pub struct MiningWorker {
    pub id: usize,
    pub nonce_start: u64,
    pub nonce_end: u64,
    pub hash_counter: Arc<AtomicU64>,
    pub should_stop: Arc<AtomicBool>,
}

impl MiningWorker {
    pub fn new(
        id: usize,
        nonce_start: u64,
        nonce_end: u64,
        hash_counter: Arc<AtomicU64>,
        should_stop: Arc<AtomicBool>,
    ) -> Self {
        Self {
            id,
            nonce_start,
            nonce_end,
            hash_counter,
            should_stop,
        }
    }

    pub fn mine(&self, challenge: &[u8; 32], difficulty: u8) -> Option<Solution> {
        let start_time = Instant::now();
        let mut nonce = self.nonce_start;
        let mut hash_attempts = 0u64;

        info!(
            "Worker {} starting mining. Range: {} to {}, Difficulty: {}",
            self.id, self.nonce_start, self.nonce_end, difficulty
        );
        

        while nonce < self.nonce_end && !self.should_stop.load(Ordering::Relaxed) {
            let nonce_bytes = nonce.to_le_bytes();
            hash_attempts += 1;
            
            // Update counter periodically (batch for performance)
            if hash_attempts % HASH_COUNTER_BATCH_SIZE == 0 {
                self.hash_counter.fetch_add(HASH_COUNTER_BATCH_SIZE, Ordering::Relaxed);
                debug!(
                    "Worker {} processed {} attempts, current nonce: {}",
                    self.id, hash_attempts, nonce
                );
            }
            
            // Try to generate a hash
            match drillx::hash(challenge, &nonce_bytes) {
                Ok(hash) => {
                    let hash_difficulty = hash.difficulty() as u8;
                    if hash_difficulty >= difficulty {
                        let elapsed = start_time.elapsed();
                        info!(
                            "Worker {} found solution! Nonce: {}, Difficulty: {}, Time: {:?}",
                            self.id,
                            nonce,
                            hash_difficulty,
                            elapsed
                        );

                        let mut sol = Solution::new(nonce, hash.d, hash_difficulty);
                        sol.hash_attempts = hash_attempts;
                        sol.time_taken_ms = elapsed.as_millis() as u64;

                        return Some(sol);
                    }
                }
                Err(e) => {
                    // No valid equihash solution for this nonce, continue
                    debug!("Worker {} no solution for nonce {}: {:?}", self.id, nonce, e);
                }
            }

            nonce += 1;
        }

        None
    }

    // Removed unused mine_random method
}