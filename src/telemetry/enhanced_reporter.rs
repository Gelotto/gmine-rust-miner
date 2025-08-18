use anyhow::Result;
use chrono::Utc;
use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Enhanced telemetry data with comprehensive metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedTelemetryData {
    // Identity
    pub wallet_address: String,
    pub miner_instance_id: String,
    
    // Mining Performance
    pub current_epoch: u64,
    pub current_phase: String,
    pub timestamp: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashrate_mhs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solutions_found: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reveals_submitted: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claims_attempted: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claims_successful: Option<u32>,
    
    // Economic Metrics (NEW)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_earned_this_epoch: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_total_balance: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_spent_wei: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_price_gwei: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roi_percentage: Option<f64>,
    
    // Transaction Metrics (NEW)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commits_attempted: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commits_successful: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reveals_attempted: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reveals_successful: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tx_error: Option<String>,
    
    // Network Competition (NEW)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch_total_miners: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch_difficulty: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch_rank: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate_percentage: Option<f64>,
    
    // Operational Health
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_range_start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_range_end: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_balance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error_message: Option<String>,
}

/// Statistics tracker for success rates
#[derive(Debug, Clone)]
pub struct MiningStats {
    pub commits_attempted: u32,
    pub commits_successful: u32,
    pub reveals_attempted: u32,
    pub reveals_successful: u32,
    pub claims_attempted: u32,
    pub claims_successful: u32,
    pub total_gas_spent_wei: u64,
    pub total_power_earned: u64,
    pub epochs_won: u32,
    pub epochs_participated: u32,
    pub start_time: std::time::Instant,
}

impl Default for MiningStats {
    fn default() -> Self {
        Self {
            commits_attempted: 0,
            commits_successful: 0,
            reveals_attempted: 0,
            reveals_successful: 0,
            claims_attempted: 0,
            claims_successful: 0,
            total_gas_spent_wei: 0,
            total_power_earned: 0,
            epochs_won: 0,
            epochs_participated: 0,
            start_time: std::time::Instant::now(),
        }
    }
}

impl MiningStats {
    pub fn commits_success_rate(&self) -> f64 {
        if self.commits_attempted == 0 {
            0.0
        } else {
            (self.commits_successful as f64 / self.commits_attempted as f64) * 100.0
        }
    }
    
    pub fn reveals_success_rate(&self) -> f64 {
        if self.reveals_attempted == 0 {
            0.0
        } else {
            (self.reveals_successful as f64 / self.reveals_attempted as f64) * 100.0
        }
    }
    
    pub fn claims_success_rate(&self) -> f64 {
        if self.claims_attempted == 0 {
            0.0
        } else {
            (self.claims_successful as f64 / self.claims_attempted as f64) * 100.0
        }
    }
    
    pub fn win_rate(&self) -> f64 {
        if self.epochs_participated == 0 {
            0.0
        } else {
            (self.epochs_won as f64 / self.epochs_participated as f64) * 100.0
        }
    }
    
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}

/// Enhanced telemetry reporter with comprehensive metrics tracking
pub struct EnhancedTelemetryReporter {
    client: Client,
    endpoint: String,
    wallet_address: String,
    miner_instance_id: String,
    stats: Arc<RwLock<MiningStats>>,
    last_power_balance: Arc<RwLock<Option<u64>>>,
}

impl EnhancedTelemetryReporter {
    pub fn new(wallet_address: String, miner_instance_id: String) -> Result<Self> {
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .build()?;
        
        // Use production endpoint
        let endpoint = "https://gmine.gelotto.io/api/telemetry".to_string();
        
        Ok(Self {
            client,
            endpoint,
            wallet_address,
            miner_instance_id,
            stats: Arc::new(RwLock::new(MiningStats::default())),
            last_power_balance: Arc::new(RwLock::new(None)),
        })
    }
    
    /// Record a commit attempt
    pub async fn record_commit_attempt(&self, success: bool, gas_used: Option<u64>) {
        let mut stats = self.stats.write().await;
        stats.commits_attempted += 1;
        if success {
            stats.commits_successful += 1;
        }
        if let Some(gas) = gas_used {
            stats.total_gas_spent_wei += gas;
        }
    }
    
    /// Record a reveal attempt
    pub async fn record_reveal_attempt(&self, success: bool, gas_used: Option<u64>) {
        let mut stats = self.stats.write().await;
        stats.reveals_attempted += 1;
        if success {
            stats.reveals_successful += 1;
        }
        if let Some(gas) = gas_used {
            stats.total_gas_spent_wei += gas;
        }
    }
    
    /// Record a claim attempt
    pub async fn record_claim_attempt(&self, success: bool, power_earned: Option<u64>, gas_used: Option<u64>) {
        let mut stats = self.stats.write().await;
        stats.claims_attempted += 1;
        if success {
            stats.claims_successful += 1;
            if let Some(power) = power_earned {
                stats.total_power_earned += power;
                stats.epochs_won += 1;
            }
        }
        if let Some(gas) = gas_used {
            stats.total_gas_spent_wei += gas;
        }
    }
    
    /// Record epoch participation
    pub async fn record_epoch_participation(&self) {
        let mut stats = self.stats.write().await;
        stats.epochs_participated += 1;
    }
    
    /// Send comprehensive telemetry update
    pub async fn send_telemetry(
        &self,
        epoch: u64,
        phase: &str,
        hashrate_mhs: Option<f64>,
        solutions_found: Option<u32>,
        reveals_submitted: Option<u32>,
        network_info: Option<(u32, u32)>, // (total_miners, difficulty)
        power_balance: Option<u64>,
        gas_balance: Option<String>,
        last_error: Option<String>,
        nonce_range: Option<(u64, u64)>,
    ) -> Result<()> {
        let stats = self.stats.read().await;
        
        // Calculate ROI
        let roi = if stats.total_gas_spent_wei > 0 && stats.total_power_earned > 0 {
            let gas_cost_usd = (stats.total_gas_spent_wei as f64) / 1e18 * 50.0; // Assume $50/INJ
            let power_value_usd = (stats.total_power_earned as f64) / 1e6 * 0.1; // Assume $0.1/POWER
            Some(((power_value_usd - gas_cost_usd) / gas_cost_usd) * 100.0)
        } else {
            None
        };
        
        // Calculate power earned this epoch
        let power_earned_this_epoch = if let Some(balance) = power_balance {
            let mut last_balance = self.last_power_balance.write().await;
            let earned = if let Some(last) = *last_balance {
                if balance > last {
                    Some(balance - last)
                } else {
                    Some(0)
                }
            } else {
                None
            };
            *last_balance = Some(balance);
            earned
        } else {
            None
        };
        
        let data = EnhancedTelemetryData {
            wallet_address: self.wallet_address.clone(),
            miner_instance_id: self.miner_instance_id.clone(),
            current_epoch: epoch,
            current_phase: phase.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            
            // Mining Performance
            hashrate_mhs,
            solutions_found,
            reveals_submitted,
            claims_attempted: Some(stats.claims_attempted),
            claims_successful: Some(stats.claims_successful),
            
            // Economic Metrics
            power_earned_this_epoch,
            power_total_balance: power_balance,
            gas_spent_wei: Some(stats.total_gas_spent_wei),
            gas_price_gwei: Some(160.0), // Current testnet gas price
            roi_percentage: roi,
            
            // Transaction Metrics
            commits_attempted: Some(stats.commits_attempted),
            commits_successful: Some(stats.commits_successful),
            reveals_attempted: Some(stats.reveals_attempted),
            reveals_successful: Some(stats.reveals_successful),
            last_tx_error: last_error.clone(),
            
            // Network Competition
            epoch_total_miners: network_info.map(|(miners, _)| miners),
            epoch_difficulty: network_info.map(|(_, diff)| diff),
            epoch_rank: None, // TODO: Calculate from blockchain
            win_rate_percentage: Some(stats.win_rate()),
            
            // Operational Health
            uptime_seconds: Some(stats.uptime_seconds()),
            nonce_range_start: nonce_range.map(|(start, _)| start),
            nonce_range_end: nonce_range.map(|(_, end)| end),
            gas_balance,
            last_error_message: last_error,
        };
        
        debug!("Sending enhanced telemetry: epoch={}, phase={}, hashrate={:?}", epoch, phase, hashrate_mhs);
        
        let response = self.client
            .post(&self.endpoint)
            .json(&data)
            .send()
            .await;
        
        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    debug!("Enhanced telemetry sent successfully");
                    Ok(())
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    warn!("Telemetry backend returned error {}: {}", status, body);
                    // Don't fail the miner if telemetry fails
                    Ok(())
                }
            }
            Err(e) => {
                warn!("Failed to send telemetry: {}", e);
                // Don't fail the miner if telemetry fails
                Ok(())
            }
        }
    }
    
    /// Get current statistics
    pub async fn get_stats(&self) -> MiningStats {
        self.stats.read().await.clone()
    }
    
    /// Test telemetry connection
    pub async fn test_connection(&self) -> Result<bool> {
        let test_data = EnhancedTelemetryData {
            wallet_address: "inj1testwalletaddressfortestingconnection".to_string(),
            miner_instance_id: "00000000-0000-0000-0000-000000000000".to_string(),
            current_epoch: 0,
            current_phase: "test".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            hashrate_mhs: Some(0.0),
            solutions_found: Some(0),
            reveals_submitted: Some(0),
            claims_attempted: Some(0),
            claims_successful: Some(0),
            power_earned_this_epoch: None,
            power_total_balance: None,
            gas_spent_wei: Some(0),
            gas_price_gwei: Some(0.0),
            roi_percentage: None,
            commits_attempted: Some(0),
            commits_successful: Some(0),
            reveals_attempted: Some(0),
            reveals_successful: Some(0),
            last_tx_error: None,
            epoch_total_miners: None,
            epoch_difficulty: None,
            epoch_rank: None,
            win_rate_percentage: Some(0.0),
            uptime_seconds: Some(0),
            nonce_range_start: None,
            nonce_range_end: None,
            gas_balance: None,
            last_error_message: None,
        };
        
        match self.client.post(&self.endpoint).json(&test_data).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false)
        }
    }
}