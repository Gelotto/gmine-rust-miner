/// Chain integration modules for mobile mining
pub mod bridge_client;
pub mod embedded_bridge;
pub mod contracts;

// Re-export commonly used types
pub use bridge_client::{SignRequest, SignResponse, MessageData, Coin};
pub use embedded_bridge::EmbeddedBridgeClient;
pub use contracts::{NetworkConfig, V2_MINING_CONTRACT, gas_limits, messages, queries};