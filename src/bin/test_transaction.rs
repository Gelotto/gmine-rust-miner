use gmine_miner::chain::rust_signer::RustSigner;
use injective_std::types::cosmos::base::v1beta1::Coin;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <advance_epoch|commit|reveal|claim> [args...]", args[0]);
        std::process::exit(1);
    }
    
    // Get mnemonic from environment
    let mnemonic = env::var("MNEMONIC")
        .expect("MNEMONIC environment variable must be set");
    
    // Create signer
    let signer = RustSigner::new(&mnemonic)?;
    
    // Get account info
    let account = signer.get_account().await?;
    println!("Account: {}", account.address);
    println!("Account Number: {}", account.account_number);
    println!("Sequence: {}", account.sequence);
    
    // Test fee
    let fee = Some(vec![Coin {
        denom: "inj".to_string(),
        amount: "20000000000000".to_string(),
    }]);
    
    let tx_hash = match args[1].as_str() {
        "advance_epoch" => {
            println!("\nTesting advance_epoch transaction...");
            signer.sign_and_broadcast_advance_epoch(
                account.account_number,
                account.sequence,
                fee,
            ).await?
        },
        "commit" => {
            if args.len() < 3 {
                eprintln!("Usage: {} commit <commitment_hex>", args[0]);
                std::process::exit(1);
            }
            println!("\nTesting commit_solution transaction...");
            let commitment_hex = &args[2];
            let commitment = hex::decode(commitment_hex)?;
            println!("Commitment bytes: {:?}", commitment);
            println!("Commitment base64: {}", base64::encode(&commitment));
            
            signer.sign_and_broadcast_commit(
                commitment,
                account.account_number,
                account.sequence,
                fee,
            ).await?
        },
        "reveal" => {
            if args.len() < 5 {
                eprintln!("Usage: {} reveal <nonce_hex> <digest_hex> <salt_hex>", args[0]);
                std::process::exit(1);
            }
            println!("\nTesting reveal_solution transaction...");
            let nonce = hex::decode(&args[2])?;
            let digest = hex::decode(&args[3])?;
            let salt = hex::decode(&args[4])?;
            
            println!("Nonce bytes: {:?}", nonce);
            println!("Digest bytes: {:?}", digest);
            println!("Salt bytes: {:?}", salt);
            
            signer.sign_and_broadcast_reveal(
                nonce,
                digest,
                salt,
                account.account_number,
                account.sequence,
                fee,
            ).await?
        },
        "claim" => {
            if args.len() < 3 {
                eprintln!("Usage: {} claim <epoch_number>", args[0]);
                std::process::exit(1);
            }
            println!("\nTesting claim_reward transaction...");
            let epoch: u64 = args[2].parse()?;
            
            signer.sign_and_broadcast_claim_rewards(
                Some(epoch),
                account.account_number,
                account.sequence,
                fee,
            ).await?
        },
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            std::process::exit(1);
        }
    };
    
    println!("\nTransaction submitted!");
    println!("Hash: {}", tx_hash);
    println!("\nCheck on testnet:");
    println!("https://testnet.explorer.injective.network/transaction/{}", tx_hash);
    
    // Wait a bit then check the transaction
    println!("\nWaiting 5 seconds to check transaction status...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    
    // Query the transaction
    let tx_response = signer.query_tx(&tx_hash).await?;
    println!("\nTransaction result:");
    println!("Code: {}", tx_response.code);
    if tx_response.code == 0 {
        println!("Status: SUCCESS ✅");
    } else {
        println!("Status: FAILED ❌");
        println!("Raw log: {}", tx_response.raw_log);
    }
    
    Ok(())
}