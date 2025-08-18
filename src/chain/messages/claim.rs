use super::{ExecuteMsg, MessageBuilder};

/// Message for claiming mining rewards
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClaimRewardMsg {
    /// The epoch number to claim rewards for
    pub epoch_number: u64,
}

impl ClaimRewardMsg {
    /// Create a new claim reward message
    pub fn new(epoch_number: u64) -> Self {
        Self { epoch_number }
    }
}

impl MessageBuilder for ClaimRewardMsg {
    fn build_msg(&self) -> ExecuteMsg {
        ExecuteMsg::ClaimReward {
            epoch_number: self.epoch_number,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_claim_creation() {
        let epoch = 42;
        let msg = ClaimRewardMsg::new(epoch);
        assert_eq!(msg.epoch_number, epoch);
    }
    
    #[test]
    fn test_message_serialization() {
        let msg = ClaimRewardMsg::new(100);
        
        let json_bytes = msg.to_json_bytes().unwrap();
        let json_str = String::from_utf8(json_bytes).unwrap();
        
        // Verify JSON structure
        assert!(json_str.contains("claim_reward"));
        assert!(json_str.contains("epoch_number"));
        assert!(json_str.contains("100"));
    }
}