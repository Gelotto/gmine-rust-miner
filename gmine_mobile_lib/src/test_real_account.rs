#[cfg(test)]
mod tests {
    use crate::transaction::Eip712TransactionBuilder;
    use crate::mobile_wallet::MobileWallet;
    use crate::types::Fee;
    use serde_json::json;

    #[tokio::test]
    async fn test_eip712_with_real_account() {
        println!("\n=== Testing EIP-712 with Real Testnet Account ===\n");
        
        // Use the test mnemonic from CLAUDE.md
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        
        // Create wallet from mnemonic
        let wallet = MobileWallet::from_mnemonic(mnemonic, "").unwrap();
        let address = wallet.address.clone();
        println!("Wallet address: {}", address);
        
        // Get account info from chain
        let client = crate::blockchain::BlockchainClient::new();
        
        let (account_number, sequence) = match client.get_account_info(&address) {
            Ok((num, seq)) => {
                println!("Account found on chain!");
                println!("Account number: {}", num);
                println!("Sequence: {}", seq);
                (num, seq)
            }
            Err(e) => {
                println!("Failed to get account info: {}", e);
                println!("Using defaults for new account");
                (0, 0)
            }
        };
        
        // Create transaction builder with real wallet
        let builder = Eip712TransactionBuilder::new(
            &wallet.private_key().unwrap().secret_bytes(),
            &wallet.public_key().unwrap().serialize(),
            "testnet"
        ).unwrap();
        
        // Test commitment message
        let msg = json!({
            "commit_solution": {
                "commitment": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"test commitment data")
            }
        });
        
        // Build transaction
        let tx = builder.build_transaction(
            &address,
            "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y",  // Mining contract
            msg,
            account_number,
            sequence,
            Some(Fee::default()),
            "Test EIP-712 transaction",
        ).unwrap();
        
        println!("\nTransaction built successfully!");
        
        // Try to submit transaction
        match builder.submit_transaction(&tx) {
            Ok(txhash) => {
                println!("✅ Transaction submitted successfully!");
                println!("Transaction hash: {}", txhash);
                println!("View on explorer: https://testnet.explorer.injective.network/transaction/{}", txhash);
            }
            Err(e) => {
                println!("❌ Transaction submission failed: {}", e);
                let error_str = e.to_string();
                if error_str.contains("signature verification failed") {
                    println!("Still have signature verification issues");
                } else if error_str.contains("account sequence mismatch") {
                    println!("Sequence mismatch - account sequence has changed");
                } else if error_str.contains("insufficient funds") {
                    println!("Insufficient funds - need testnet INJ");
                } else if error_str.contains("invalid empty tx") {
                    println!("Invalid empty tx - transaction structure issue");
                } else {
                    println!("Different error type");
                }
            }
        }
    }
}