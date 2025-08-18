#!/bin/bash

echo "=== Verifying GMINE Installation ==="
echo ""

# Check if gmine is in PATH
if command -v gmine >/dev/null 2>&1; then
    echo "✅ gmine is in PATH"
    GMINE_CMD="gmine"
else
    echo "❌ gmine is NOT in PATH"
    echo ""
    echo "Checking ~/.gmine/bin/gmine directly..."
    if [ -f "$HOME/.gmine/bin/gmine" ]; then
        echo "✅ Binary exists at ~/.gmine/bin/gmine"
        GMINE_CMD="$HOME/.gmine/bin/gmine"
    else
        echo "❌ Binary not found!"
        exit 1
    fi
fi

echo ""
echo "=== Checking Binary Features ==="
"$GMINE_CMD" --help | grep -E "(init|mine|service|logs|status)" || echo "No subcommands found"

echo ""
echo "=== Testing Interactive Setup ==="
echo "Running: $GMINE_CMD init"
echo "(This should prompt for mnemonic...)"
echo ""

# Add to PATH for this session if needed
export PATH="$HOME/.gmine/bin:$PATH"

# The issue is that 'curl | sh' is non-interactive by design
# To test properly, you need to either:
# 1. Run 'gmine init' directly (after adding to PATH)
# 2. Download and run the installer script (not piped)