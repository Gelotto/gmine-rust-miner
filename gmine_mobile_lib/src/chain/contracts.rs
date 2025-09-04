/// Production contract addresses for GMINE V2 deployment
/// These are the actual deployed contracts on Injective testnet

/// V3.5 Mining Contract with Migration Capability (deployed 2025-09-03)
/// Adds migration capability and configurable parameters, MIN_STAKE reduced to 100 POWER
pub const V3_5_MINING_CONTRACT: &str = "inj1vd520adql0apl3wsuyhhpptl79yqwxx73e4j66";
pub const V3_5_POWER_TOKEN: &str = "inj1esn6fgltm0fvqe2n57cdkvtwwpyyf9due8ps49";

/// V3.4 Mining Contract with Just-in-Time History Fix (deployed 2025-09-02)
/// Fixes epoch finalization bug where epochs couldn't be finalized if advance_epoch wasn't called
pub const V3_4_MINING_CONTRACT: &str = "inj1h2rq8q2ly6mwgwv4jcd5qpjvfqwvwee5v9n032";

/// V3.3 Mining Contract (deprecated - has epoch finalization bug)
pub const V3_3_MINING_CONTRACT: &str = "inj1y32mvdpmtz9gpyvxdlldulc6ertxs7z7zajs2j";

/// V2 Optimized Mining Contract (deployed 2025-08-07)
/// Gas costs: 154,585 gas = $0.0019 per reveal = $1.62/month
pub const V2_MINING_CONTRACT: &str = "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y";

/// V1 Contracts (for reference, not used in mobile)
pub const V1_POWER_TOKEN: &str = "inj13yyqg7nk6hxq9knnw9a2wqm8rfryjp0u75mcgr";
pub const V1_MINING_CONTRACT: &str = "inj1p7eqy7gmfwvzn25la5hpmpdg7p6zqrnu8hltrd";

/// Network configuration for mobile mining
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub chain_id: String,
    pub grpc_endpoint: String,
    pub rest_endpoint: String,
    pub mining_contract: String,
    pub gas_price: String,
}

impl NetworkConfig {
    /// Get testnet configuration (V2 optimized)
    pub fn testnet() -> Self {
        Self {
            chain_id: "injective-888".to_string(),
            grpc_endpoint: "https://testnet.sentry.chain.grpc.injective.network:443".to_string(),
            rest_endpoint: "https://testnet.sentry.tm.injective.network:443".to_string(),
            mining_contract: V3_5_MINING_CONTRACT.to_string(),
            gas_price: "500000000inj".to_string(), // 0.5 INJ per gas unit
        }
    }
    
    /// Get mainnet configuration (when available)
    pub fn mainnet() -> Self {
        Self {
            chain_id: "injective-1".to_string(),
            grpc_endpoint: "https://sentry.chain.grpc.injective.network:443".to_string(),
            rest_endpoint: "https://sentry.tm.injective.network:443".to_string(),
            mining_contract: "TBD".to_string(), // To be deployed
            gas_price: "500000000inj".to_string(),
        }
    }
}

/// Gas limits for different transaction types (from V2 measurements)
pub mod gas_limits {
    /// Commit solution gas limit (measured: ~150k gas)
    pub const COMMIT_SOLUTION: u64 = 250_000;
    
    /// Reveal solution gas limit (measured: ~155k gas)  
    pub const REVEAL_SOLUTION: u64 = 300_000;
    
    /// Claim rewards gas limit (higher due to token minting)
    pub const CLAIM_REWARDS: u64 = 400_000;
    
    /// Advance epoch (permissionless keeper operation)
    pub const ADVANCE_EPOCH: u64 = 200_000;
    
    /// Finalize epoch (permissionless keeper operation)
    pub const FINALIZE_EPOCH: u64 = 200_000;
}

/// Contract message types for V2 mining contract
pub mod messages {
    use serde_json::{json, Value};
    
    /// Create commit solution message
    pub fn commit_solution(commitment: &[u8; 32]) -> Value {
        json!({
            "commit_solution": {
                "commitment": commitment.to_vec()
            }
        })
    }
    
    /// Create reveal solution message
    pub fn reveal_solution(nonce: &[u8; 8], digest: &[u8; 16], salt: &[u8; 32]) -> Value {
        json!({
            "reveal_solution": {
                "nonce": nonce.to_vec(),
                "digest": digest.to_vec(),
                "salt": salt.to_vec()
            }
        })
    }
    
    /// Create claim reward message
    pub fn claim_reward(epoch_number: u64) -> Value {
        json!({
            "claim_reward": {
                "epoch_number": epoch_number
            }
        })
    }
    
    /// Create advance epoch message (permissionless)
    pub fn advance_epoch() -> Value {
        json!({
            "advance_epoch": {}
        })
    }
    
    /// Create finalize epoch message (permissionless)
    pub fn finalize_epoch(epoch_number: u64) -> Value {
        json!({
            "finalize_epoch": {
                "epoch_number": epoch_number
            }
        })
    }
}

/// Contract query types for V2 mining contract
pub mod queries {
    use serde_json::{json, Value};
    
    /// Query current epoch info
    pub fn epoch_info() -> Value {
        json!({
            "epoch_info": {}
        })
    }
    
    /// Query miner info for specific address
    pub fn miner_info(address: &str) -> Value {
        json!({
            "miner_info": {
                "address": address
            }
        })
    }
    
    /// Query unclaimed rewards for miner
    pub fn unclaimed_rewards(address: &str) -> Value {
        json!({
            "unclaimed_rewards": {
                "address": address
            }
        })
    }
    
    /// Query epoch statistics  
    pub fn epoch_stats(epoch_number: u64) -> Value {
        json!({
            "epoch_stats": {
                "epoch_number": epoch_number
            }
        })
    }
    
    /// Query mining difficulty for current epoch
    pub fn current_difficulty() -> Value {
        json!({
            "current_difficulty": {}
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_contract_addresses() {
        // V2 mining contract should be valid Injective address
        assert!(V2_MINING_CONTRACT.starts_with("inj"));
        assert_eq!(V2_MINING_CONTRACT.len(), 42); // Standard bech32 length
    }
    
    #[test]
    fn test_network_config() {
        let testnet = NetworkConfig::testnet();
        assert_eq!(testnet.chain_id, "injective-888");
        assert_eq!(testnet.mining_contract, V3_5_MINING_CONTRACT);
        
        let mainnet = NetworkConfig::mainnet();
        assert_eq!(mainnet.chain_id, "injective-1");
    }
    
    #[test]
    fn test_message_creation() {
        let commitment = [1u8; 32];
        let msg = messages::commit_solution(&commitment);
        assert!(msg.get("commit_solution").is_some());
        
        let nonce = [2u8; 8];
        let digest = [3u8; 16];
        let salt = [4u8; 32];
        let msg = messages::reveal_solution(&nonce, &digest, &salt);
        assert!(msg.get("reveal_solution").is_some());
    }
}