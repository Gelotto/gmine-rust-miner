#[cfg(test)]
mod tests {
    use crate::tx_proto::ProtoTransactionBuilder;
    use crate::mobile_wallet::MobileWallet;
    use serde_json::json;

    #[tokio::test]
    async fn test_proto_debugging() {
        println!("\n=== Testing Protobuf Encoding Debug ===\n");
        
        // Use test wallet
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = MobileWallet::from_mnemonic(mnemonic, "").unwrap();
        let address = wallet.address.clone();
        println!("Wallet address: {}", address);
        
        // Create proto transaction builder
        let builder = ProtoTransactionBuilder::new(
            &wallet.private_key().unwrap().secret_bytes(),
            &wallet.public_key().unwrap().serialize(),
            "testnet"
        ).unwrap();
        
        // Test advance_epoch message (simplest case)
        let msg = json!({"_msg_type": "advance_epoch"});
        
        println!("Building transaction with message: {}", msg);
        
        // Build transaction
        match builder.build_transaction(
            &address,
            "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y",
            msg,
            741150,  // account_number from Node.js test
            9104,    // sequence from Node.js test
            None,
            "",
        ) {
            Ok(tx_bytes) => {
                println!("\n✅ Transaction built successfully!");
                println!("Transaction size: {} bytes", tx_bytes.len());
                println!("Transaction hex: {}", hex::encode(&tx_bytes));
                
                // The logs from tx_proto.rs will show the detailed debugging info
            }
            Err(e) => {
                println!("\n❌ Transaction building failed: {}", e);
            }
        }
    }
    
    #[test]
    fn test_message_formatting() {
        // Test how messages should be formatted
        let advance_epoch = json!({"advance_epoch": {}});
        println!("advance_epoch formatted: {}", advance_epoch);
        
        let commit = json!({"commit_solution": {"commitment": "dGVzdCBjb21taXRtZW50"}});
        println!("commit formatted: {}", commit);
    }
}