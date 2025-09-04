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
            mining_contract: "inj1vd520adql0apl3wsuyhhpptl79yqwxx73e4j66".to_string(), // V3.5 with migration capability
            power_token: "inj1esn6fgltm0fvqe2n57cdkvtwwpyyf9due8ps49".to_string(), // V3.5 power token
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
        assert_eq!(addrs.mining_contract, "inj1vd520adql0apl3wsuyhhpptl79yqwxx73e4j66");
        assert_eq!(addrs.power_token, "inj1esn6fgltm0fvqe2n57cdkvtwwpyyf9due8ps49");
    }
}

// V3.3 Query Messages
#[derive(Serialize, Debug)]
pub struct GetStakeInfoMsg {
    #[serde(rename = "stake_info")]
    pub stake_info: StakeInfoQuery,
}

#[derive(Serialize, Debug)]
pub struct StakeInfoQuery {
    pub miner: String,
}

#[derive(Serialize, Debug)]
pub struct GetEmissionMetricsMsg {
    #[serde(rename = "emission_metrics")]
    pub emission_metrics: EmptyStruct,
}

#[derive(Serialize, Debug)]
pub struct GetStakingMultipliersMsg {
    #[serde(rename = "staking_multipliers")]
    pub staking_multipliers: EmptyStruct,
}

// V3.3 Response Types
#[derive(Deserialize, Debug, Clone)]
pub struct StakeInfoResponse {
    pub miner: String,
    pub amount_staked: String,  // Changed from 'amount' to match contract
    pub lock_until: u64,
    pub multiplier: u64,
    pub effective_stake: String,
    // Note: original_lock_duration is not returned by the contract
}

#[derive(Deserialize, Debug, Clone)]
pub struct EmissionMetricsResponse {
    pub current_epoch_emissions: String,
    pub daily_emissions_estimate: String,
    pub active_staking_ratio: String,
    pub effective_emission_rate: String,
    pub circuit_breaker_active: bool,
    pub warmup_active: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct StakingMultipliersResponse {
    pub multipliers: Vec<MultiplierTier>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MultiplierTier {
    pub lock_days: u64,
    pub multiplier: u64,
    pub blocks: u64,
}

// V3.3 Query Functions
/// Query stake information for a specific miner
pub async fn query_stake_info(
    client: &InjectiveClient,
    contract_address: &str,
    miner_address: &str,
) -> Result<StakeInfoResponse> {
    let query_msg = GetStakeInfoMsg {
        stake_info: StakeInfoQuery {
            miner: miner_address.to_string(),
        },
    };
    
    let query_data = serde_json::to_vec(&query_msg)?;
    let response = client.query_contract_smart(
        contract_address,
        query_data,
    ).await?;
    
    log::debug!("Raw stake info response: {}", serde_json::to_string_pretty(&response)?);
    
    let stake_info: StakeInfoResponse = serde_json::from_value(response)?;
    
    log::info!(
        "Stake info - Amount: {} POWER, Multiplier: {}x, Locked until block: {}",
        stake_info.amount_staked,
        stake_info.multiplier as f64 / 1000.0,
        stake_info.lock_until
    );
    
    Ok(stake_info)
}

/// Query current emission metrics from the contract
pub async fn query_emission_metrics(
    client: &InjectiveClient,
    contract_address: &str,
) -> Result<EmissionMetricsResponse> {
    let query_msg = GetEmissionMetricsMsg {
        emission_metrics: EmptyStruct {},
    };
    
    let query_data = serde_json::to_vec(&query_msg)?;
    let response = client.query_contract_smart(
        contract_address,
        query_data,
    ).await?;
    
    let metrics: EmissionMetricsResponse = serde_json::from_value(response)?;
    
    log::info!(
        "Emission metrics - Current: {} POWER/epoch, Daily estimate: {} POWER",
        metrics.current_epoch_emissions,
        metrics.daily_emissions_estimate
    );
    
    Ok(metrics)
}

/// Query available staking multiplier tiers
pub async fn query_staking_multipliers(
    client: &InjectiveClient,
    contract_address: &str,
) -> Result<StakingMultipliersResponse> {
    let query_msg = GetStakingMultipliersMsg {
        staking_multipliers: EmptyStruct {},
    };
    
    let query_data = serde_json::to_vec(&query_msg)?;
    let response = client.query_contract_smart(
        contract_address,
        query_data,
    ).await?;
    
    let multipliers: StakingMultipliersResponse = serde_json::from_value(response)?;
    
    log::debug!("Available staking tiers:");
    for tier in &multipliers.multipliers {
        log::debug!(
            "  {} days ({}): {}x multiplier",
            tier.lock_days,
            tier.blocks,
            tier.multiplier as f64 / 1000.0
        );
    }
    
    Ok(multipliers)
}

/// Query POWER token balance for an address
pub async fn query_power_balance(
    client: &InjectiveClient,
    power_token_address: &str,
    address: &str,
) -> Result<cw20::BalanceResponse> {
    let query_msg = cw20::Cw20QueryMsg::Balance {
        address: address.to_string(),
    };
    
    let query_data = serde_json::to_vec(&query_msg)?;
    let response = client.query_contract_smart(
        power_token_address,
        query_data,
    ).await?;
    
    let balance: cw20::BalanceResponse = serde_json::from_value(response)?;
    
    log::debug!(
        "POWER balance for {}: {} ({})",
        address,
        balance.balance,
        balance.balance.u128() as f64 / 1_000_000.0
    );
    
    Ok(balance)
}