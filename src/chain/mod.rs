pub mod wallet;
pub mod client;
pub mod client_real;
pub mod messages;
pub mod proto;
pub mod tx_builder;
pub mod queries;
pub mod account_types;
pub mod bridge_client;
pub mod rust_signer;

pub use wallet::{InjectiveWallet, TransactionSigner};
// Use the real client implementation
pub use client_real::{InjectiveClient, ClientConfig};
pub use messages::{CommitSolutionMsg, RevealSolutionMsg, ClaimRewardMsg, StakeTokensMsg, UnstakeTokensMsg};
pub use tx_builder::ProperTxBuilder;
pub use queries::{query_epoch_info, EpochInfoResponse, ContractAddresses};
pub use bridge_client::{BridgeClient, SignRequest, MessageData, Coin};
pub use rust_signer::RustSigner;