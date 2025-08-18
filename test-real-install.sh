#!/bin/bash

# Test the real user installation experience

echo "Testing GMINE installer as a real user would experience it..."
echo ""

# Method 1: Download and run (interactive)
echo "Method 1: Download script first, then run (INTERACTIVE)"
echo "This is what happens when users follow proper installation:"
echo ""
curl -fsSL https://raw.githubusercontent.com/Gelotto/gmine-rust-miner/main/install.sh -o install-gmine.sh
chmod +x install-gmine.sh
./install-gmine.sh

# Clean up
rm -f install-gmine.sh