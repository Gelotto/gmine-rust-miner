/// Proto definitions for Injective/Cosmos chain integration
/// These are generated from the proto files in the proto/ directory

// Include the generated proto code
pub mod cosmos {
    pub mod base {
        pub mod v1beta1 {
            tonic::include_proto!("cosmos.base.v1beta1");
        }
        
        pub mod tendermint {
            pub mod v1beta1 {
                tonic::include_proto!("cosmos.base.tendermint.v1beta1");
            }
        }
    }
    
    pub mod vesting {
        pub mod v1beta1 {
            tonic::include_proto!("cosmos.vesting.v1beta1");
        }
    }
    
    pub mod tx {
        pub mod v1beta1 {
            tonic::include_proto!("cosmos.tx.v1beta1");
        }
    }
    
    pub mod auth {
        pub mod v1beta1 {
            tonic::include_proto!("cosmos.auth.v1beta1");
        }
    }
    
    pub mod bank {
        pub mod v1beta1 {
            tonic::include_proto!("cosmos.bank.v1beta1");
        }
    }
}

pub mod cosmwasm {
    pub mod wasm {
        pub mod v1 {
            tonic::include_proto!("cosmwasm.wasm.v1");
        }
    }
}

pub mod injective {
    pub mod types {
        pub mod v1beta1 {
            tonic::include_proto!("injective.types.v1beta1");
        }
    }
    
    pub mod crypto {
        pub mod v1beta1 {
            pub mod ethsecp256k1 {
                tonic::include_proto!("injective.crypto.v1beta1.ethsecp256k1");
            }
        }
    }
}

// Re-export commonly used types for convenience
pub use cosmos::base::v1beta1::Coin;
pub use cosmos::tx::v1beta1::{
    Tx, TxRaw, TxBody, AuthInfo, SignDoc, SignerInfo, ModeInfo, Fee, Any, SignMode,
    SimulateRequest, SimulateResponse, GasInfo, BroadcastTxRequest, BroadcastTxResponse,
    BroadcastMode, TxResponse, service_client::ServiceClient
};
pub use cosmos::auth::v1beta1::{
    BaseAccount, QueryAccountRequest, QueryAccountResponse,
    query_client::QueryClient as AuthQueryClient
};
pub use cosmos::bank::v1beta1::{
    QueryBalanceRequest, QueryBalanceResponse,
    query_client::QueryClient as BankQueryClient
};
pub use cosmos::base::tendermint::v1beta1::{
    GetNodeInfoRequest, GetNodeInfoResponse,
    GetLatestBlockRequest, GetLatestBlockResponse,
    service_client::ServiceClient as TendermintServiceClient
};
pub use cosmwasm::wasm::v1::{MsgExecuteContract, MsgExecuteContractResponse};