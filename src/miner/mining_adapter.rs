/// Mining Adapter - Bridges the gap between MiningEngine and Orchestrator
/// Provides the interface expected by Gemini Pro's orchestrator design

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use blake2::{Blake2b512, Digest};
use rand::Rng;

use super::engine::MiningEngine;
use super::solution::Solution;
use crate::orchestrator::CommitmentData;

/// Adapter that wraps MiningEngine to work with the orchestrator
pub struct MiningAdapter {
    /// The underlying mining engine
    engine: Arc<RwLock<MiningEngine>>,
    /// Current epoch being mined
    current_epoch: Arc<RwLock<Option<u64>>>,
    /// Last found solution
    last_solution: Arc<RwLock<Option<Solution>>>,
}

impl MiningAdapter {
    /// Create a new mining adapter
    pub fn new(worker_count: usize) -> Self {
        Self {
            engine: Arc::new(RwLock::new(MiningEngine::new(worker_count))),
            current_epoch: Arc::new(RwLock::new(None)),
            last_solution: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Start mining for a specific epoch
    pub async fn start_mining(
        &self,
        epoch: u64,
        difficulty: u8,
        nonce_range: (u64, u64),
    ) -> Result<()> {
        // Store current epoch
        *self.current_epoch.write().await = Some(epoch);
        
        // Generate challenge from epoch (placeholder - should query from contract)
        let challenge = self.generate_challenge(epoch);
        
        // Start the engine
        let mut engine = self.engine.write().await;
        engine.start_mining(
            challenge,
            difficulty,
            nonce_range.0,
            nonce_range.1,
        ).await?;
        
        log::info!(
            "Started mining for epoch {} with difficulty {} and nonce range {:?}",
            epoch,
            difficulty,
            nonce_range
        );
        
        Ok(())
    }
    
    /// Start mining with specific target hash from contract
    pub async fn start_mining_with_target(
        &self,
        epoch: u64,
        target_hash: [u8; 32],
        difficulty: u8,
        nonce_range: (u64, u64),
    ) -> Result<()> {
        // Store current epoch
        *self.current_epoch.write().await = Some(epoch);
        
        // Use the actual target hash from the contract
        let challenge = target_hash;
        
        // Start the engine
        let mut engine = self.engine.write().await;
        engine.start_mining(
            challenge,
            difficulty,
            nonce_range.0,
            nonce_range.1,
        ).await?;
        
        log::info!(
            "Started mining for epoch {} with target_hash from contract, difficulty {} and nonce range {:?}",
            epoch,
            difficulty,
            nonce_range
        );
        
        Ok(())
    }
    
    /// Stop mining
    pub async fn stop_mining(&self) -> Result<()> {
        let engine = self.engine.read().await;
        engine.stop();
        
        *self.current_epoch.write().await = None;
        
        log::info!("Stopped mining");
        Ok(())
    }
    
    /// Check if a solution has been found (non-blocking)
    pub async fn check_solution(&self) -> Option<CommitmentData> {
        // Try to get a solution from the engine (with very short timeout)
        let mut engine = self.engine.write().await;
        let solution = engine.wait_for_solution(
            std::time::Duration::from_millis(10)
        ).await;
        
        if let Some(sol) = solution {
            // Store the solution
            *self.last_solution.write().await = Some(sol.clone());
            
            // Generate commitment data
            let salt = self.generate_salt();
            let commitment = self.create_commitment(
                &sol.nonce.to_le_bytes(),
                &sol.digest,
                &salt,
            );
            
            let epoch = self.current_epoch.read().await.unwrap_or(0);
            
            return Some(CommitmentData {
                epoch,
                nonce: sol.nonce.to_le_bytes(),
                digest: sol.digest,
                salt,
                commitment,
            });
        }
        
        None
    }
    
    /// Get current hashrate
    pub async fn get_hashrate(&self) -> f64 {
        self.engine.read().await.get_hashrate()
    }
    
    /// Generate challenge from epoch (placeholder implementation)
    fn generate_challenge(&self, epoch: u64) -> [u8; 32] {
        // In production, this would query the actual challenge from the contract
        // For now, use a deterministic challenge based on epoch
        let mut hasher = Blake2b512::new();
        hasher.update(b"gmine_challenge");
        hasher.update(&epoch.to_le_bytes());
        let result = hasher.finalize();
        let mut challenge = [0u8; 32];
        challenge.copy_from_slice(&result[0..32]);
        challenge
    }
    
    /// Generate random salt for commitment
    fn generate_salt(&self) -> [u8; 32] {
        let mut rng = rand::thread_rng();
        let mut salt = [0u8; 32];
        rng.fill(&mut salt);
        salt
    }
    
    /// Create commitment hash using Blake2b512 (matches contract)
    fn create_commitment(
        &self,
        nonce: &[u8; 8],
        digest: &[u8; 16],
        salt: &[u8; 32],
    ) -> [u8; 32] {
        let mut hasher = Blake2b512::new();
        hasher.update(nonce);
        hasher.update(digest);
        hasher.update(salt);
        let result = hasher.finalize();
        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(&result[0..32]);
        commitment
    }
}

/// Wrapper that implements the interface expected by the orchestrator
pub struct MiningEngineWrapper {
    adapter: MiningAdapter,
}

impl MiningEngineWrapper {
    pub fn new(worker_count: usize) -> Self {
        Self {
            adapter: MiningAdapter::new(worker_count),
        }
    }
    
    pub async fn start_mining(
        &mut self,
        epoch: u64,
        difficulty: u8,
        nonce_range: (u64, u64),
    ) -> Result<()> {
        self.adapter.start_mining(epoch, difficulty, nonce_range).await
    }
    
    pub async fn start_mining_with_target(
        &mut self,
        epoch: u64,
        target_hash: [u8; 32],
        difficulty: u8,
        nonce_range: (u64, u64),
    ) -> Result<()> {
        self.adapter.start_mining_with_target(epoch, target_hash, difficulty, nonce_range).await
    }
    
    pub async fn check_solution(&mut self) -> Option<CommitmentData> {
        self.adapter.check_solution().await
    }
    
    pub async fn stop_mining(&mut self) -> Result<()> {
        self.adapter.stop_mining().await
    }
    
    pub async fn get_hashrate(&self) -> f64 {
        self.adapter.get_hashrate().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_mining_adapter() {
        let mut wrapper = MiningEngineWrapper::new(2);
        
        // Start mining
        wrapper.start_mining(1, 8, (0, 1000000)).await.unwrap();
        
        // Wait a bit for mining
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        
        // Check hashrate
        let hashrate = wrapper.get_hashrate().await;
        assert!(hashrate >= 0.0);
        
        // Stop mining
        wrapper.stop_mining().await.unwrap();
    }
}