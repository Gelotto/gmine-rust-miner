use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    pub nonce: u64,
    pub digest: [u8; 16], // Fixed size array instead of Vec
    pub difficulty: u8,
    pub hash_attempts: u64,
    pub time_taken_ms: u64,
}

impl Solution {
    pub fn new(nonce: u64, digest: [u8; 16], difficulty: u8) -> Self {
        Self {
            nonce,
            digest,
            difficulty,
            hash_attempts: 0,
            time_taken_ms: 0,
        }
    }

    pub fn to_hex(&self) -> String {
        hex::encode(&self.digest)
    }

    pub fn hashrate(&self) -> f64 {
        if self.time_taken_ms == 0 {
            return 0.0;
        }
        (self.hash_attempts as f64) / (self.time_taken_ms as f64 / 1000.0)
    }
}

// Note: extern crate not needed in Rust 2021 edition