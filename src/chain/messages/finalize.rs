use serde::{Serialize, Deserialize};
use super::{ExecuteMsg, MessageBuilder};

/// Message to finalize an epoch (permissionless operation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizeEpochMsg {
    pub epoch_number: u64,
}

impl FinalizeEpochMsg {
    /// Create a new finalize epoch message
    pub fn new(epoch_number: u64) -> Self {
        Self { epoch_number }
    }
}

impl MessageBuilder for FinalizeEpochMsg {
    fn build_msg(&self) -> ExecuteMsg {
        ExecuteMsg::FinalizeEpoch {
            epoch_number: self.epoch_number,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_finalize_epoch_msg_creation() {
        let msg = FinalizeEpochMsg::new(42);
        assert_eq!(msg.epoch_number, 42);
    }
    
    #[test]
    fn test_finalize_epoch_msg_serialization() {
        let msg = FinalizeEpochMsg::new(10);
        let execute_msg = msg.build_msg();
        
        match execute_msg {
            ExecuteMsg::FinalizeEpoch { epoch_number } => {
                assert_eq!(epoch_number, 10);
            }
            _ => panic!("Wrong message type"),
        }
    }
}