#!/bin/bash
# Start GMINE miner with secure mnemonic handling

# Check if mnemonic is provided
if [ -z "$MNEMONIC" ]; then
    echo "Error: MNEMONIC environment variable not set"
    echo "Usage: MNEMONIC='your mnemonic here' ./start-miner.sh"
    exit 1
fi

# Start the miner
echo "Starting GMINE miner..."
./target/release/simple_miner mine