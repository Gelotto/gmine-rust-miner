use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Types of telemetry events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    MinerStarted,
    MinerStopped,
    MiningAttempt,
    SolutionFound,
    Submission,
    EpochChange,
    RewardsClaimed,
    Error,
    SystemMetrics,
}

/// Main telemetry event structure matching TimescaleDB schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinerEvent {
    pub time: DateTime<Utc>,
    pub miner_id: Uuid,
    pub event_type: EventType,
    pub wallet_address: String,
    
    // Mining metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_rate: Option<f64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solutions_found: Option<u32>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_start: Option<u64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_end: Option<u64>,
    
    // Chain interaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch_number: Option<u64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<u64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_used: Option<u64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inj_balance: Option<f64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rewards_earned: Option<u128>,
    
    // System metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_usage: Option<f32>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_count: Option<u32>,
    
    // Performance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    
    // Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl Default for MinerEvent {
    fn default() -> Self {
        Self {
            time: Utc::now(),
            miner_id: Uuid::new_v4(),
            event_type: EventType::SystemMetrics,
            wallet_address: String::new(),
            hash_rate: None,
            solutions_found: None,
            nonce_start: None,
            nonce_end: None,
            epoch_number: None,
            difficulty: None,
            gas_used: None,
            inj_balance: None,
            rewards_earned: None,
            cpu_usage: None,
            memory_mb: None,
            worker_count: None,
            duration_ms: None,
            metadata: None,
        }
    }
}

/// Batch of telemetry events for sending
#[derive(Debug, Serialize, Deserialize)]
pub struct TelemetryBatch {
    pub events: Vec<MinerEvent>,
    pub version: String,
}

/// Current miner statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MinerStats {
    pub current_hash_rate: f64,
    pub total_hashes: u64,
    pub solutions_found: u32,
    pub submissions_sent: u32,
    pub total_gas_used: u64,
    pub total_rewards_earned: u128,
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub available_memory: u64,
    pub network_bytes: u64,
    pub uptime_seconds: u64,
}