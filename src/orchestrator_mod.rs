/// Orchestrator module - Implements Gemini Pro's complete mining lifecycle management
pub mod epoch_monitor;
pub mod transaction_manager;

// Re-export main types
pub use epoch_monitor::{EpochMonitor, EpochMonitorConfig, EpochInfo, EpochPhase};
pub use transaction_manager::{
    TransactionManager, TransactionManagerConfig, 
    TransactionStatus, TransactionType, QueuedTransaction
};

// Import main orchestrator from mod.rs
mod orchestrator {
    pub use super::super::orchestrator::*;
}

// Re-export orchestrator types from mod.rs
pub use crate::orchestrator::{
    MiningOrchestrator, OrchestratorConfig,
    MiningPhase, MiningState, CommitmentData
};