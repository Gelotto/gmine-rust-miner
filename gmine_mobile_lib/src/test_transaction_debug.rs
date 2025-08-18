#[cfg(test)]
mod tests {
    use crate::transaction::Eip712TransactionBuilder;
    use crate::mobile_wallet::MobileWallet;
    use crate::types::Fee;
    use serde_json::json;

    #[test]
    fn test_transaction_structure() {
        println!("\n=== Debugging Transaction Structure ===\n");
        
        // Use test wallet
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = MobileWallet::from_mnemonic(mnemonic, "").unwrap();
        let address = wallet.address.clone();
        println!("Wallet address: {}", address);
        
        // Create transaction builder
        let builder = Eip712TransactionBuilder::new(
            &wallet.private_key().unwrap().secret_bytes(),
            &wallet.public_key().unwrap().serialize(),
            "testnet"
        ).unwrap();
        
        // Simple test message
        let msg = json!({
            "commit_solution": {
                "commitment": "dGVzdCBjb21taXRtZW50IGRhdGE="
            }
        });
        
        // Build transaction with minimal values
        let tx = builder.build_transaction(
            &address,
            "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y",
            msg,
            0,  // account_number
            0,  // sequence
            Some(Fee::default()),
            "",  // empty memo
        ).unwrap();
        
        println!("Full transaction JSON:");
        println!("{}", serde_json::to_string_pretty(&tx).unwrap());
        
        // Check if transaction has expected structure
        assert!(tx.get("tx").is_some(), "Transaction missing 'tx' field");
        assert!(tx.get("mode").is_some(), "Transaction missing 'mode' field");
        
        let tx_body = &tx["tx"];
        assert!(tx_body.get("body").is_some(), "Transaction missing 'body' field");
        assert!(tx_body.get("auth_info").is_some(), "Transaction missing 'auth_info' field");
        assert!(tx_body.get("signatures").is_some(), "Transaction missing 'signatures' field");
        
        println!("\n✅ Transaction structure looks valid");
    }
}