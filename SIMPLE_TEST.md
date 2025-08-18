# Testing the One-Liner Install

## Step 1: Merge to main first
```bash
git checkout main
git merge feature/enhanced-installer
git push origin main
```

## Step 2: Test the actual one-liner
```bash
# Clean any existing installation
rm -rf ~/.gmine

# Run the one-liner (just like users will)
curl -fsSL https://raw.githubusercontent.com/Gelotto/gmine-rust-miner/main/install.sh | sh
```

## What happens:
1. Downloads and installs `gmine` to `~/.gmine/bin`
2. Prompts: "Would you like to set up your miner now?"
3. If you say yes, runs the interactive setup wizard
4. Tells you to add gmine to your PATH

## That's it!

After installation, users can:
- `gmine init` - Run setup wizard
- `gmine mine` - Start mining
- `gmine service install` - Install as service
- `gmine status` - Check if running

The enhanced features are all built into the binary that gets installed by the one-liner.