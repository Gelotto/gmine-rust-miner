# Testing the Enhanced Installer - One Line!

## Quick Test (30 seconds)

Just run this one command to test everything:

```bash
./test-one-liner.sh
```

This will:
1. Build and install gmine to a test directory
2. Prompt you to run the setup wizard
3. Show you the new commands available

## What to Test After Installation

Once installed, test these features:

```bash
# Add test installation to PATH
export PATH="$HOME/.gmine-test/bin:$PATH"

# Test interactive setup (try invalid mnemonics to test validation)
gmine init

# Test backward compatibility
gmine --workers 2

# Test new subcommands
gmine status
gmine mine --debug
```

## Cleanup

```bash
rm -rf ~/.gmine-test
```

## Ready to Ship?

If everything works, merge to main:

```bash
git checkout main
git merge feature/enhanced-installer
git push origin main
```

That's it! The one-liner installer will automatically use the enhanced version.