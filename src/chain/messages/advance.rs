use super::{ExecuteMsg, MessageBuilder};

/// Message for advancing to the next epoch
/// This is called when the Settlement phase has ended
/// to move the current epoch to history and start a new one
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdvanceEpochMsg {}

impl AdvanceEpochMsg {
    /// Create a new advance epoch message
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for AdvanceEpochMsg {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageBuilder for AdvanceEpochMsg {
    fn build_msg(&self) -> ExecuteMsg {
        ExecuteMsg::AdvanceEpoch {}
    }
}