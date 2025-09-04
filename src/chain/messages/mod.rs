mod commit;
mod reveal;
mod claim;
mod finalize;
mod advance;
mod stake;
mod unstake;

pub use commit::CommitSolutionMsg;
pub use reveal::RevealSolutionMsg;
pub use claim::ClaimRewardMsg;
pub use finalize::FinalizeEpochMsg;
pub use advance::AdvanceEpochMsg;
pub use stake::StakeTokensMsg;
pub use unstake::UnstakeTokensMsg;

use serde::{Serialize, Deserialize};
use cosmwasm_std::Uint128;

/// Base message structure for CosmWasm ExecuteMsg
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    CommitSolution { commitment: [u8; 32] },
    RevealSolution { nonce: [u8; 8], digest: [u8; 16], salt: [u8; 32] },
    ClaimReward { epoch_number: u64 },
    FinalizeEpoch { epoch_number: u64 },
    AdvanceEpoch {},
    UnstakeTokens { amount: Uint128 },
}

/// Contract address for the GMINE mining contract on testnet
pub const MINING_CONTRACT_ADDRESS: &str = "inj1vd520adql0apl3wsuyhhpptl79yqwxx73e4j66"; // V3.5 with migration capability

/// Helper trait for building messages
pub trait MessageBuilder {
    /// Build the ExecuteMsg for this message
    fn build_msg(&self) -> ExecuteMsg;
    
    /// Get the contract address this message targets
    fn contract_address(&self) -> String {
        MINING_CONTRACT_ADDRESS.to_string()
    }
    
    /// Serialize the message to JSON bytes
    fn to_json_bytes(&self) -> anyhow::Result<Vec<u8>> {
        let msg = self.build_msg();
        Ok(serde_json::to_vec(&msg)?)
    }
}