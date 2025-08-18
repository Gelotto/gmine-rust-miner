/// Mining statistics collection and reporting
use serde::{Serialize, Deserialize};
use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Mining statistics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningStatistics {
    // Performance metrics
    pub current_hashrate: f64,
    pub average_hashrate: f64,
    pub total_hashes: u64,
    pub mining_duration: Duration,
    
    // Current state
    pub current_epoch: u64,
    pub current_difficulty: u8,
    pub nonce_range_start: u64,
    pub nonce_range_end: u64,
    pub current_phase: String,
    
    // Results
    pub solutions_found: u64,
    pub solutions_submitted: u64,
    pub solutions_accepted: u64,
    pub best_solution_difficulty: Option<u8>,
    
    // Errors
    pub connection_errors: u64,
    pub mining_errors: u64,
    pub last_error: Option<String>,
}

impl Default for MiningStatistics {
    fn default() -> Self {
        Self {
            current_hashrate: 0.0,
            average_hashrate: 0.0,
            total_hashes: 0,
            mining_duration: Duration::from_secs(0),
            current_epoch: 0,
            current_difficulty: 0,
            nonce_range_start: 0,
            nonce_range_end: 0,
            current_phase: "Idle".to_string(),
            solutions_found: 0,
            solutions_submitted: 0,
            solutions_accepted: 0,
            best_solution_difficulty: None,
            connection_errors: 0,
            mining_errors: 0,
            last_error: None,
        }
    }
}

/// Statistics collector for mining operations
pub struct StatsCollector {
    stats: Arc<RwLock<MiningStatistics>>,
    start_time: Option<Instant>,
    last_update: Instant,
    hash_count_window: Vec<(Instant, u64)>, // For calculating current hashrate
}

impl StatsCollector {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(RwLock::new(MiningStatistics::default())),
            start_time: None,
            last_update: Instant::now(),
            hash_count_window: Vec::new(),
        }
    }
    
    /// Start mining session
    pub async fn start_mining(&mut self, epoch: u64, difficulty: u8, nonce_start: u64, nonce_end: u64) {
        self.start_time = Some(Instant::now());
        self.last_update = Instant::now();
        self.hash_count_window.clear();
        
        let mut stats = self.stats.write().await;
        stats.current_epoch = epoch;
        stats.current_difficulty = difficulty;
        stats.nonce_range_start = nonce_start;
        stats.nonce_range_end = nonce_end;
        stats.current_phase = "FindingSolution".to_string();
    }
    
    /// Update hash count
    pub async fn update_hashes(&mut self, new_hashes: u64) {
        let now = Instant::now();
        
        // Add to window for current hashrate calculation
        self.hash_count_window.push((now, new_hashes));
        
        // Keep only last 5 seconds of data
        let cutoff = now - Duration::from_secs(5);
        self.hash_count_window.retain(|(time, _)| *time > cutoff);
        
        // Calculate current hashrate
        let window_hashes: u64 = self.hash_count_window.iter().map(|(_, h)| h).sum();
        let window_duration = if let Some((first_time, _)) = self.hash_count_window.first() {
            now.duration_since(*first_time).as_secs_f64()
        } else {
            1.0
        };
        let current_hashrate = window_hashes as f64 / window_duration.max(1.0);
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_hashes += new_hashes;
        stats.current_hashrate = current_hashrate;
        
        // Calculate average hashrate
        if let Some(start) = self.start_time {
            let total_duration = now.duration_since(start).as_secs_f64();
            stats.average_hashrate = stats.total_hashes as f64 / total_duration.max(1.0);
            stats.mining_duration = now.duration_since(start);
        }
    }
    
    /// Update mining phase
    pub async fn update_phase(&mut self, phase: &str) {
        let mut stats = self.stats.write().await;
        stats.current_phase = phase.to_string();
    }
    
    /// Record solution found
    pub async fn solution_found(&mut self, difficulty: u8) {
        let mut stats = self.stats.write().await;
        stats.solutions_found += 1;
        
        // Update best difficulty
        if let Some(best) = stats.best_solution_difficulty {
            if difficulty > best {
                stats.best_solution_difficulty = Some(difficulty);
            }
        } else {
            stats.best_solution_difficulty = Some(difficulty);
        }
    }
    
    /// Record solution submitted
    pub async fn solution_submitted(&mut self) {
        let mut stats = self.stats.write().await;
        stats.solutions_submitted += 1;
    }
    
    /// Record solution accepted
    pub async fn solution_accepted(&mut self) {
        let mut stats = self.stats.write().await;
        stats.solutions_accepted += 1;
    }
    
    /// Record error
    pub async fn record_error(&mut self, error_type: &str, message: String) {
        let mut stats = self.stats.write().await;
        match error_type {
            "connection" => stats.connection_errors += 1,
            "mining" => stats.mining_errors += 1,
            _ => {}
        }
        stats.last_error = Some(message);
    }
    
    /// Get current statistics snapshot
    pub async fn get_stats(&self) -> MiningStatistics {
        self.stats.read().await.clone()
    }
    
    /// Reset statistics
    pub async fn reset(&mut self) {
        *self.stats.write().await = MiningStatistics::default();
        self.start_time = None;
        self.hash_count_window.clear();
    }
}