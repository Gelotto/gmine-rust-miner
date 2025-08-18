/// Proper transaction builder using actual Cosmos SDK proto types
/// This replaces all placeholder implementations with real functionality

use anyhow::{Result, anyhow};
use prost::Message;
use tiny_keccak::Hasher;

use crate::chain::proto::{
    Any, AuthInfo, Coin, Fee, ModeInfo, MsgExecuteContract, 
    SignDoc, SignerInfo, TxBody, TxRaw
};
use crate::chain::proto::cosmos::tx::v1beta1::mode_info;
use crate::chain::wallet::{InjectiveWallet, TransactionSigner};

/// Complete transaction builder for Cosmos SDK transactions
pub struct ProperTxBuilder<'a> {
    chain_id: String,
    account_number: u64,
    sequence: u64,
    gas_limit: u64,
    gas_price: String,
    wallet: &'a InjectiveWallet,
    signer: TransactionSigner,
}

impl<'a> ProperTxBuilder<'a> {
    /// Create a new transaction builder
    pub fn new(
        chain_id: String,
        account_number: u64,
        sequence: u64,
        wallet: &'a InjectiveWallet,
    ) -> Self {
        Self {
            chain_id,
            account_number,
            sequence,
            gas_limit: 250000,  // Default gas limit (increased for contract requirements)
            gas_price: "500000000inj".to_string(), // Default gas price
            wallet,
            signer: TransactionSigner::new(),
        }
    }
    
    /// Set the gas limit for transactions
    pub fn set_gas_limit(&mut self, gas_limit: u64) {
        self.gas_limit = gas_limit;
    }
    
    /// Set the gas price for transactions
    pub fn set_gas_price(&mut self, gas_price: String) {
        self.gas_price = gas_price;
    }
    
    /// Builder pattern method to set gas limit
    pub fn with_gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = gas_limit;
        self
    }
    
    /// Build a complete signed transaction for contract execution
    pub fn build_execute_contract_tx(
        &self,
        contract_address: &str,
        msg: Vec<u8>,
        funds: Vec<Coin>,
    ) -> Result<Vec<u8>> {
        // 1. Create the MsgExecuteContract
        let execute_msg = MsgExecuteContract {
            sender: self.wallet.address.clone(),
            contract: contract_address.to_string(),
            msg,
            funds,
        };
        
        // 2. Encode the message and wrap in Any
        let mut msg_bytes = Vec::new();
        execute_msg.encode(&mut msg_bytes)?;
        
        let any_msg = Any {
            type_url: "/cosmwasm.wasm.v1.MsgExecuteContract".to_string(),
            value: msg_bytes,
        };
        
        // 3. Create TxBody with the message
        let tx_body = TxBody {
            messages: vec![any_msg],
            memo: "".to_string(),
            timeout_height: 0,
            extension_options: vec![],
            non_critical_extension_options: vec![],
        };
        
        // 4. Create the Fee
        let fee = Fee {
            amount: self.parse_gas_price()?,
            gas_limit: self.gas_limit,
            payer: "".to_string(),
            granter: "".to_string(),
        };
        
        // 5. Get compressed public key and create PubKey protobuf message
        let compressed_key = self.wallet.public_key_compressed()?;
        
        // Create the PubKey protobuf message
        use crate::chain::proto::injective::crypto::v1beta1::ethsecp256k1::PubKey;
        let pub_key_msg = PubKey {
            key: compressed_key.to_vec(),
        };
        
        // Encode the PubKey message to bytes
        let mut pub_key_bytes = Vec::new();
        pub_key_msg.encode(&mut pub_key_bytes)?;
        
        // Wrap the encoded PubKey in Any
        let pub_key_any = Any {
            type_url: "/injective.crypto.v1beta1.ethsecp256k1.PubKey".to_string(),
            value: pub_key_bytes,
        };
        
        // 6. Create SignerInfo
        let signer_info = SignerInfo {
            public_key: Some(pub_key_any),
            mode_info: Some(ModeInfo {
                sum: Some(mode_info::Sum::Single(mode_info::Single {
                    mode: 1, // SIGN_MODE_DIRECT = 1
                })),
            }),
            sequence: self.sequence,
        };
        
        // 7. Create AuthInfo
        let auth_info = AuthInfo {
            signer_infos: vec![signer_info],
            fee: Some(fee),
        };
        
        // 8. Encode TxBody and AuthInfo
        let mut body_bytes = Vec::new();
        tx_body.encode(&mut body_bytes)?;
        
        let mut auth_info_bytes = Vec::new();
        auth_info.encode(&mut auth_info_bytes)?;
        
        // 9. Create SignDoc
        let sign_doc = SignDoc {
            body_bytes: body_bytes.clone(),
            auth_info_bytes: auth_info_bytes.clone(),
            chain_id: self.chain_id.clone(),
            account_number: self.account_number,
        };
        
        // 10. Encode and hash SignDoc for signing
        let mut sign_doc_bytes = Vec::new();
        sign_doc.encode(&mut sign_doc_bytes)?;
        
        // Use SHA256 for Cosmos SDK SIGN_MODE_DIRECT compatibility
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&sign_doc_bytes);
        let sign_hash: [u8; 32] = hasher.finalize().into();
        
        // 11. Sign the hash
        let private_key = self.wallet.private_key()?;
        let signature = self.signer.sign_message(&sign_hash, &private_key)?;
        
        // 12. Create TxRaw with signature
        let tx_raw = TxRaw {
            body_bytes,
            auth_info_bytes,
            signatures: vec![signature],
        };
        
        // 13. Encode TxRaw for broadcast
        let mut tx_bytes = Vec::new();
        tx_raw.encode(&mut tx_bytes)?;
        
        Ok(tx_bytes)
    }
    
    /// Parse gas price string into Coin array
    fn parse_gas_price(&self) -> Result<Vec<Coin>> {
        // Parse format like "500000000inj"
        let price_str = &self.gas_price;
        
        // Find where the number ends and denom begins
        let split_pos = price_str
            .chars()
            .position(|c| c.is_alphabetic())
            .ok_or_else(|| anyhow!("Invalid gas price format"))?;
        
        let (amount_str, denom) = price_str.split_at(split_pos);
        let amount: u128 = amount_str.parse()?;
        
        // Calculate total fee (gas_limit * gas_price)
        // For Injective: gas_price is already in the smallest unit (e.g., 500000000 = 0.0000000005 INJ)
        // So fee = gas_limit * gas_price without any division
        let total_fee = amount * (self.gas_limit as u128);
        
        Ok(vec![Coin {
            denom: denom.to_string(),
            amount: total_fee.to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_transaction_building() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = InjectiveWallet::from_mnemonic_no_passphrase(mnemonic).unwrap();
        
        let builder = ProperTxBuilder::new(
            "injective-888".to_string(),
            1,
            0,
            &wallet,
        );
        
        let contract_msg = r#"{"commit_solution":{"commitment":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32]}}"#;
        
        let tx_bytes = builder.build_execute_contract_tx(
            "inj1contract",
            contract_msg.as_bytes().to_vec(),
            vec![],
        );
        
        assert!(tx_bytes.is_ok());
        let tx = tx_bytes.unwrap();
        assert!(!tx.is_empty());
        
        // Verify we can decode it back
        let decoded = TxRaw::decode(&tx[..]);
        assert!(decoded.is_ok());
    }
    
    #[test]
    fn test_gas_price_parsing() {
        let wallet = InjectiveWallet::from_mnemonic_no_passphrase("test test test test test test test test test test test junk").unwrap();
        let mut builder = ProperTxBuilder::new(
            "test".to_string(),
            0,
            0,
            &wallet,
        );
        
        builder.set_gas_price("500000000inj".to_string());
        builder.set_gas_limit(250000);
        
        let coins = builder.parse_gas_price().unwrap();
        assert_eq!(coins.len(), 1);
        assert_eq!(coins[0].denom, "inj");
        // Fee should be calculated based on gas limit
    }
}