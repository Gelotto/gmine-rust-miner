/// State persistence for GMINE mobile mining
/// Handles saving/loading mining state to survive crashes and app restarts
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use chrono::{DateTime, Utc};

/// Mobile mining state that needs to persist across app restarts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileState {
    /// Current epoch number
    pub epoch: u64,
    /// Current mining phase
    pub phase: MiningPhase,
    /// Last saved timestamp
    pub last_saved: i64,
    /// Mining statistics
    pub stats: PersistentStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MiningPhase {
    /// Idle state - waiting for epoch to start
    Idle,
    /// Actively mining and looking for solutions
    Mining {
        epoch: u64,
        current_nonce: u64,
        target_difficulty: u64,
    },
    /// Waiting for commit window to open
    WaitingForCommitWindow {
        epoch: u64,
        nonce: [u8; 8],
        digest: [u8; 16],
        salt: [u8; 32],
        commitment: [u8; 32],
    },
    /// Committing solution to chain
    Committing {
        epoch: u64,
        nonce: [u8; 8],
        digest: [u8; 16],
        salt: [u8; 32],
        commitment: [u8; 32],
    },
    /// Waiting for reveal window to open
    WaitingForRevealWindow {
        epoch: u64,
        nonce: [u8; 8],
        digest: [u8; 16],
        salt: [u8; 32],
        commitment: [u8; 32],
    },
    /// Revealing solution on chain
    Revealing {
        epoch: u64,
        nonce: [u8; 8],
        digest: [u8; 16],
        salt: [u8; 32],
    },
    /// Claiming rewards from previous epoch
    Claiming {
        epoch: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentStats {
    /// Total hashes computed across all sessions
    pub total_hashes: u64,
    /// Total mining time in seconds
    pub total_mining_time_secs: u64,
    /// Solutions found across all sessions
    pub solutions_found: u64,
    /// Last session start time
    pub session_start: Option<i64>,
    /// Rewards earned (in smallest unit)
    pub total_rewards_earned: u128,
}

impl Default for PersistentStats {
    fn default() -> Self {
        Self {
            total_hashes: 0,
            total_mining_time_secs: 0,
            solutions_found: 0,
            session_start: None,
            total_rewards_earned: 0,
        }
    }
}

impl Default for MobileState {
    fn default() -> Self {
        Self {
            epoch: 1,
            phase: MiningPhase::Idle,
            last_saved: Utc::now().timestamp(),
            stats: PersistentStats::default(),
        }
    }
}

/// Mobile state persistence manager
pub struct MobileStateManager {
    state_file: PathBuf,
    current_state: MobileState,
}

impl MobileStateManager {
    /// Create a new state manager with Android app data directory
    pub fn new(app_data_dir: PathBuf) -> Self {
        let state_file = app_data_dir.join("gmine_mobile.state");
        
        Self {
            state_file,
            current_state: MobileState::default(),
        }
    }

    /// Load state from persistent storage
    pub fn load_state(&mut self) -> Result<()> {
        if !self.state_file.exists() {
            log::info!("No existing state file found, using default state");
            return Ok(());
        }

        let state_data = fs::read_to_string(&self.state_file)?;
        self.current_state = serde_json::from_str(&state_data)?;
        
        log::info!("Loaded mining state from {}: epoch {}, phase {:?}", 
            self.state_file.display(),
            self.current_state.epoch,
            self.current_state.phase
        );
        
        Ok(())
    }

    /// Save current state to persistent storage
    pub fn save_state(&mut self) -> Result<()> {
        self.current_state.last_saved = Utc::now().timestamp();
        
        let state_data = serde_json::to_string_pretty(&self.current_state)?;
        
        // Ensure parent directory exists
        if let Some(parent) = self.state_file.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Write state atomically (write to temp file first, then rename)
        let temp_file = self.state_file.with_extension("state.tmp");
        fs::write(&temp_file, state_data)?;
        fs::rename(temp_file, &self.state_file)?;
        
        log::debug!("Saved mining state to {}", self.state_file.display());
        Ok(())
    }

    /// Get current state (read-only)
    pub fn get_state(&self) -> &MobileState {
        &self.current_state
    }

    /// Update mining phase
    pub fn set_phase(&mut self, phase: MiningPhase) -> Result<()> {
        log::info!("Mining phase transition: {:?} -> {:?}", 
            self.current_state.phase, phase);
            
        self.current_state.phase = phase;
        self.save_state()
    }

    /// Update current epoch
    pub fn set_epoch(&mut self, epoch: u64) -> Result<()> {
        if epoch != self.current_state.epoch {
            log::info!("Epoch transition: {} -> {}", self.current_state.epoch, epoch);
            self.current_state.epoch = epoch;
            self.save_state()?;
        }
        Ok(())
    }

    /// Record mining session start
    pub fn start_mining_session(&mut self) -> Result<()> {
        self.current_state.stats.session_start = Some(Utc::now().timestamp());
        self.save_state()
    }

    /// Record mining session end and update stats
    pub fn end_mining_session(&mut self, hashes_computed: u64) -> Result<()> {
        if let Some(session_start) = self.current_state.stats.session_start {
            let session_duration = Utc::now().timestamp() - session_start;
            self.current_state.stats.total_mining_time_secs += session_duration as u64;
        }
        
        self.current_state.stats.total_hashes += hashes_computed;
        self.current_state.stats.session_start = None;
        
        log::info!("Mining session ended: {} hashes, total: {} hashes, total time: {}s", 
            hashes_computed,
            self.current_state.stats.total_hashes,
            self.current_state.stats.total_mining_time_secs
        );
        
        self.save_state()
    }

    /// Record solution found
    pub fn record_solution(&mut self) -> Result<()> {
        self.current_state.stats.solutions_found += 1;
        log::info!("Solution found! Total solutions: {}", 
            self.current_state.stats.solutions_found);
        self.save_state()
    }

    /// Record rewards earned
    pub fn record_rewards(&mut self, amount: u128) -> Result<()> {
        self.current_state.stats.total_rewards_earned += amount;
        log::info!("Rewards earned: {} (total: {})", 
            amount, self.current_state.stats.total_rewards_earned);
        self.save_state()
    }

    /// Get mining statistics
    pub fn get_stats(&self) -> &PersistentStats {
        &self.current_state.stats
    }

    /// Check if state needs recovery (e.g., app crashed during critical phase)
    pub fn needs_recovery(&self) -> bool {
        match &self.current_state.phase {
            MiningPhase::Committing { .. } => true,
            MiningPhase::Revealing { .. } => true,
            MiningPhase::WaitingForCommitWindow { .. } => true,
            MiningPhase::WaitingForRevealWindow { .. } => true,
            _ => false,
        }
    }

    /// Get recovery information for UI
    pub fn get_recovery_info(&self) -> Option<String> {
        match &self.current_state.phase {
            MiningPhase::Committing { epoch, .. } => {
                Some(format!("Recovering commitment for epoch {}", epoch))
            },
            MiningPhase::Revealing { epoch, .. } => {
                Some(format!("Recovering reveal for epoch {}", epoch))
            },
            MiningPhase::WaitingForCommitWindow { epoch, .. } => {
                Some(format!("Waiting to commit solution for epoch {}", epoch))
            },
            MiningPhase::WaitingForRevealWindow { epoch, .. } => {
                Some(format!("Waiting to reveal solution for epoch {}", epoch))
            },
            _ => None,
        }
    }

    /// Clear state (for testing or reset)
    pub fn reset_state(&mut self) -> Result<()> {
        self.current_state = MobileState::default();
        self.save_state()?;
        
        log::info!("Mining state reset to default");
        Ok(())
    }

    /// Get total mining uptime in human-readable format
    pub fn get_uptime_display(&self) -> String {
        let total_secs = self.current_state.stats.total_mining_time_secs;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        
        format!("{}h {}m {}s", hours, minutes, seconds)
    }

    /// Get average hash rate across all sessions
    pub fn get_average_hash_rate(&self) -> f64 {
        if self.current_state.stats.total_mining_time_secs > 0 {
            self.current_state.stats.total_hashes as f64 / self.current_state.stats.total_mining_time_secs as f64
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_state_persistence() {
        let temp_dir = tempdir().unwrap();
        let mut manager = MobileStateManager::new(temp_dir.path().to_path_buf());
        
        // Load default state
        manager.load_state().unwrap();
        assert_eq!(manager.get_state().epoch, 1);
        
        // Update state
        manager.set_epoch(123).unwrap();
        manager.record_solution().unwrap();
        
        // Create new manager and load
        let mut manager2 = MobileStateManager::new(temp_dir.path().to_path_buf());
        manager2.load_state().unwrap();
        
        assert_eq!(manager2.get_state().epoch, 123);
        assert_eq!(manager2.get_state().stats.solutions_found, 1);
    }
    
    #[test]
    fn test_mining_session_tracking() {
        let temp_dir = tempdir().unwrap();
        let mut manager = MobileStateManager::new(temp_dir.path().to_path_buf());
        
        manager.start_mining_session().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        manager.end_mining_session(1000).unwrap();
        
        let stats = manager.get_stats();
        assert_eq!(stats.total_hashes, 1000);
        assert!(stats.total_mining_time_secs > 0);
    }
}