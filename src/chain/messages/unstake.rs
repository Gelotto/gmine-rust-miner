use cosmwasm_std::Uint128;
use serde::{Serialize, Deserialize};
use super::{ExecuteMsg, MessageBuilder, MINING_CONTRACT_ADDRESS};

/// Message for unstaking POWER tokens from the mining contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnstakeTokensMsg {
    pub amount: Uint128,
}

impl UnstakeTokensMsg {
    /// Create a new unstake message
    pub fn new(amount: Uint128) -> Self {
        Self { amount }
    }
}

impl MessageBuilder for UnstakeTokensMsg {
    fn build_msg(&self) -> ExecuteMsg {
        ExecuteMsg::UnstakeTokens {
            amount: self.amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_unstake_message() {
        let amount = Uint128::new(1_000_000);
        let msg = UnstakeTokensMsg::new(amount);
        
        // Verify the message builds correctly
        match msg.build_msg() {
            ExecuteMsg::UnstakeTokens { amount: a } => {
                assert_eq!(a, amount);
            }
            _ => panic!("Wrong message type"),
        }
    }
}