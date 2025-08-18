// Library exports for gmine_miner

pub mod chain;
pub mod config;
pub mod miner;
pub mod orchestrator;
pub mod telemetry;
pub mod bridge_manager;

// Re-export main types for convenience
pub use chain::{InjectiveClient, InjectiveWallet, ContractAddresses};
pub use orchestrator::{MiningOrchestrator, OrchestratorConfig};
pub use bridge_manager::BridgeManager;