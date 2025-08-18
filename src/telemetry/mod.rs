use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{Duration, interval};
use uuid::Uuid;

pub mod collector;
pub mod reporter;
pub mod simple_reporter;
pub mod enhanced_reporter;
pub mod types;

use collector::TelemetryCollector;
use reporter::TelemetryReporter;
use types::*;

// Re-export telemetry reporters for easier access
pub use simple_reporter::SimpleTelemetryReporter;
pub use enhanced_reporter::{EnhancedTelemetryReporter, MiningStats};

/// Main telemetry manager that coordinates collection and reporting
pub struct TelemetryManager {
    enabled: bool,
    miner_id: Uuid,
    wallet_address: String,
    collector: Arc<TelemetryCollector>,
    reporter: Option<Arc<TelemetryReporter>>,
    config: TelemetryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub batch_size: usize,
    pub flush_interval_secs: u64,
    pub retry_attempts: u32,
    pub timeout_secs: u64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: "https://gmine.gelotto.io/api/telemetry".to_string(),
            batch_size: 100,
            flush_interval_secs: 30,
            retry_attempts: 3,
            timeout_secs: 10,
        }
    }
}

impl TelemetryManager {
    pub fn new(
        wallet_address: String,
        config: TelemetryConfig,
    ) -> Result<Self> {
        let miner_id = Uuid::new_v4();
        let collector = Arc::new(TelemetryCollector::new());
        
        let reporter = if config.enabled {
            Some(Arc::new(TelemetryReporter::new(
                config.endpoint.clone(),
                config.timeout_secs,
            )?))
        } else {
            None
        };

        Ok(Self {
            enabled: config.enabled,
            miner_id,
            wallet_address,
            collector,
            reporter,
            config,
        })
    }

    /// Start the telemetry background tasks
    pub async fn start(&self) -> Result<()> {
        if !self.enabled {
            tracing::info!("Telemetry disabled");
            return Ok(());
        }

        tracing::info!("Starting telemetry manager for miner {}", self.miner_id);
        
        // Start system metrics collection
        let collector = self.collector.clone();
        tokio::spawn(async move {
            collector.start_collection().await;
        });

        // Start batch reporting
        if let Some(reporter) = &self.reporter {
            let reporter = reporter.clone();
            let collector = self.collector.clone();
            let interval_secs = self.config.flush_interval_secs;
            let batch_size = self.config.batch_size;
            
            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(interval_secs));
                loop {
                    interval.tick().await;
                    if let Ok(events) = collector.get_pending_events(batch_size).await {
                        if !events.is_empty() {
                            if let Err(e) = reporter.send_batch(events).await {
                                tracing::warn!("Failed to send telemetry batch: {}", e);
                            }
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Record a mining attempt
    pub async fn record_mining_attempt(
        &self,
        nonce_start: u64,
        nonce_end: u64,
        hashes_computed: u64,
        duration_ms: u64,
    ) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let event = MinerEvent {
            time: chrono::Utc::now(),
            miner_id: self.miner_id,
            event_type: EventType::MiningAttempt,
            wallet_address: self.wallet_address.clone(),
            hash_rate: Some((hashes_computed as f64 / duration_ms as f64) * 1000.0),
            solutions_found: Some(0),
            nonce_start: Some(nonce_start),
            nonce_end: Some(nonce_end),
            duration_ms: Some(duration_ms),
            ..Default::default()
        };

        self.collector.add_event(event).await?;
        Ok(())
    }

    /// Record a solution found
    pub async fn record_solution_found(
        &self,
        nonce: u64,
        difficulty: u64,
        hash: String,
    ) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let event = MinerEvent {
            time: chrono::Utc::now(),
            miner_id: self.miner_id,
            event_type: EventType::SolutionFound,
            wallet_address: self.wallet_address.clone(),
            solutions_found: Some(1),
            nonce_start: Some(nonce),
            nonce_end: Some(nonce),
            metadata: Some(serde_json::json!({
                "difficulty": difficulty,
                "hash": hash,
            })),
            ..Default::default()
        };

        self.collector.add_event(event).await?;
        Ok(())
    }

    /// Record a submission to the chain
    pub async fn record_submission(
        &self,
        tx_hash: String,
        gas_used: u64,
        success: bool,
    ) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let event = MinerEvent {
            time: chrono::Utc::now(),
            miner_id: self.miner_id,
            event_type: EventType::Submission,
            wallet_address: self.wallet_address.clone(),
            gas_used: Some(gas_used),
            metadata: Some(serde_json::json!({
                "tx_hash": tx_hash,
                "success": success,
            })),
            ..Default::default()
        };

        self.collector.add_event(event).await?;
        Ok(())
    }

    /// Record rewards claimed
    pub async fn record_rewards_claimed(
        &self,
        amount: u128,
        tx_hash: String,
    ) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let event = MinerEvent {
            time: chrono::Utc::now(),
            miner_id: self.miner_id,
            event_type: EventType::RewardsClaimed,
            wallet_address: self.wallet_address.clone(),
            rewards_earned: Some(amount),
            metadata: Some(serde_json::json!({
                "tx_hash": tx_hash,
            })),
            ..Default::default()
        };

        self.collector.add_event(event).await?;
        Ok(())
    }

    /// Get current statistics
    pub async fn get_stats(&self) -> Result<MinerStats> {
        self.collector.get_current_stats().await
    }
}