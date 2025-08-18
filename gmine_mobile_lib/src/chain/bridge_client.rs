/// Bridge client types shared between embedded and remote bridge implementations
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct SignRequest {
    pub chain_id: String,
    pub account_number: u64,
    pub sequence: u64,
    pub messages: Vec<MessageData>,
    pub gas_limit: u64,
    pub gas_price: String,
    pub memo: String,
    pub request_id: String,
}

#[derive(Debug, Serialize)]
pub struct MessageData {
    pub contract: String,
    pub msg: serde_json::Value,
    pub funds: Vec<Coin>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Coin {
    pub denom: String,
    pub amount: String,
}

#[derive(Debug, Deserialize)]
pub struct SignResponse {
    pub success: bool,
    pub tx_hash: Option<String>,
    pub error: Option<String>,
    pub request_id: String,
}