#!/bin/bash

# GMINE Rust-Only Mining Script (Recommended for 24/7 Mining)
# Uses native Rust EIP-712 signer - no JavaScript precision issues
#
# This script is now just a wrapper for: gmine mine --use-rust-signer
# For interactive setup, run: gmine init

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

echo -e "${MAGENTA}=== GMINE Rust-Only Mining Client ===${NC}"
echo -e "${BLUE}Using native Rust EIP-712 signer (no Node.js bridge)${NC}"
echo ""

# Show key benefits
echo -e "${GREEN}Benefits of Rust-only mode:${NC}"
echo -e "  ✓ No JavaScript precision errors (handles nonces > 2^53-1)"
echo -e "  ✓ 10x faster transaction signing"
echo -e "  ✓ No 9MB Node.js runtime overhead"
echo -e "  ✓ Single process (no bridge child process)"
echo -e "  ✓ More reliable for 24/7 mining"
echo ""

# Forward all arguments to mine.sh with --rust-signer forced
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Check if mine.sh exists
if [ ! -f "$SCRIPT_DIR/mine.sh" ]; then
    echo -e "${RED}Error: mine.sh not found in $SCRIPT_DIR${NC}"
    echo -e "${YELLOW}Please ensure mine.sh is in the same directory as mine-rust.sh${NC}"
    exit 1
fi

# Always add --rust-signer flag
exec "$SCRIPT_DIR/mine.sh" --rust-signer "$@"