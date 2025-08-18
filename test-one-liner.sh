#!/bin/bash

# Quick test script to simulate the one-liner installation experience
# This tests the enhanced installer end-to-end

set -e

echo "🧪 Testing GMINE One-Liner Installation Experience"
echo ""
echo "This simulates what users will experience with:"
echo "  curl -fsSL https://raw.githubusercontent.com/Gelotto/gmine-rust-miner/main/install.sh | sh"
echo ""

# Clean up any previous test installation
if [ -d "$HOME/.gmine-test" ]; then
    echo "Cleaning up previous test installation..."
    rm -rf "$HOME/.gmine-test"
fi

# Set test installation directory
export GMINE_INSTALL_DIR="$HOME/.gmine-test"
echo "Installing to test directory: $GMINE_INSTALL_DIR"
echo ""

# Run the installer (from source since we're on feature branch)
echo "Running installer..."
./install.sh --from-source

echo ""
echo "✅ Installation complete!"
echo ""
echo "Test the enhanced features:"
echo "  1. Add to PATH: export PATH=\"$GMINE_INSTALL_DIR/bin:\$PATH\""
echo "  2. Run setup: gmine init"
echo "  3. Start mining: gmine mine"
echo "  4. Check status: gmine status"
echo ""
echo "To clean up test installation:"
echo "  rm -rf $GMINE_INSTALL_DIR"