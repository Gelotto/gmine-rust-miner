# GMINE Orchestrator Fix - Duplicate Commit Issue

## Date: 2025-09-03

### Problem Identified
The user reported only earning 20 POWER once and then nothing after that, despite being the only miner. The logs showed repeated "Already committed for epoch 53" errors.

### Root Cause
The orchestrator was not tracking which epochs it had already committed to. After successfully claiming rewards for an epoch, it would transition back to idle state and immediately try to mine the same epoch again, resulting in:
1. "Already committed" errors from the contract
2. Wasted mining cycles
3. Missing opportunities to mine new epochs

### Fix Implemented
Added epoch commitment tracking to the orchestrator:

1. **Added `committed_epochs` field to `MiningState`**
   - Tracks epochs we've successfully committed to
   - Persists across restarts (saved to state file)
   - Automatically prunes old entries to prevent unbounded growth

2. **Check before starting mining**
   - When in idle state and seeing a commit phase, check if we've already committed
   - If already committed, wait for next epoch instead of mining again

3. **Track successful commits**
   - When a commit succeeds, add the epoch to the tracking list
   - Keeps only the last 20 epochs to prevent memory issues

4. **Handle missed reveal windows**
   - When we miss a reveal window and move to a new epoch, check if already committed
   - Prevents duplicate mining attempts

### Testing the Fix
```bash
# Stop any running miner
pkill -f simple_miner

# Run the updated miner
cd /home/haquem/gelotto/gmine-rust-miner
./mine-rust.sh -m "your mnemonic here" > miner-log.txt 2>&1 &

# Monitor the logs
tail -f miner-log.txt | grep -E "(Already committed|Successfully committed|earned|POWER)"
```

### Expected Behavior After Fix
- Miner will commit once per epoch
- After claiming rewards, it will wait for the next epoch instead of trying to re-commit
- No more "Already committed" errors
- Continuous earning of POWER tokens (20 per epoch as the only miner)

### Code Changes
File: `src/orchestrator/mod.rs`
- Added `committed_epochs: Vec<u64>` to `MiningState` struct
- Added check in idle state before starting mining
- Added tracking on successful commit
- Added check when handling missed reveal windows