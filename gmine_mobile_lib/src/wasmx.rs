// Manual implementation of MsgExecuteContractCompat since injective-protobuf doesn't include wasmx module

use prost::Message;

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgExecuteContractCompat {
    /// Sender is the that actor that signed the messages
    #[prost(string, tag = "1")]
    pub sender: ::prost::alloc::string::String,
    /// Contract is the address of the smart contract
    #[prost(string, tag = "2")]
    pub contract: ::prost::alloc::string::String,
    /// Msg json encoded message to be passed to the contract
    #[prost(string, tag = "3")]
    pub msg: ::prost::alloc::string::String,
    /// Funds coins that are transferred to the contract on execution
    /// This is a string field (e.g., "1000000inj,2000000usdt")
    #[prost(string, tag = "4")]
    pub funds: ::prost::alloc::string::String,
}

impl MsgExecuteContractCompat {
    pub const TYPE_URL: &'static str = "/injective.wasmx.v1.MsgExecuteContractCompat";
}