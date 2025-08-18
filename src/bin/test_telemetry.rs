use anyhow::Result;
use gmine_miner::telemetry::simple_reporter::SimpleTelemetryReporter;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("gmine_miner=debug,info")
        .init();

    println!("Testing GMINE Telemetry Integration");
    println!("=====================================");

    // Create telemetry reporter
    let reporter = SimpleTelemetryReporter::new(
        "inj1test123456789".to_string(),
        "test-miner-local-001".to_string(),
    )?;

    // Test connection
    println!("\n1. Testing connection to telemetry backend...");
    if reporter.test_connection().await? {
        println!("   ✅ Connected to https://gmine.gelotto.io");
    } else {
        println!("   ❌ Failed to connect to telemetry backend");
        return Ok(());
    }

    // Send initial telemetry
    println!("\n2. Sending initial telemetry data...");
    reporter.send_telemetry(
        100, // Epoch 100
        Some(42.5), // 42.5 MH/s
        Some(0), // No solutions yet
        Some(0), // No reveals yet
        Some("0.15".to_string()), // Gas balance
        None, // No errors
    ).await?;
    println!("   ✅ Initial telemetry sent");

    // Simulate finding a solution
    println!("\n3. Simulating solution found...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    reporter.send_telemetry(
        100, // Same epoch
        Some(45.2), // Increased hashrate
        Some(1), // Found 1 solution
        Some(0), // No reveals yet
        Some("0.14".to_string()), // Gas used slightly
        None,
    ).await?;
    println!("   ✅ Solution telemetry sent");

    // Simulate reveal submission
    println!("\n4. Simulating reveal submission...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    reporter.send_telemetry(
        100, // Same epoch
        Some(44.8), // Hashrate stable
        Some(1), // Still 1 solution
        Some(1), // Submitted 1 reveal
        Some("0.13".to_string()), // Gas used for reveal
        None,
    ).await?;
    println!("   ✅ Reveal telemetry sent");

    // Simulate error condition
    println!("\n5. Simulating error condition...");
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    reporter.send_telemetry(
        101, // New epoch
        Some(0.0), // Mining stopped
        Some(0),
        Some(0),
        Some("0.01".to_string()), // Low gas!
        Some("Insufficient gas for reveal transaction".to_string()),
    ).await?;
    println!("   ✅ Error telemetry sent");

    println!("\n✅ All telemetry tests completed successfully!");
    println!("\nView the dashboard at: https://gmine.gelotto.io/vitals/inj1test123456789");
    
    Ok(())
}