use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info};

use super::{solution::Solution, worker::MiningWorker};

pub struct MiningEngine {
    threads: usize,
    hash_counter: Arc<AtomicU64>,
    should_stop: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
    solution_tx: mpsc::Sender<Solution>,
    solution_rx: mpsc::Receiver<Solution>,
    start_time: Arc<Mutex<Option<Instant>>>,
    last_hash_count: Arc<AtomicU64>,
    last_hash_time: Arc<Mutex<Option<Instant>>>,
}

impl MiningEngine {
    pub fn new(threads: usize) -> Self {
        let (solution_tx, solution_rx) = mpsc::channel(10); // Reduced buffer size
        
        Self {
            threads,
            hash_counter: Arc::new(AtomicU64::new(0)),
            should_stop: Arc::new(AtomicBool::new(false)),
            workers: Vec::new(),
            solution_tx,
            solution_rx,
            start_time: Arc::new(Mutex::new(None)),
            last_hash_count: Arc::new(AtomicU64::new(0)),
            last_hash_time: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn start_mining(
        &mut self,
        challenge: [u8; 32],
        difficulty: u8,
        nonce_start: u64,
        nonce_end: u64,
    ) -> Result<()> {
        info!(
            "Starting mining engine with {} threads, difficulty: {}",
            self.threads, difficulty
        );

        // CRITICAL FIX: Clean up old workers before starting new ones
        if !self.workers.is_empty() {
            info!("Cleaning up {} old workers before starting new epoch", self.workers.len());
            self.shutdown().await;
        }

        // Also drain any stale solutions from the channel
        let mut drained = 0;
        while let Ok(_) = self.solution_rx.try_recv() {
            drained += 1;
        }
        if drained > 0 {
            info!("Drained {} stale solutions from channel", drained);
        }

        let nonce_range = nonce_end - nonce_start;
        let nonce_per_worker = nonce_range / self.threads as u64;

        self.should_stop.store(false, Ordering::Relaxed);
        self.hash_counter.store(0, Ordering::Relaxed);
        
        // Handle potential mutex poisoning gracefully
        match self.start_time.lock() {
            Ok(mut guard) => *guard = Some(Instant::now()),
            Err(poisoned) => {
                error!("Start time mutex poisoned, recovering...");
                let mut guard = poisoned.into_inner();
                *guard = Some(Instant::now());
            }
        }

        for i in 0..self.threads {
            let worker_start = nonce_start + (i as u64 * nonce_per_worker);
            let worker_end = if i == self.threads - 1 {
                nonce_end
            } else {
                worker_start + nonce_per_worker
            };

            let worker = MiningWorker::new(
                i,
                worker_start,
                worker_end,
                Arc::clone(&self.hash_counter),
                Arc::clone(&self.should_stop),
            );

            let solution_tx = self.solution_tx.clone();
            let challenge = challenge; // No need to clone, arrays are Copy
            let should_stop = Arc::clone(&self.should_stop);

            let handle = tokio::task::spawn_blocking(move || {
                if let Some(solution) = worker.mine(&challenge, difficulty) {
                    if let Err(e) = solution_tx.blocking_send(solution) {
                        error!("Failed to send solution: {}", e);
                    }
                    should_stop.store(true, Ordering::Relaxed);
                }
            });

            self.workers.push(handle);
        }

        self.start_hashrate_monitor().await;

        Ok(())
    }

    pub async fn wait_for_solution(&mut self, timeout: Duration) -> Option<Solution> {
        tokio::time::timeout(timeout, self.solution_rx.recv()).await.ok()?
    }

    pub fn stop(&self) {
        info!("Stopping mining engine");
        self.should_stop.store(true, Ordering::Relaxed);
    }

    pub async fn shutdown(&mut self) {
        self.stop();
        
        for worker in self.workers.drain(..) {
            if let Err(e) = worker.await {
                error!("Worker shutdown error: {}", e);
            }
        }
    }

    pub fn get_hashrate(&self) -> f64 {
        let current_count = self.hash_counter.load(Ordering::Relaxed);
        let now = Instant::now();
        
        // Use sliding window approach - measure hashrate over last measurement interval
        let last_count = self.last_hash_count.load(Ordering::Relaxed);
        
        let last_time = match self.last_hash_time.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => {
                error!("Last hash time mutex poisoned in get_hashrate, recovering...");
                *poisoned.into_inner()
            }
        };
        
        // Calculate based on recent interval if available
        if let Some(last) = last_time {
            let elapsed = now.duration_since(last).as_secs_f64();
            if elapsed > 0.0 && current_count > last_count {
                let interval_hashes = current_count - last_count;
                let interval_rate = interval_hashes as f64 / elapsed;
                
                // Update tracking for next call
                self.last_hash_count.store(current_count, Ordering::Relaxed);
                match self.last_hash_time.lock() {
                    Ok(mut guard) => *guard = Some(now),
                    Err(poisoned) => {
                        let mut guard = poisoned.into_inner();
                        *guard = Some(now);
                    }
                }
                
                return interval_rate;
            }
        }
        
        // Fall back to overall average if no recent data
        let start_time = match self.start_time.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => {
                error!("Start time mutex poisoned in get_hashrate, recovering...");
                *poisoned.into_inner()
            }
        };
        
        if let Some(start) = start_time {
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                // Initialize tracking for next call
                self.last_hash_count.store(current_count, Ordering::Relaxed);
                match self.last_hash_time.lock() {
                    Ok(mut guard) => *guard = Some(now),
                    Err(poisoned) => {
                        let mut guard = poisoned.into_inner();
                        *guard = Some(now);
                    }
                }
                
                return current_count as f64 / elapsed;
            }
        }
        
        0.0
    }

    async fn start_hashrate_monitor(&self) {
        let hash_counter = Arc::clone(&self.hash_counter);
        let should_stop = Arc::clone(&self.should_stop);

        tokio::spawn(async move {
            let mut last_count = 0u64;
            let mut last_time = Instant::now();

            while !should_stop.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_secs(5)).await;
                
                let current_count = hash_counter.load(Ordering::Relaxed);
                let current_time = Instant::now();
                let elapsed = current_time.duration_since(last_time).as_secs_f64();
                
                if elapsed > 0.0 {
                    let hashrate = (current_count - last_count) as f64 / elapsed;
                    info!(
                        "Hashrate: {:.2} H/s ({:.2} MH/s)",
                        hashrate,
                        hashrate / 1_000_000.0
                    );
                }
                
                last_count = current_count;
                last_time = current_time;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_single_thread_mining() {
        let mut engine = MiningEngine::new(1);
        
        let challenge = [0u8; 32];
        let difficulty = 8;
        let nonce_start = 0;
        let nonce_end = 1_000_000;

        engine.start_mining(challenge, difficulty, nonce_start, nonce_end)
            .await
            .unwrap();

        let solution = engine.wait_for_solution(Duration::from_secs(60)).await;
        
        if let Some(sol) = solution {
            println!("Found solution: nonce={}, difficulty={}", sol.nonce, sol.difficulty);
            assert!(sol.difficulty >= difficulty);
        }

        engine.shutdown().await;
    }
}