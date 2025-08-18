/// Native Rust EIP-712 signer to replace Node.js bridge
use anyhow::{Result, anyhow};
use gmine_mobile::{
    tx_proto::ProtoTransactionBuilder,
    mobile_wallet::MobileWallet,
    types::{Fee as MobileFee, Coin as MobileCoin},
};
use serde_json::{json, Value};
use crate::chain::Coin;

#[derive(Clone)]
pub struct RustSigner {
    mnemonic: String,
    address: String,
    network: String,
    contract_address: String,
}

impl RustSigner {
    /// Create a new RustSigner from mnemonic
    pub fn new(mnemonic: &str, network: &str, contract_address: &str) -> Result<Self> {
        // Create wallet from mnemonic to get address
        let wallet = MobileWallet::from_mnemonic_no_passphrase(mnemonic)
            .map_err(|e| anyhow!("Failed to create wallet: {}", e))?;
        
        let address = wallet.address.clone();
        
        Ok(Self {
            mnemonic: mnemonic.to_string(),
            address,
            network: network.to_string(),
            contract_address: contract_address.to_string(),
        })
    }
    
    /// Get the wallet address
    pub fn address(&self) -> &str {
        &self.address
    }
    
    /// Sign and broadcast a commit transaction
    pub async fn sign_and_broadcast_commit(
        &self,
        commitment: Vec<u8>,
        account_number: u64,
        sequence: u64,
        fee: Option<Vec<Coin>>,
    ) -> Result<String> {
        // Contract expects commitment as [u8; 32] which serializes to array of numbers
        // NOT as a base64 string
        let msg = json!({
            "commitment": commitment
        });
        
        self.sign_and_broadcast("commit_solution", msg, account_number, sequence, fee).await
    }
    
    /// Sign and broadcast a reveal transaction
    pub async fn sign_and_broadcast_reveal(
        &self,
        nonce: Vec<u8>,
        digest: Vec<u8>,
        salt: Vec<u8>,
        account_number: u64,
        sequence: u64,
        fee: Option<Vec<Coin>>,
    ) -> Result<String> {
        // Contract expects nonce: [u8; 8], digest: [u8; 16], salt: [u8; 32]
        // which serialize to arrays of numbers, NOT base64 strings
        let msg = json!({
            "nonce": nonce,
            "digest": digest,
            "salt": salt
        });
        
        self.sign_and_broadcast("reveal_solution", msg, account_number, sequence, fee).await
    }
    
    /// Sign and broadcast a claim rewards transaction
    pub async fn sign_and_broadcast_claim(
        &self,
        epoch_number: u64,
        account_number: u64,
        sequence: u64,
        fee: Option<Vec<Coin>>,
    ) -> Result<String> {
        // ProtoTransactionBuilder expects epoch_number for claim_reward
        // Add hint to distinguish from finalize_epoch
        let msg = json!({
            "epoch_number": epoch_number,
            "_msg_type": "claim_reward"
        });
        
        self.sign_and_broadcast("claim_reward", msg, account_number, sequence, fee).await
    }
    
    /// Sign and broadcast advance_epoch message
    pub async fn sign_and_broadcast_advance_epoch(
        &self,
        account_number: u64,
        sequence: u64,
        fee: Option<Vec<Coin>>,
    ) -> Result<String> {
        // ProtoTransactionBuilder expects an empty message for advance_epoch
        let msg = json!({});
        
        self.sign_and_broadcast("advance_epoch", msg, account_number, sequence, fee).await
    }
    
    /// Sign and broadcast finalize_epoch message
    pub async fn sign_and_broadcast_finalize_epoch(
        &self,
        epoch_number: u64,
        account_number: u64,
        sequence: u64,
        fee: Option<Vec<Coin>>,
    ) -> Result<String> {
        // ProtoTransactionBuilder expects epoch_number at top level
        let msg = json!({
            "epoch_number": epoch_number
        });
        
        self.sign_and_broadcast("finalize_epoch", msg, account_number, sequence, fee).await
    }
    
    /// Internal method to sign and broadcast any transaction type
    async fn sign_and_broadcast(
        &self,
        msg_type: &str,
        msg_data: Value,
        account_number: u64,
        sequence: u64,
        fee: Option<Vec<Coin>>,
    ) -> Result<String> {
        log::info!("RustSigner: msg_type={}, msg_data={}", msg_type, msg_data);
        
        // For advance_epoch and claim_reward, we need to add a hint for tx_proto
        let msg_with_hint = if msg_type == "advance_epoch" && msg_data.is_object() && msg_data.as_object().unwrap().is_empty() {
            json!({"_msg_type": "advance_epoch"})
        } else {
            msg_data.clone()
        };
        // Recreate wallet and tx_builder for each transaction
        let wallet = MobileWallet::from_mnemonic_no_passphrase(&self.mnemonic)
            .map_err(|e| anyhow!("Failed to create wallet: {}", e))?;
        
        // Get compressed public key for EIP-712 signing
        let compressed_pub_key = wallet.public_key_compressed()
            .map_err(|e| anyhow!("Failed to get compressed public key: {}", e))?;
        
        let tx_builder = ProtoTransactionBuilder::new(
            wallet.private_key_bytes(),
            &compressed_pub_key,
            &self.network
        ).map_err(|e| anyhow!("Failed to create transaction builder: {}", e))?;
        
        // Create proper gas fee (not contract funds)
        // The 'fee' parameter here is actually contract funds, which are usually empty
        // We need to create a proper gas fee for the transaction
        let mobile_fee = Some(MobileFee {
            amount: vec![MobileCoin {
                denom: "inj".to_string(),
                amount: "500000000000000".to_string(), // 0.0005 INJ
            }],
            gas: "350000".to_string(),
            payer: String::new(),
            granter: String::new(),
        });
        
        // Build the transaction (returns protobuf bytes)
        let tx_bytes = tx_builder.build_transaction(
            &self.address,
            &self.contract_address,
            msg_with_hint,
            account_number,
            sequence,
            mobile_fee,
            "", // memo
        ).map_err(|e| anyhow!("Failed to build transaction: {}", e))?;
        
        // Submit the transaction
        let tx_hash = tx_builder.submit_transaction(tx_bytes)
            .await
            .map_err(|e| anyhow!("Failed to submit transaction: {}", e))?;
        
        Ok(tx_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rust_signer_creation() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let signer = RustSigner::new(mnemonic, "testnet", "inj1test").unwrap();
        assert!(signer.address().starts_with("inj"));
    }
}