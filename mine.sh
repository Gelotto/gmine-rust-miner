#!/bin/bash

# GMINE Mining Script with Node.js Bridge (Legacy)
# For 24/7 mining, use mine-rust.sh or run: gmine mine --use-rust-signer
# This script uses the Node.js bridge which has JavaScript precision limitations

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
DEFAULT_MNEMONIC="abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
DEFAULT_WORKERS=1
DEFAULT_NETWORK="testnet"
LOG_DIR="mining_logs"
STATE_FILE="gmine_miner.state"

# Help message
show_help() {
    cat << EOF
GMINE Production Mining Script

USAGE:
    ./mine.sh [OPTIONS]

OPTIONS:
    -m, --mnemonic MNEMONIC    Wallet mnemonic phrase (24 words)
    -w, --workers COUNT        Number of mining workers (default: 1)
    -n, --network NETWORK      Network: testnet or mainnet (default: testnet)
    -d, --duration SECONDS     Mining duration in seconds (default: unlimited)
    -l, --log-file FILE        Custom log file path
    -s, --state-file FILE      Custom state file path
    -v, --verbose              Enable debug logging
    -r, --rust-signer          Use Rust-native EIP-712 signer (experimental)
    -h, --help                 Show this help message

EXAMPLES:
    # Mine with default settings (testnet, 1 worker)
    ./mine.sh

    # Mine with custom mnemonic
    ./mine.sh -m "your wallet mnemonic phrase here..."
    
    # Mine with 4 workers for 1 hour
    ./mine.sh -w 4 -d 3600
    
    # Mine with debug logging
    ./mine.sh -v
    
    # Mine with Rust-native EIP-712 signer (experimental)
    ./mine.sh -r

NOTE:
    For EIP-712 bridge to work properly, run from gmine-miner directory:
    cd gmine-miner && ../mine.sh
    
    Or use the direct binary approach:
    cd gmine-miner && RUST_LOG=info ../target/release/simple_miner --mnemonic "..." --network testnet --workers 1

ENVIRONMENT VARIABLES:
    MNEMONIC                   Wallet mnemonic (alternative to -m)
    GMINE_WORKERS              Number of workers (alternative to -w)
    GMINE_NETWORK              Network selection (alternative to -n)

REAL DATA SOURCES:
    - Blockchain: Live Injective testnet (testnet.sentry.chain.grpc.injective.network:443)
    - Contracts: Real deployed contracts on testnet
    - Mining: inj1h2rq8q2ly6mwgwv4jcd5qpjvfqwvwee5v9n032 (V3.4 with JIT History fix)
    - Power Token: inj18ju0hzpu5ylz0nh7dcwcrssnh8aq4vdhmnr2vg
    - Telemetry: Production backend at https://gmine.gelotto.io
    - EIP-712 Bridge: Node.js service for proper Injective signing

EOF
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -m|--mnemonic)
                MNEMONIC="$2"
                shift 2
                ;;
            -w|--workers)
                WORKERS="$2"
                shift 2
                ;;
            -n|--network)
                NETWORK="$2"
                shift 2
                ;;
            -d|--duration)
                DURATION="$2"
                shift 2
                ;;
            -l|--log-file)
                LOG_FILE="$2"
                shift 2
                ;;
            -s|--state-file)
                STATE_FILE="$2"
                shift 2
                ;;
            -v|--verbose)
                VERBOSE=true
                shift
                ;;
            -r|--rust-signer)
                USE_RUST_SIGNER=true
                shift
                ;;
            -h|--help)
                show_help
                exit 0
                ;;
            *)
                echo -e "${RED}Error: Unknown option $1${NC}"
                echo "Use --help for usage information"
                exit 1
                ;;
        esac
    done
}

# Set defaults
MNEMONIC="${MNEMONIC:-$DEFAULT_MNEMONIC}"
WORKERS="${GMINE_WORKERS:-${WORKERS:-$DEFAULT_WORKERS}}"
NETWORK="${GMINE_NETWORK:-${NETWORK:-$DEFAULT_NETWORK}}"
VERBOSE="${VERBOSE:-false}"
USE_RUST_SIGNER="${USE_RUST_SIGNER:-false}"

# Validate inputs
validate_inputs() {
    # Find the mining binary in various locations
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    
    # Based on the mining client requirements, we need to run from gmine-miner directory
    # for the bridge integration to work correctly
    if [[ -f "./target/release/simple_miner" && -d "./bridge" ]]; then
        # Running from gmine-rust-miner directory
        MINER_BINARY="./target/release/simple_miner"
        BRIDGE_BASE="./bridge"     # bridge is now local to this repo
        MINER_WORKDIR="."
    else
        echo -e "${RED}Error: Mining binary or bridge not found${NC}"
        echo "Current directory: $(pwd)"
        echo "Binary check: ./target/release/simple_miner exists: $(test -f './target/release/simple_miner' && echo 'YES' || echo 'NO')"
        echo "Bridge check: ./bridge exists: $(test -d './bridge' && echo 'YES' || echo 'NO')"
        echo "Miner dir check: ./gmine-miner exists: $(test -d './gmine-miner' && echo 'YES' || echo 'NO')"
        echo ""
        echo "Please run: cargo build --release --bin simple_miner"
        echo "And ensure bridge is built: cd bridge && npm run build"
        exit 1
    fi
    
    # Verify bridge exists only if not using Rust signer
    if [[ "$USE_RUST_SIGNER" != "true" ]]; then
        BRIDGE_CHECK_PATH="$MINER_WORKDIR/$BRIDGE_BASE/dist/index.js"
        if [[ ! -f "$BRIDGE_CHECK_PATH" ]]; then
            echo -e "${RED}Error: Bridge script not found at $BRIDGE_CHECK_PATH${NC}"
            echo "Please build the bridge:"
            echo "  cd bridge && npm run build"
            exit 1
        fi
    fi
    
    echo -e "${BLUE}Using binary: $MINER_BINARY${NC}"

    # Validate network
    if [[ "$NETWORK" != "testnet" && "$NETWORK" != "mainnet" ]]; then
        echo -e "${RED}Error: Network must be 'testnet' or 'mainnet'${NC}"
        exit 1
    fi

    # Check if mainnet is available
    if [[ "$NETWORK" = "mainnet" ]]; then
        echo -e "${RED}Error: Mainnet not yet supported${NC}"
        exit 1
    fi

    # Validate workers
    if [[ ! "$WORKERS" =~ ^[1-9][0-9]*$ ]]; then
        echo -e "${RED}Error: Workers must be a positive integer${NC}"
        exit 1
    fi

    # Validate duration if provided
    if [[ -n "$DURATION" && ! "$DURATION" =~ ^[1-9][0-9]*$ ]]; then
        echo -e "${RED}Error: Duration must be a positive integer (seconds)${NC}"
        exit 1
    fi

    # Validate mnemonic word count
    WORD_COUNT=$(echo "$MNEMONIC" | wc -w)
    if [[ "$WORD_COUNT" -ne 12 && "$WORD_COUNT" -ne 24 ]]; then
        echo -e "${RED}Error: Mnemonic must be 12 or 24 words${NC}"
        exit 1
    fi
}

# Setup logging
setup_logging() {
    # Get original working directory for log file
    ORIGINAL_DIR="$(pwd)"
    mkdir -p "$ORIGINAL_DIR/$LOG_DIR"
    
    if [[ -z "$LOG_FILE" ]]; then
        TIMESTAMP=$(date +%Y%m%d_%H%M%S)
        LOG_FILE="$ORIGINAL_DIR/$LOG_DIR/mining_$TIMESTAMP.log"
    else
        # Make log file path absolute if it's relative
        if [[ "$LOG_FILE" != /* ]]; then
            LOG_FILE="$ORIGINAL_DIR/$LOG_FILE"
        fi
    fi
    
    echo -e "${BLUE}Log file: $LOG_FILE${NC}"
}

# Display configuration
show_config() {
    echo -e "${GREEN}=== GMINE Mining Configuration ===${NC}"
    echo -e "Network: ${YELLOW}$NETWORK${NC}"
    echo -e "Workers: ${YELLOW}$WORKERS${NC}"
    echo -e "State file: ${YELLOW}$STATE_FILE${NC}"
    echo -e "Duration: ${YELLOW}${DURATION:-unlimited}${NC}"
    echo -e "Verbose: ${YELLOW}$VERBOSE${NC}"
    if [[ "$USE_RUST_SIGNER" = "true" ]]; then
        echo -e "EIP-712 Signer: ${YELLOW}Rust-native (experimental)${NC}"
    else
        echo -e "EIP-712 Signer: ${YELLOW}Node.js bridge${NC}"
    fi
    echo ""
    echo -e "${GREEN}=== Real Contract Addresses (V2 - Fixed) ===${NC}"
    
    if [[ "$NETWORK" = "testnet" ]]; then
        echo -e "Mining Contract: ${YELLOW}inj1h2rq8q2ly6mwgwv4jcd5qpjvfqwvwee5v9n032${NC} (V3.4 with JIT History fix)"
        echo -e "Power Token: ${YELLOW}inj18ju0hzpu5ylz0nh7dcwcrssnh8aq4vdhmnr2vg${NC} (V3.4 new power token)"
        echo -e "gRPC Endpoint: ${YELLOW}https://testnet.sentry.chain.grpc.injective.network:443${NC}"
        echo -e "Telemetry: ${YELLOW}https://gmine.gelotto.io${NC}"
        echo ""
        echo -e "${GREEN}=== Recent Fixes Applied ===${NC}"
        echo -e "✅ Nonce wraparound bug fixed - workers stay in assigned partition"
        echo -e "✅ Gas limit increased to 400k for claim transactions"
        echo -e "✅ Proper 3-step reward process: advance → finalize → claim"
    fi
    echo ""
}

# Build mining command
build_command() {
    # Get original working directory for state file
    ORIGINAL_DIR="$(pwd)"
    
    # Make state file path absolute if it's relative
    if [[ "$STATE_FILE" != /* ]]; then
        ABS_STATE_FILE="$ORIGINAL_DIR/$STATE_FILE"
    else
        ABS_STATE_FILE="$STATE_FILE"
    fi
    
    MINER_ARGS=()
    MINER_ARGS+=("--mnemonic" "$MNEMONIC")
    MINER_ARGS+=("--network" "$NETWORK")
    MINER_ARGS+=("--workers" "$WORKERS")
    MINER_ARGS+=("--state-file" "$ABS_STATE_FILE")
    
    if [[ "$USE_RUST_SIGNER" = "true" ]]; then
        MINER_ARGS+=("--use-rust-signer")
    fi
    
    if [[ "$VERBOSE" = "true" ]]; then
        MINER_ARGS+=("--debug")
        export RUST_LOG="debug"
    else
        export RUST_LOG="info"
    fi
}

# Monitor mining progress
monitor_mining() {
    local log_file="$1"
    local start_time=$(date +%s)
    
    echo -e "${BLUE}Monitoring mining progress...${NC}"
    echo "Press Ctrl+C to stop mining"
    echo ""
    
    # Monitor in background
    (
        sleep 10  # Give mining time to start
        while kill -0 $MINING_PID 2>/dev/null; do
            if [[ -f "$log_file" ]]; then
                # Check for nonce range assignment (shows partition is working)
                if tail -n 100 "$log_file" | grep -q "Calculated Blake2b512 nonce range"; then
                    echo -e "${BLUE}[$(date)] ℹ Nonce partition assigned${NC}"
                    tail -n 100 "$log_file" | grep "Calculated Blake2b512 nonce range" | tail -1
                fi
                
                # Check for solutions found
                if tail -n 50 "$log_file" | grep -q "Found solution for epoch"; then
                    echo -e "${YELLOW}[$(date)] ⚡ Solution found!${NC}"
                    tail -n 50 "$log_file" | grep "Found solution for epoch" | tail -1
                fi
                
                # Check for successful reveals
                if tail -n 50 "$log_file" | grep -q "Successfully revealed"; then
                    echo -e "${GREEN}[$(date)] ✓ REVEAL SUCCESS!${NC}"
                    tail -n 50 "$log_file" | grep "Successfully revealed" | tail -1
                fi
                
                # Check for successful claims (this means POWER earned!)
                if tail -n 50 "$log_file" | grep -q "Successfully claimed"; then
                    echo -e "${GREEN}[$(date)] 💰 POWER TOKENS EARNED!${NC}"
                    tail -n 50 "$log_file" | grep "Successfully claimed" | tail -1
                fi
                
                # Check for any nonce range errors (should not happen with fix)
                if tail -n 50 "$log_file" | grep -q "Nonce out of range"; then
                    echo -e "${RED}[$(date)] ⚠ WARNING: Nonce out of range detected (should be fixed!)${NC}"
                    tail -n 50 "$log_file" | grep "Nonce out of range" | tail -1
                fi
                
                # Check for Rust signer status
                if tail -n 50 "$log_file" | grep -q "Rust-native EIP-712 signer"; then
                    echo -e "${BLUE}[$(date)] 🦀 Using Rust-native EIP-712 signer${NC}"
                    tail -n 50 "$log_file" | grep "Rust-native EIP-712 signer" | tail -1
                fi
            fi
            sleep 30  # Check every 30 seconds
        done
    ) &
    MONITOR_PID=$!
}

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Stopping mining...${NC}"
    
    # Kill monitor if running
    if [[ -n "$MONITOR_PID" ]]; then
        kill $MONITOR_PID 2>/dev/null || true
    fi
    
    # Kill mining process if running
    if [[ -n "$MINING_PID" ]]; then
        kill $MINING_PID 2>/dev/null || true
        wait $MINING_PID 2>/dev/null || true
    fi
    
    echo -e "${BLUE}Mining stopped. State saved to: $ABS_STATE_FILE${NC}"
    echo -e "${BLUE}Logs available at: $LOG_FILE${NC}"
    
    # Show final stats if available
    if [[ -f "$ABS_STATE_FILE" ]]; then
        if command -v jq >/dev/null 2>&1; then
            EPOCH=$(jq -r '.epoch // "unknown"' "$ABS_STATE_FILE" 2>/dev/null || echo "unknown")
            PHASE=$(jq -r '.phase // "unknown"' "$ABS_STATE_FILE" 2>/dev/null || echo "unknown")
            echo -e "${BLUE}Final state: Epoch $EPOCH, Phase: $PHASE${NC}"
        fi
    fi
}

# Main execution
main() {
    parse_args "$@"
    validate_inputs
    setup_logging
    show_config
    
    # Set up signal handlers
    trap cleanup EXIT
    trap cleanup INT
    trap cleanup TERM
    
    # Build command arguments
    build_command
    
    echo -e "${GREEN}=== Starting GMINE Mining ===${NC}"
    echo -e "Binary: ${YELLOW}$MINER_BINARY${NC}"
    echo -e "Args: ${YELLOW}${MINER_ARGS[*]}${NC}"
    echo -e "Log Level: ${YELLOW}$RUST_LOG${NC}"
    echo -e "Working Dir: ${YELLOW}$MINER_WORKDIR${NC}"
    echo -e "Bridge Path: ${YELLOW}$BRIDGE_BASE${NC}"
    echo ""
    
    # Change to the correct working directory for the miner
    cd "$MINER_WORKDIR"
    
    # Export bridge base for the mining client to find
    export BRIDGE_BASE="$BRIDGE_BASE"
    
    # Start mining with logging and optional timeout
    if [[ -n "$DURATION" ]]; then
        timeout "$DURATION" "$MINER_BINARY" "${MINER_ARGS[@]}" 2>&1 | tee "$LOG_FILE" &
    else
        "$MINER_BINARY" "${MINER_ARGS[@]}" 2>&1 | tee "$LOG_FILE" &
    fi
    MINING_PID=$!
    
    # Start monitoring
    monitor_mining "$LOG_FILE"
    
    # Wait for mining process
    wait $MINING_PID
    EXIT_CODE=$?
    
    # Handle exit codes
    if [[ $EXIT_CODE -eq 124 ]]; then
        echo -e "${YELLOW}Mining completed (timeout reached)${NC}"
    elif [[ $EXIT_CODE -eq 0 ]]; then
        echo -e "${GREEN}Mining completed successfully${NC}"
    else
        echo -e "${RED}Mining exited with error code: $EXIT_CODE${NC}"
    fi
    
    return $EXIT_CODE
}

# Run main function with all arguments
main "$@"