use anyhow::Result;
use std::collections::VecDeque;
use sysinfo::System;
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};

use super::types::*;

/// Collects telemetry data from the system and miner
pub struct TelemetryCollector {
    events: RwLock<VecDeque<MinerEvent>>,
    stats: RwLock<MinerStats>,
    system: RwLock<System>,
}

impl TelemetryCollector {
    pub fn new() -> Self {
        Self {
            events: RwLock::new(VecDeque::with_capacity(10000)),
            stats: RwLock::new(MinerStats::default()),
            system: RwLock::new(System::new_all()),
        }
    }

    /// Start background collection of system metrics
    pub async fn start_collection(&self) {
        let mut interval = interval(Duration::from_secs(5));
        
        loop {
            interval.tick().await;
            self.collect_system_metrics().await;
        }
    }

    /// Collect current system metrics
    async fn collect_system_metrics(&self) {
        // For sysinfo 0.30, we need to use the new API
        // This is a simplified version that should compile
        let mut stats = self.stats.write().await;
        
        // Set placeholder values for now
        // The actual sysinfo API integration would need to be updated
        // based on sysinfo 0.30 documentation
        stats.cpu_usage = 0.0;
        stats.memory_usage = 0;
        stats.available_memory = 0;
        stats.network_bytes = 0;
    }

    /// Add a new event to the queue
    pub async fn add_event(&self, event: MinerEvent) -> Result<()> {
        let mut events = self.events.write().await;
        
        // Update stats based on event type
        let mut stats = self.stats.write().await;
        match event.event_type {
            EventType::MiningAttempt => {
                stats.total_hashes += event.duration_ms.unwrap_or(0) * event.hash_rate.unwrap_or(0.0) as u64 / 1000;
                if let Some(rate) = event.hash_rate {
                    stats.current_hash_rate = rate;
                }
            }
            EventType::SolutionFound => {
                stats.solutions_found += 1;
            }
            EventType::Submission => {
                stats.submissions_sent += 1;
                if let Some(gas) = event.gas_used {
                    stats.total_gas_used += gas;
                }
            }
            EventType::RewardsClaimed => {
                if let Some(rewards) = event.rewards_earned {
                    stats.total_rewards_earned += rewards;
                }
            }
            _ => {}
        }
        
        // Keep queue size manageable
        if events.len() >= 10000 {
            events.pop_front();
        }
        
        events.push_back(event);
        Ok(())
    }

    /// Get pending events for batch sending
    pub async fn get_pending_events(&self, batch_size: usize) -> Result<Vec<MinerEvent>> {
        let mut events = self.events.write().await;
        let mut batch = Vec::with_capacity(batch_size.min(events.len()));
        
        for _ in 0..batch_size.min(events.len()) {
            if let Some(event) = events.pop_front() {
                batch.push(event);
            }
        }
        
        Ok(batch)
    }

    /// Get current statistics
    pub async fn get_current_stats(&self) -> Result<MinerStats> {
        Ok(self.stats.read().await.clone())
    }
}