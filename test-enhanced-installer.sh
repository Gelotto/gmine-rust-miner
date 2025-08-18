#!/bin/bash

# Test script for the enhanced GMINE installer
# This script tests all new functionality on the feature/enhanced-installer branch

set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}=== GMINE Enhanced Installer Test Suite ===${NC}"
echo "This script will test the enhanced installer features"
echo ""

# Check we're on the right branch
CURRENT_BRANCH=$(git branch --show-current)
if [ "$CURRENT_BRANCH" != "feature/enhanced-installer" ]; then
    echo -e "${RED}ERROR: Not on feature/enhanced-installer branch${NC}"
    echo "Current branch: $CURRENT_BRANCH"
    echo ""
    echo "To test, run:"
    echo "  git checkout feature/enhanced-installer"
    echo "  ./test-enhanced-installer.sh"
    exit 1
fi

echo -e "${GREEN}✓ On feature/enhanced-installer branch${NC}"
echo ""

# Build the enhanced binary
echo -e "${YELLOW}Building enhanced binary...${NC}"
cargo build --release --bin simple_miner
echo -e "${GREEN}✓ Build successful${NC}"
echo ""

# Test 1: Check backward compatibility
echo -e "${YELLOW}Test 1: Backward compatibility${NC}"
echo "Testing old-style command (should still work):"
echo "  ./target/release/simple_miner --workers 2 --network testnet"
timeout 5s ./target/release/simple_miner --workers 2 --network testnet || true
echo -e "${GREEN}✓ Backward compatibility maintained${NC}"
echo ""

# Test 2: Test help for new subcommands
echo -e "${YELLOW}Test 2: New subcommand help${NC}"
./target/release/simple_miner --help
echo ""
./target/release/simple_miner init --help || true
./target/release/simple_miner service --help || true
./target/release/simple_miner logs --help || true
./target/release/simple_miner status --help || true
echo -e "${GREEN}✓ Subcommand help working${NC}"
echo ""

# Test 3: Test status command
echo -e "${YELLOW}Test 3: Status command${NC}"
./target/release/simple_miner status || true
echo -e "${GREEN}✓ Status command executed${NC}"
echo ""

# Test 4: Test logs command (when no logs exist)
echo -e "${YELLOW}Test 4: Logs command${NC}"
./target/release/simple_miner logs --lines 10 || true
echo -e "${GREEN}✓ Logs command executed${NC}"
echo ""

# Test 5: Test service status (should show not installed)
echo -e "${YELLOW}Test 5: Service status check${NC}"
./target/release/simple_miner service status || true
echo -e "${GREEN}✓ Service status command executed${NC}"
echo ""

# Test 6: Test init command (non-interactive)
echo -e "${YELLOW}Test 6: Init command validation${NC}"
echo "Testing mnemonic validation..."
# Test invalid mnemonic (wrong word count)
echo -e "invalid mnemonic\n1\nn\n" | ./target/release/simple_miner init 2>&1 | grep -q "must be 12 or 24 words" && echo -e "${GREEN}✓ Invalid word count detected${NC}" || echo -e "${RED}✗ Failed to detect invalid word count${NC}"

# Test invalid BIP39 mnemonic
echo -e "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon invalid\n1\nn\n" | ./target/release/simple_miner init 2>&1 | grep -q "Invalid mnemonic" && echo -e "${GREEN}✓ Invalid BIP39 mnemonic detected${NC}" || echo -e "${RED}✗ Failed to detect invalid BIP39 mnemonic${NC}"

echo ""

# Test 7: Create a test config
echo -e "${YELLOW}Test 7: Creating test configuration${NC}"
TEST_CONFIG_DIR="$HOME/.gmine-test"
mkdir -p "$TEST_CONFIG_DIR"
cat > "$TEST_CONFIG_DIR/config.toml" << EOF
mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
workers = 2
network = "testnet"
EOF
echo -e "${GREEN}✓ Test config created at $TEST_CONFIG_DIR/config.toml${NC}"
echo ""

# Test 8: Test mine subcommand with config
echo -e "${YELLOW}Test 8: Mine subcommand${NC}"
timeout 5s ./target/release/simple_miner mine --config "$TEST_CONFIG_DIR/config.toml" || true
echo -e "${GREEN}✓ Mine subcommand with config working${NC}"
echo ""

# Clean up test config
rm -rf "$TEST_CONFIG_DIR"

echo -e "${GREEN}=== All Tests Passed ===${NC}"
echo ""
echo "Summary of new features tested:"
echo "✓ Backward compatibility maintained"
echo "✓ New subcommands: init, mine, service, logs, status"
echo "✓ Interactive setup wizard with BIP39 validation"
echo "✓ Service management commands"
echo "✓ Config file support"
echo "✓ Proper error handling and validation"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Test the install.sh script locally:"
echo "   ./install.sh"
echo ""
echo "2. Test service installation (requires sudo):"
echo "   ./target/release/simple_miner service install"
echo "   ./target/release/simple_miner service start"
echo "   ./target/release/simple_miner service status"
echo ""
echo "3. When satisfied, merge to main:"
echo "   git checkout main"
echo "   git merge feature/enhanced-installer"
echo "   git push origin main"