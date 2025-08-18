use anyhow::{anyhow, Result};
use drillx;
use blake2::{Blake2b512, Digest};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, info, warn, error};

/// Real mining solution matching desktop implementation
#[derive(Debug, Clone)]
pub struct MiningSolution {
    pub nonce: u64,
    pub digest: [u8; 16],
    pub difficulty: u8,
    pub hash_attempts: u64,
    pub time_taken_ms: u64,
    pub commitment_hash: [u8; 32],
}

/// Real mining worker for mobile - uses actual drillx algorithm
pub struct RealMiningWorker {
    id: usize,
    nonce_start: u64,
    nonce_end: u64,
    challenge: [u8; 32],
    difficulty: u8,
    should_stop: Arc<AtomicBool>,
    hash_counter: Arc<AtomicU64>,
    solution_tx: mpsc::Sender<MiningSolution>,
}

impl RealMiningWorker {
    pub fn new(
        id: usize,
        nonce_start: u64,
        nonce_end: u64,
        challenge: [u8; 32],
        difficulty: u8,
        should_stop: Arc<AtomicBool>,
        hash_counter: Arc<AtomicU64>,
        solution_tx: mpsc::Sender<MiningSolution>,
    ) -> Self {
        Self {
            id,
            nonce_start,
            nonce_end,
            challenge,
            difficulty,
            should_stop,
            hash_counter,
            solution_tx,
        }
    }

    /// Perform real mining using drillx algorithm
    pub async fn mine(self) {
        let start_time = Instant::now();
        let mut nonce = self.nonce_start;
        let mut hash_attempts = 0u64;
        
        info!(
            "Mobile worker {} starting REAL mining. Range: {} to {}, Difficulty: {}",
            self.id, self.nonce_start, self.nonce_end, self.difficulty
        );

        // Use pre-allocated memory for mobile optimization
        let mut solver_memory = equix::SolverMemory::new();
        
        while nonce < self.nonce_end && !self.should_stop.load(Ordering::Relaxed) {
            let nonce_bytes = nonce.to_le_bytes();
            hash_attempts += 1;
            
            // Update global hash counter periodically
            if hash_attempts % 1000 == 0 {
                self.hash_counter.fetch_add(1000, Ordering::Relaxed);
                
                // Mobile-specific: yield to prevent UI freeze
                if hash_attempts % 10000 == 0 {
                    tokio::task::yield_now().await;
                    debug!("Worker {} processed {} hashes", self.id, hash_attempts);
                }
            }
            
            // REAL MINING: Use drillx to generate hash
            match drillx::hash_with_memory(&mut solver_memory, &self.challenge, &nonce_bytes) {
                Ok(hash) => {
                    let hash_difficulty = hash.difficulty() as u8;
                    
                    if hash_difficulty >= self.difficulty {
                        let elapsed = start_time.elapsed();
                        info!(
                            "Worker {} found REAL solution! Nonce: {}, Difficulty: {}, Time: {:?}",
                            self.id, nonce, hash_difficulty, elapsed
                        );
                        
                        // Generate commitment hash for commit-reveal
                        let commitment_hash = Self::generate_commitment(&hash.d, &nonce_bytes);
                        
                        let solution = MiningSolution {
                            nonce,
                            digest: hash.d,
                            difficulty: hash_difficulty,
                            hash_attempts,
                            time_taken_ms: elapsed.as_millis() as u64,
                            commitment_hash,
                        };
                        
                        // Send solution through channel
                        if let Err(e) = self.solution_tx.send(solution).await {
                            error!("Failed to send solution: {}", e);
                        }
                        
                        // Continue mining for more solutions
                    }
                }
                Err(_) => {
                    // No valid equihash solution for this nonce, continue
                }
            }
            
            nonce += 1;
            
            // Wrap around if we reach the end of our range
            if nonce >= self.nonce_end {
                nonce = self.nonce_start;
            }
        }
        
        info!("Worker {} stopped after {} attempts", self.id, hash_attempts);
    }
    
    /// Generate Blake2b512 commitment hash for commit-reveal protocol
    fn generate_commitment(digest: &[u8; 16], nonce: &[u8; 8]) -> [u8; 32] {
        let mut hasher = Blake2b512::new();
        hasher.update(b"GMINE_COMMIT_V1");
        hasher.update(digest);
        hasher.update(nonce);
        
        let hash = hasher.finalize();
        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(&hash[..32]);
        commitment
    }
}

/// Mobile-optimized mining engine using real drillx
pub struct RealMiningEngine {
    threads: usize,
    workers: Vec<tokio::task::JoinHandle<()>>,
    should_stop: Arc<AtomicBool>,
    hash_counter: Arc<AtomicU64>,
    solution_rx: mpsc::Receiver<MiningSolution>,
    solution_tx: mpsc::Sender<MiningSolution>,
    current_challenge: Option<[u8; 32]>,
    current_difficulty: u8,
    nonce_range_start: u64,
    nonce_range_end: u64,
}

impl RealMiningEngine {
    pub fn new(threads: usize) -> Self {
        let (solution_tx, solution_rx) = mpsc::channel(10);
        
        // Mobile optimization: limit threads
        let threads = threads.min(4);
        
        Self {
            threads,
            workers: Vec::new(),
            should_stop: Arc::new(AtomicBool::new(false)),
            hash_counter: Arc::new(AtomicU64::new(0)),
            solution_rx,
            solution_tx,
            current_challenge: None,
            current_difficulty: 0,
            nonce_range_start: 0,
            nonce_range_end: u64::MAX,
        }
    }
    
    /// Start real mining with given parameters
    pub async fn start_mining(
        &mut self,
        challenge: [u8; 32],
        difficulty: u8,
        nonce_start: u64,
        nonce_end: u64,
    ) -> Result<()> {
        if !self.workers.is_empty() {
            return Err(anyhow!("Mining already in progress"));
        }
        
        info!(
            "Starting REAL mobile mining with {} threads, difficulty: {}",
            self.threads, difficulty
        );
        
        self.current_challenge = Some(challenge);
        self.current_difficulty = difficulty;
        self.nonce_range_start = nonce_start;
        self.nonce_range_end = nonce_end;
        
        // Reset counters
        self.should_stop.store(false, Ordering::Relaxed);
        self.hash_counter.store(0, Ordering::Relaxed);
        
        // Calculate nonce range per worker
        let nonce_range = nonce_end - nonce_start;
        let nonce_per_worker = nonce_range / self.threads as u64;
        
        // Spawn real mining workers
        for i in 0..self.threads {
            let worker_start = nonce_start + (i as u64 * nonce_per_worker);
            let worker_end = if i == self.threads - 1 {
                nonce_end
            } else {
                worker_start + nonce_per_worker
            };
            
            let worker = RealMiningWorker::new(
                i,
                worker_start,
                worker_end,
                challenge,
                difficulty,
                Arc::clone(&self.should_stop),
                Arc::clone(&self.hash_counter),
                self.solution_tx.clone(),
            );
            
            let handle = tokio::spawn(async move {
                worker.mine().await;
            });
            
            self.workers.push(handle);
        }
        
        Ok(())
    }
    
    /// Stop all mining workers
    pub async fn stop_mining(&mut self) {
        info!("Stopping real mining engine");
        
        // Signal all workers to stop
        self.should_stop.store(true, Ordering::Relaxed);
        
        // Wait for all workers to finish
        for worker in self.workers.drain(..) {
            let _ = worker.await;
        }
        
        info!("All mining workers stopped");
    }
    
    /// Try to receive a solution (non-blocking)
    pub fn try_recv_solution(&mut self) -> Option<MiningSolution> {
        self.solution_rx.try_recv().ok()
    }
    
    /// Wait for next solution (blocking)
    pub async fn recv_solution(&mut self) -> Option<MiningSolution> {
        self.solution_rx.recv().await
    }
    
    /// Get current hashrate (hashes per second)
    pub fn get_hashrate(&self) -> f64 {
        let hashes = self.hash_counter.load(Ordering::Relaxed) as f64;
        // Convert to MH/s for display
        hashes / 1_000_000.0
    }
    
    /// Get total hashes computed
    pub fn get_total_hashes(&self) -> u64 {
        self.hash_counter.load(Ordering::Relaxed)
    }
    
    /// Check if mining is active
    pub fn is_mining(&self) -> bool {
        !self.workers.is_empty() && !self.should_stop.load(Ordering::Relaxed)
    }
}