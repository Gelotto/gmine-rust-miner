use cosmwasm_std::{to_binary, Uint128};
use serde::Serialize;
use anyhow::{Result, bail};

/// Injective block time in seconds
const INJECTIVE_BLOCK_TIME_SECONDS: u64 = 5;

/// Minimum stake amount (1 POWER = 1_000_000 micro)
const MIN_STAKE_AMOUNT: u128 = 1_000_000;

/// Message payload for staking POWER tokens - matches contract's ReceiveMsg
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiveMsg {
    StakeTokens { lock_duration_blocks: u64 },
}

/// Message for staking POWER tokens to increase mining rewards
#[derive(Debug, Clone)]
pub struct StakeTokensMsg {
    pub amount: Uint128,
    pub duration_blocks: u64,
}

impl StakeTokensMsg {
    /// Create a new stake message with validation
    pub fn new(amount: Uint128, duration_days: u64) -> Result<Self> {
        // Validate minimum stake amount
        if amount.u128() < MIN_STAKE_AMOUNT {
            bail!("Stake amount must be at least {} (1 POWER)", MIN_STAKE_AMOUNT);
        }
        
        // Validate duration is a valid tier
        let valid_durations = [0, 30, 90, 180, 365, 730];
        if !valid_durations.contains(&duration_days) {
            bail!("Invalid stake duration. Valid options: {:?} days", valid_durations);
        }
        
        // Convert days to blocks
        let duration_blocks = duration_days * 24 * 60 * 60 / INJECTIVE_BLOCK_TIME_SECONDS;
        
        Ok(Self {
            amount,
            duration_blocks,
        })
    }
    
    /// Build the CW20 Send message to stake tokens
    pub fn build_cw20_send(&self, mining_contract: &str) -> Result<cw20::Cw20ExecuteMsg> {
        let msg = ReceiveMsg::StakeTokens {
            lock_duration_blocks: self.duration_blocks,
        };
        
        Ok(cw20::Cw20ExecuteMsg::Send {
            contract: mining_contract.to_string(),
            amount: self.amount,
            msg: to_binary(&msg)?,
        })
    }
    
    /// Get the expected multiplier for this stake duration
    pub fn expected_multiplier(&self) -> f64 {
        let days = self.duration_blocks * INJECTIVE_BLOCK_TIME_SECONDS / (24 * 60 * 60);
        match days {
            0..=29 => 1.0,      // No lock: 1x
            30..=89 => 1.5,     // 30 days: 1.5x
            90..=179 => 2.0,    // 90 days: 2x
            180..=364 => 3.0,   // 180 days: 3x
            365..=729 => 4.0,   // 1 year: 4x
            _ => 5.0,           // 2 years: 5x
        }
    }
}

/// Helper to convert duration days to multiplier text
pub fn duration_to_multiplier_text(days: u64) -> &'static str {
    match days {
        30 => "1.5x multiplier",
        90 => "2x multiplier",
        180 => "3x multiplier", 
        365 => "4x multiplier",
        730 => "5x multiplier",
        _ => "1x multiplier (no lock)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Uint128;
    
    #[test]
    fn test_stake_message_validation() {
        // Test minimum amount validation
        let result = StakeTokensMsg::new(Uint128::new(999_999), 30);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least"));
        
        // Test valid amount
        let result = StakeTokensMsg::new(Uint128::new(1_000_000), 30);
        assert!(result.is_ok());
        
        // Test invalid duration
        let result = StakeTokensMsg::new(Uint128::new(1_000_000), 45);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid stake duration"));
    }
    
    #[test]
    fn test_block_calculation() {
        let msg = StakeTokensMsg::new(Uint128::new(1_000_000), 30).unwrap();
        // 30 days * 24 hours * 60 minutes * 60 seconds / 5 seconds per block
        assert_eq!(msg.duration_blocks, 518_400);
        
        let msg = StakeTokensMsg::new(Uint128::new(1_000_000), 365).unwrap();
        // 365 days * 24 * 60 * 60 / 5
        assert_eq!(msg.duration_blocks, 6_307_200);
    }
    
    #[test]
    fn test_cw20_message_building() {
        let msg = StakeTokensMsg::new(Uint128::new(10_000_000), 90).unwrap();
        let cw20_msg = msg.build_cw20_send("inj1test").unwrap();
        
        match cw20_msg {
            cw20::Cw20ExecuteMsg::Send { contract, amount, msg: binary_msg } => {
                assert_eq!(contract, "inj1test");
                assert_eq!(amount, Uint128::new(10_000_000));
                
                // Verify the binary message can be decoded
                let decoded: ReceiveMsg = serde_json::from_slice(&binary_msg.0).unwrap();
                match decoded {
                    ReceiveMsg::StakeTokens { lock_duration_blocks } => {
                        assert_eq!(lock_duration_blocks, 1_555_200); // 90 days in blocks
                    }
                }
            }
            _ => panic!("Wrong CW20 message type"),
        }
    }
    
    #[test]
    fn test_multiplier_calculation() {
        let msg = StakeTokensMsg::new(Uint128::new(1_000_000), 0).unwrap();
        assert_eq!(msg.expected_multiplier(), 1.0);
        
        let msg = StakeTokensMsg::new(Uint128::new(1_000_000), 30).unwrap();
        assert_eq!(msg.expected_multiplier(), 1.5);
        
        let msg = StakeTokensMsg::new(Uint128::new(1_000_000), 730).unwrap();
        assert_eq!(msg.expected_multiplier(), 5.0);
    }
}