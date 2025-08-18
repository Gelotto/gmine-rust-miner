# GMINE Rust Miner

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

The official high-performance CPU mining client for the GMINE network, written in Rust.

This repository contains the production mining client (`simple_miner`) for mining POWER tokens on the Injective blockchain through proof-of-work using the drillx (Equihash) algorithm.

---

## Table of Contents

- [Prerequisites](#prerequisites)
- [Quick Start: Start Mining in 5 Minutes](#quick-start-start-mining-in-5-minutes)
- [Configuration](#configuration)
- [Usage Guide](#usage-guide)
  - [Choosing Your Signing Method](#choosing-your-signing-method)
  - [Running with Native Rust Signer (Recommended)](#running-with-native-rust-signer-recommended)
  - [Running with Node.js Bridge (Legacy)](#running-with-nodejs-bridge-legacy)
- [Binaries Explained](#binaries-explained)
  - [`simple_miner` (Production Miner)](#simple_miner-production-miner)
  - [`gmine_miner` (Development Tool)](#gmine_miner-development-tool)
- [Building from Source](#building-from-source)
- [Docker Usage](#docker-usage)
- [Technical Overview](#technical-overview)
- [Performance & Requirements](#performance--requirements)
- [Security Considerations](#security-considerations)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

---

## Prerequisites

Before you begin, ensure you have the following:

- **Rust Toolchain**: Install via [rustup](https://rustup.rs/) (1.70+ required)
- **Git**
- **Build Essentials**: `build-essential` on Debian/Ubuntu, or equivalent for your OS
- **Node.js and npm** (optional): Only required if using the legacy Node.js bridge
- **Injective Testnet Wallet**: With at least 0.5 INJ for gas fees

---

## Quick Start: Start Mining in 5 Minutes

The fastest way to start mining is using the native Rust EIP-712 signer:

1. **Clone the repository:**
   ```bash
   git clone https://github.com/Gelotto/gmine-rust-miner.git
   cd gmine-rust-miner
   ```

2. **Build the mining client:**
   ```bash
   cargo build --release --bin simple_miner
   ```

3. **Start mining with test wallet (for testing only):**
   ```bash
   ./mine-rust.sh --workers 4
   ```

4. **For production mining with your own wallet:**
   ```bash
   ./mine-rust.sh --mnemonic "your twelve word mnemonic phrase here" --workers 4
   ```

   **⚠️ WARNING**: Use a NEW wallet for mining, not your main wallet!

5. **Get testnet INJ tokens:**
   Visit https://testnet.faucet.injective.network/ to get free testnet tokens

---

## Configuration

The miner accepts the following command-line arguments:

| Argument | Description | Default | Required |
|----------|-------------|---------|----------|
| `--mnemonic` / `-m` | Wallet mnemonic phrase (12 or 24 words) | Test mnemonic | No (but recommended) |
| `--workers` / `-w` | Number of CPU threads for mining | 1 | No |
| `--network` / `-n` | Network to mine on (`testnet` or `mainnet`) | `testnet` | No |
| `--duration` / `-d` | Mining duration in seconds | Unlimited | No |
| `--rust-signer` / `-r` | Use native Rust EIP-712 signer | `false` (for mine.sh) | No |
| `--verbose` / `-v` | Enable debug logging | `false` | No |

### Environment Variables

You can also configure the miner using environment variables:

- `MNEMONIC`: Wallet mnemonic phrase
- `GMINE_WORKERS`: Number of mining workers
- `GMINE_NETWORK`: Network selection

---

## Usage Guide

### Choosing Your Signing Method

GMINE mining requires EIP-712 signatures for submitting solutions. This repository provides two methods:

#### 1. **Native Rust Signer** (Recommended) ✅
- **Pros**: 
  - Handles nonces > 2^53-1 correctly (no JavaScript precision errors)
  - 10x faster transaction signing
  - No external dependencies
  - Single process, more reliable for 24/7 mining
- **Usage**: `./mine-rust.sh`

#### 2. **Node.js Bridge** (Legacy)
- **Pros**: Battle-tested with ethers.js library
- **Cons**: 
  - JavaScript precision limitation causes failures after ~2-3 hours
  - Requires Node.js runtime (9MB overhead)
  - Separate bridge process
- **Usage**: `./mine.sh`

### Running with Native Rust Signer (Recommended)

```bash
# Basic usage with test wallet
./mine-rust.sh

# Production mining with your wallet and 4 workers
./mine-rust.sh --mnemonic "your mnemonic phrase" --workers 4

# Mine for 1 hour with debug logging
./mine-rust.sh --duration 3600 --verbose

# Using environment variables
export MNEMONIC="your mnemonic phrase"
export GMINE_WORKERS=4
./mine-rust.sh
```

### Running with Node.js Bridge (Legacy)

First, build the bridge:
```bash
cd bridge
npm install
npm run build
cd ..
```

Then run:
```bash
# Basic usage
./mine.sh

# With options (same as mine-rust.sh)
./mine.sh --mnemonic "your mnemonic phrase" --workers 4
```

---

## Binaries Explained

### `simple_miner` (Production Miner)

This is the **official production mining client** that:
- Connects to the Injective blockchain
- Mines drillx proof-of-work solutions
- Submits commit/reveal transactions
- Claims POWER token rewards

**This is the binary you should use for actual mining.**

### `gmine_miner` (Development Tool)

This is a **testing and development tool** that:
- Does NOT connect to the blockchain
- Does NOT submit real transactions
- Used for testing mining algorithms locally

**⚠️ WARNING**: Do NOT use `gmine_miner` for actual mining. You will not earn any rewards!

---

## Building from Source

### Standard Build
```bash
# Clone the repository
git clone https://github.com/Gelotto/gmine-rust-miner.git
cd gmine-rust-miner

# Build the production miner
cargo build --release --bin simple_miner

# The binary will be at: ./target/release/simple_miner
```

### Development Build
```bash
# Build with debug symbols
cargo build --bin simple_miner

# Run tests
cargo test

# Check code
cargo clippy
```

---

## Docker Usage

### Using Pre-built Image
```bash
docker pull gelottohq/gmine:latest

docker run -d \
  --name gmine-miner \
  -e MNEMONIC="your twelve word mnemonic phrase here" \
  -e NETWORK=testnet \
  -e WORKERS=4 \
  -v gmine-data:/data \
  gelottohq/gmine:latest
```

### Building Your Own Image
```bash
docker build -t my-gmine-miner .
docker run -d --name my-miner my-gmine-miner
```

---

## Technical Overview

### Mining Process
1. **Epoch Management**: Mining occurs in epochs with three phases:
   - Commit Phase: Submit commitment hash of solution
   - Reveal Phase: Reveal the actual solution
   - Settlement Phase: Claim rewards

2. **Proof-of-Work**: Uses drillx (Equihash variant) algorithm
   - Memory-hard to prevent ASIC dominance
   - Difficulty adjusts based on network hashrate

3. **Reward System**: 
   - Miners earn POWER tokens for valid solutions
   - Proportional rewards based on mining score
   - Individual claim system (no batch gas issues)

### Architecture
- **`src/`**: Core mining engine and orchestration
- **`bridge/`**: Node.js EIP-712 signing service (legacy)
- **`gmine_mobile_lib/`**: Shared library with native Rust EIP-712
- **`proto/`**: Protobuf definitions for Injective

### Contract Addresses (Testnet)
- Mining Contract: `inj1mdq8lej6n35lp977w9nvc7mglwc3tqh5cms42y`
- Power Token: `inj1326k32dr7vjx5tnkuxlt58vkejj60r5ens29s8`

---

## Performance & Requirements

### System Requirements
- **CPU**: Any x86_64 processor (more cores = higher hashrate)
- **RAM**: 512MB minimum, 2GB recommended
- **Disk**: 100MB for binaries and state
- **Network**: Stable internet connection

### Performance Metrics
- **Hashrate**: ~600-1000 solutions/hour per core
- **Gas Usage**: ~$1.62/month at current prices
- **Power Efficiency**: Optimized for CPU mining

---

## Security Considerations

### ⚠️ Critical Security Notes

1. **Use a Dedicated Wallet**: 
   - Create a NEW wallet specifically for mining
   - Do NOT use your main wallet with significant funds
   - Only keep enough INJ for gas fees

2. **Private Key Security**:
   - Never share your mnemonic phrase
   - Never commit it to version control
   - Consider using environment variables or secure key management

3. **Verify Source Code**:
   - This is open source - review the code before running
   - Check for any modifications if building from forks

---

## Troubleshooting

### Common Issues

**"Mining binary not found"**
- Run: `cargo build --release --bin simple_miner`
- Ensure you're in the repository root directory

**"Bridge script not found" (Node.js mode)**
- Build the bridge: `cd bridge && npm install && npm run build`

**"Nonce out of range" errors**
- Switch to Rust signer: `./mine-rust.sh`
- This is a JavaScript precision limitation

**"No funds for gas"**
- Get testnet INJ: https://testnet.faucet.injective.network/
- Need minimum 0.5 INJ for gas fees

**Low hashrate or no solutions found**
- Increase workers: `--workers 4` (or number of CPU cores)
- Check CPU usage with `top` or `htop`

### Logs and Debugging
- Logs are saved in `mining_logs/` directory
- Enable verbose mode: `--verbose` or `-v`
- Check state file: `gmine_miner.state`

---

## Contributing

We welcome contributions! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Development Guidelines
- Run `cargo fmt` before committing
- Ensure `cargo clippy` passes
- Add tests for new functionality
- Update documentation as needed

---

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## Additional Resources

- **Main GMINE Repository**: https://github.com/Gelotto/gmine
- **Vendor Dependencies**: https://github.com/Gelotto/gmine-vendor
- **Telemetry Dashboard**: https://gmine.gelotto.io
- **Injective Testnet Faucet**: https://testnet.faucet.injective.network/

---

**GMINE: First legitimate proof-of-work implementation on CosmWasm** ⛏️