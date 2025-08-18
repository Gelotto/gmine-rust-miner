# Bridge Service

This is the Node.js EIP-712 bridge service for GMINE mining on Injective. It provides transaction signing capabilities for the mining client.

## Purpose

The bridge service handles EIP-712 transaction signing which is required for Injective blockchain compatibility. While the main mining client has a native Rust EIP-712 implementation, this Node.js bridge is maintained for:

1. Compatibility with older deployments
2. Testing and verification purposes
3. Alternative signing method if needed

## Installation

```bash
cd bridge
npm install
npm run build
```

## Usage

The bridge is automatically started by the mining scripts when needed:

```bash
# Use bridge with mine.sh (without --rust-signer flag)
../mine.sh --workers 4

# The bridge runs on port 3000 by default
```

## API Endpoints

- `POST /sign` - Sign a transaction using EIP-712
- `GET /health` - Health check endpoint

## Configuration

The bridge uses environment variables:
- `MNEMONIC` - Wallet mnemonic phrase
- `PORT` - Server port (default: 3000)

## Note

The native Rust EIP-712 implementation in `gmine_mobile_lib` is the recommended signing method for better performance and reliability.