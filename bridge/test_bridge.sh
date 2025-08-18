#!/bin/bash

# Test the Node.js bridge service

echo "Testing GMINE Bridge (Node.js/TypeScript)"
echo "=========================================="

# Set test environment
export MNEMONIC="abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
export NETWORK="testnet"
export PORT="8080"
export BRIDGE_API_KEY="gmine-internal-key"

# Start the bridge in background
echo "Starting bridge service..."
npm run start &
BRIDGE_PID=$!

# Wait for service to start
sleep 3

# Test health endpoint
echo ""
echo "Testing health endpoint..."
curl -s http://127.0.0.1:8080/health | jq .

# Test sign-and-broadcast with a simple query (this will fail but shows the bridge is working)
echo ""
echo "Testing sign-and-broadcast endpoint..."
curl -s -X POST http://127.0.0.1:8080/sign-and-broadcast \
  -H "Content-Type: application/json" \
  -H "X-API-Key: gmine-internal-key" \
  -d '{
    "chain_id": "injective-888",
    "account_number": 0,
    "sequence": 0,
    "messages": [{
      "contract": "inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y",
      "msg": {"commit_solution": {"commitment": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1]}},
      "funds": []
    }],
    "gas_limit": 200000,
    "gas_price": "500000000inj",
    "memo": "",
    "request_id": "test-001"
  }' | jq .

# Kill the bridge
echo ""
echo "Stopping bridge service..."
kill $BRIDGE_PID

echo ""
echo "Test complete!"