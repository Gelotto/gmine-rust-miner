/// MsgExecuteContractCompat for Injective's EIP-712 compatibility
/// This is different from standard MsgExecuteContract in that:
/// 1. The `funds` field is a string instead of repeated Coin
/// 2. Format: "100inj,200usdt" or "0" for no funds
#[derive(Clone, PartialEq, prost::Message)]
pub struct MsgExecuteContractCompat {
    /// Sender is the actor that signed the message
    #[prost(string, tag = "1")]
    pub sender: String,
    
    /// Contract is the address of the smart contract
    #[prost(string, tag = "2")]
    pub contract: String,
    
    /// Msg json encoded message to be passed to the contract
    #[prost(string, tag = "3")]
    pub msg: String,
    
    /// Funds as a string (e.g., "100inj,200usdt" or "0")
    #[prost(string, tag = "4")]
    pub funds: String,
}

impl MsgExecuteContractCompat {
    /// Create a new MsgExecuteContractCompat
    pub fn new(
        sender: String,
        contract: String,
        msg: serde_json::Value,
        funds: Vec<crate::types::Coin>,
    ) -> Self {
        // Format funds as comma-separated string
        let funds_str = if funds.is_empty() {
            "0".to_string()
        } else {
            funds.iter()
                .map(|coin| format!("{}{}", coin.amount, coin.denom))
                .collect::<Vec<_>>()
                .join(",")
        };
        
        Self {
            sender,
            contract,
            // TODO: This is a premature stringification. The caller loses the
            // structured JSON object immediately. This function should be used
            // with caution or refactored to use a builder pattern where the
            // `msg` field can be held as a `serde_json::Value` until final encoding.
            msg: msg.to_string(),
            funds: funds_str,
        }
    }
    
    /// Get the protobuf type URL
    pub fn type_url() -> &'static str {
        "/injective.wasmx.v1.MsgExecuteContractCompat"
    }
}