/// Contract Query Module - Handles all read operations from the GMINE contract
/// Implements safe, read-only queries to get epoch and miner information

use anyhow::Result;
use serde::{Serialize, Deserialize};
use serde_json::json;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use super::InjectiveClient;

/// Query message to get current epoch information
#[derive(Serialize, Debug)]
pub struct GetEpochInfoMsg {
    #[serde(rename = "current_epoch")]
    pub current_epoch: EmptyStruct,
}

/// Query message to get miner information
#[derive(Serialize, Debug)]
pub struct GetMinerInfoMsg {
    #[serde(rename = "miner_stats")]
    pub miner_stats: MinerInfoQuery,
}

#[derive(Serialize, Debug)]
pub struct MinerInfoQuery {
    pub miner: String,  // Contract expects "miner" field only
}

/// Empty struct for queries with no parameters
#[derive(Serialize, Debug)]
pub struct EmptyStruct {}

/// Phase information with block timing
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum PhaseInfo {
    Commit { ends_at: u64 },
    Reveal { ends_at: u64 },
    Settlement { ends_at: u64 },
}

/// Response from epoch info query
#[derive(Deserialize, Debug, Clone)]
pub struct EpochInfoResponse {
    pub epoch_number: u64,
    pub phase: PhaseInfo,
    pub difficulty: u8,
    pub reward_pool: String,
    pub leading_miner: Option<String>,
    pub best_score: Option<u64>,
    pub start_block: u64,
    pub target_hash: Vec<u8>,
}

/// Response from miner info query
#[derive(Deserialize, Debug, Clone)]
pub struct MinerInfoResponse {
    pub current_stake: String,
    pub last_attempt_block: u64,
    pub penalty_level: u64,
    pub successful_mines: u64,
    pub total_attempts: u64,
    pub total_rewards_earned: String,
}

/// Contract addresses for different networks
pub struct ContractAddresses {
    pub mining_contract: String,
    pub power_token: String,
}

impl ContractAddresses {
    /// Get testnet contract addresses
    pub fn testnet() -> Self {
        Self {
            mining_contract: "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y".to_string(),
            power_token: "inj1326k32dr7vjx5tnkuxlt58vkejj60r5ens29s8".to_string(),
        }
    }
}

/// Query current epoch information from the contract
pub async fn query_epoch_info(
    client: &InjectiveClient,
    contract_address: &str,
) -> Result<EpochInfoResponse> {
    // Create the query message
    let query_msg = GetEpochInfoMsg {
        current_epoch: EmptyStruct {},
    };
    
    // Serialize to JSON
    let query_data = serde_json::to_vec(&query_msg)?;
    
    // Encode as base64 (required for CosmWasm queries)
    let query_base64 = BASE64.encode(&query_data);
    
    // Build the query request
    let query_request = json!({
        "address": contract_address,
        "query_data": query_base64,
    });
    
    // Execute the query (read-only, no gas required)
    let response = client.query_contract_smart(
        contract_address,
        query_data,
    ).await?;
    
    // Debug: Print raw response to understand structure
    log::debug!("Raw epoch response: {}", serde_json::to_string_pretty(&response)?);
    
    // Parse the response
    let epoch_info: EpochInfoResponse = serde_json::from_value(response)?;
    
    log::debug!(
        "Epoch {} - Phase: {:?}, Difficulty: {}",
        epoch_info.epoch_number,
        epoch_info.phase,
        epoch_info.difficulty
    );
    
    Ok(epoch_info)
}

/// Query miner information from the contract
pub async fn query_miner_info(
    client: &InjectiveClient,
    contract_address: &str,
    miner_address: &str,
    _epoch: Option<u64>,  // Contract doesn't use epoch in query
) -> Result<MinerInfoResponse> {
    // Create the query message
    let query_msg = GetMinerInfoMsg {
        miner_stats: MinerInfoQuery {
            miner: miner_address.to_string(),
        },
    };
    
    // Serialize to JSON
    let query_data = serde_json::to_vec(&query_msg)?;
    
    // Execute the query
    let response = client.query_contract_smart(
        contract_address,
        query_data,
    ).await?;
    
    // Debug: Print raw response to understand structure
    log::debug!("Raw miner response: {}", serde_json::to_string_pretty(&response)?);
    
    // Parse the response
    let miner_info: MinerInfoResponse = serde_json::from_value(response)?;
    
    log::debug!(
        "Miner stats - Successful: {}/{}, Rewards: {}",
        miner_info.successful_mines,
        miner_info.total_attempts,
        miner_info.total_rewards_earned
    );
    
    Ok(miner_info)
}

/// Helper function to determine if we're in a valid phase for an action
pub fn can_commit(phase: &str, block_in_epoch: u64) -> bool {
    phase == "commit" || (phase == "reveal" && block_in_epoch <= 30)
}

pub fn can_reveal(phase: &str) -> bool {
    phase == "reveal"
}

pub fn can_claim(phase: &str) -> bool {
    phase == "settlement"
}

/// Calculate estimated time until next phase (in seconds)
pub fn time_until_next_phase(
    phase: &str,
    block_in_epoch: u64,
    blocks_per_epoch: u64,
) -> u64 {
    const BLOCK_TIME_SECONDS: u64 = 2; // Approximate for Injective
    
    let blocks_remaining = match phase {
        "commit" => 30 - block_in_epoch.min(30),
        "reveal" => 45 - block_in_epoch.min(45),
        "settlement" => blocks_per_epoch - block_in_epoch,
        _ => 0,
    };
    
    blocks_remaining * BLOCK_TIME_SECONDS
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_phase_validation() {
        assert!(can_commit("commit", 15));
        assert!(can_commit("reveal", 25)); // Can still commit early in reveal
        assert!(!can_commit("reveal", 35)); // Too late in reveal
        assert!(!can_commit("settlement", 48));
        
        assert!(can_reveal("reveal"));
        assert!(!can_reveal("commit"));
        assert!(!can_reveal("settlement"));
        
        assert!(can_claim("settlement"));
        assert!(!can_claim("commit"));
        assert!(!can_claim("reveal"));
    }
    
    #[test]
    fn test_time_calculation() {
        // Commit phase, block 15, should have 15 blocks = 30 seconds until reveal
        assert_eq!(time_until_next_phase("commit", 15, 50), 30);
        
        // Reveal phase, block 35, should have 10 blocks = 20 seconds until settlement
        assert_eq!(time_until_next_phase("reveal", 35, 50), 20);
        
        // Settlement phase, block 48, should have 2 blocks = 4 seconds until next epoch
        assert_eq!(time_until_next_phase("settlement", 48, 50), 4);
    }
    
    #[test]
    fn test_contract_addresses() {
        let addrs = ContractAddresses::testnet();
        assert_eq!(addrs.mining_contract, "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y");
        assert_eq!(addrs.power_token, "inj1326k32dr7vjx5tnkuxlt58vkejj60r5ens29s8");
    }
}