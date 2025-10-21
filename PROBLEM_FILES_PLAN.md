# Plan: Systematic Investigation of 8 Problem Files

## Methodology

For each problem file:
1. **Identify** the exact file causing the error
2. **Show** the error message from fix-no-pub-type
3. **Examine** the source file to understand the pattern
4. **Attempt** transformation (or skip if proof-only)
5. **Check** for string hacking in tool after any changes
6. **Compile** src/ only (isolated test)
7. **Compile** tests/ (check test impact)
8. **Run** tests (verify correctness)
9. **Compile** benches/ (check bench impact)
10. **Analyze** result (success/failure/not-supported)
11. **Document** findings and update tool if needed

## Problem Files Identified

### Chap21 (3 errors)
1. `Chap21/Exercise21_9.rs` - Proof-only module (expected failure)
2. `Chap21/Exercise21_6.rs` - Unknown pattern
3. `Chap21/Problem21_4.rs` - "Incomplete trait implementation"

### Chap41 (1 error)
4. `Chap41/???` - Unknown file (needs identification)

### Chap42 (1 error)
5. `Chap42/???` - Unknown file (needs identification)

### Chap43 (1 error)
6. `Chap43/???` - Unknown file (needs identification)

### Chap56 (2 errors)
7. `Chap56/???` - Unknown file (needs identification)
8. `Chap56/???` - Unknown file (needs identification)

## Execution Plan

### Phase 1: Identification (Files 1-8)
**Goal**: Get exact file names and error messages for all 8 files

**Steps**:
```bash
# For each chapter with errors, run fix-no-pub-type with full output
./target/debug/rusticate-fix-no-pub-type -d APAS-AI-copy/apas-ai/src/Chap21 2>&1 | grep "Error:"
./target/debug/rusticate-fix-no-pub-type -d APAS-AI-copy/apas-ai/src/Chap41 2>&1 | grep "Error:"
./target/debug/rusticate-fix-no-pub-type -d APAS-AI-copy/apas-ai/src/Chap42 2>&1 | grep "Error:"
./target/debug/rusticate-fix-no-pub-type -d APAS-AI-copy/apas-ai/src/Chap43 2>&1 | grep "Error:"
./target/debug/rusticate-fix-no-pub-type -d APAS-AI-copy/apas-ai/src/Chap56 2>&1 | grep "Error:"
```

### Phase 2: File 1 - Chap21/Exercise21_9.rs (Proof-only)
**Expected**: Mark as "not supported" without attempting transformation

**Steps**:
```bash
# 1. Examine the file
head -20 APAS-AI-copy/apas-ai/src/Chap21/Exercise21_9.rs

# 2. Verify it's proof-only (no code)
# Expected: Just documentation, no impl

# 3. Document as expected failure
# Action: None - tool correctly rejects this pattern

# 4. Mark as COMPLETE - Not Supported (by design)
```

### Phase 3: File 2 - Chap21/Exercise21_6.rs
**Expected**: Unknown pattern needs investigation

**Steps**:
```bash
# 1. Show the error
./target/debug/rusticate-fix-no-pub-type -f APAS-AI-copy/apas-ai/src/Chap21/Exercise21_6.rs

# 2. Examine the source file
cat APAS-AI-copy/apas-ai/src/Chap21/Exercise21_6.rs

# 3. Identify the pattern that's failing

# 4. Decide: Fix tool OR mark as not-supported

# IF FIX TOOL:
# 5. Modify fix_no_pub_type.rs
# 6. Check for string hacking
./target/debug/rusticate-review-string-hacking -f src/bin/fix_no_pub_type.rs

# 7. Compile the tool
cargo build --bin rusticate-fix-no-pub-type

# 8. Test on this one file
./target/debug/rusticate-fix-no-pub-type -f APAS-AI-copy/apas-ai/src/Chap21/Exercise21_6.rs

# 9. Compile src
cd APAS-AI-copy/apas-ai && cargo build --lib

# 10. Compile tests
cargo test --no-run

# 11. Run tests
cargo test

# 12. Compile benches
cargo bench --no-run

# 13. Reset APAS-AI-copy
git checkout -- .

# IF NOT SUPPORTED:
# Document why and mark as complete
```

### Phase 4: File 3 - Chap21/Problem21_4.rs
**Expected**: "Incomplete trait implementation" - needs investigation

**Steps**: (Same as File 2)

### Phase 5: Files 4-8 (Chap41, 42, 43, 56)
**Expected**: Unknown patterns

**Steps for each file**: (Same as File 2)

## Success Criteria

For each file, one of three outcomes:
1. ✅ **Fixed**: Tool updated, file transforms, all tests pass
2. ⚠️ **Not Supported**: Pattern documented as outside tool scope
3. 🔄 **Deferred**: Complex fix needed, documented for later

## Tracking Progress

| File | Chapter | Error Type | Status | Action Taken |
|------|---------|------------|--------|--------------|
| 1 | Chap21/Exercise21_9.rs | Proof-only | ⏳ | TBD |
| 2 | Chap21/Exercise21_6.rs | Unknown | ⏳ | TBD |
| 3 | Chap21/Problem21_4.rs | Incomplete trait | ⏳ | TBD |
| 4 | Chap41/??? | Unknown | ⏳ | TBD |
| 5 | Chap42/??? | Unknown | ⏳ | TBD |
| 6 | Chap43/??? | Unknown | ⏳ | TBD |
| 7 | Chap56/??? | Unknown | ⏳ | TBD |
| 8 | Chap56/??? | Unknown | ⏳ | TBD |

## Guard Rails

After ANY change to fix_no_pub_type.rs:
```bash
# 1. Check for string hacking (MANDATORY)
./target/debug/rusticate-review-string-hacking -f src/bin/fix_no_pub_type.rs

# 2. If violations found, MUST fix before proceeding
# NO EXCEPTIONS

# 3. Compile the tool
cargo build --bin rusticate-fix-no-pub-type

# 4. Only proceed if clean build + no string hacking
```

## Time Estimate

- Phase 1 (Identification): ~5 minutes
- File 1 (Proof-only): ~2 minutes (just document)
- Files 2-8 (Investigation + potential fixes): ~15-30 minutes each
- **Total**: ~2-4 hours for complete investigation

## Deliverables

1. **Updated tool** (if patterns can be supported)
2. **Documentation** of unsupported patterns
3. **Test results** for each successful transformation
4. **Updated TESTING_RESULTS.md** with detailed findings

## Exit Conditions

- All 8 files classified as: Fixed / Not Supported / Deferred
- No string hacking violations in tool
- Tool compiles cleanly
- Documentation complete

## Next Actions

**Ready to execute?** Start with Phase 1: Identification
```bash
cd /home/milnes/projects/rusticate && git checkout -b investigate-problem-files
```

Then proceed systematically through each file.
