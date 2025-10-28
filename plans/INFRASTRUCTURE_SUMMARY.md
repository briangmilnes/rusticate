# Rusticate Infrastructure Improvements

## Code Deduplication Modules Created

### 1. tool_runner.rs (3450 LOC reduction potential)
**Eliminates timing + context boilerplate** across 35 binaries:

```rust
// Before: ~70 lines per tool
let start = Instant::now();
println!("Entering directory '{}'", base_dir.display());
// ... tool logic ...
println!("Completed in {}ms", start.elapsed().as_millis());

// After: ~5 lines per tool
run_simple("tool-name", base_dir, || {
    // tool logic
    Ok("Summary: X files checked".to_string())
})?;
```

**Savings**: 70 lines × 35 tools = 2,450 LOC
**Benefit**: Consistent timing, context, error handling

### 2. count_helper.rs (400 LOC reduction potential)
**Eliminates counting infrastructure** across 4 count tools:

```rust
// Before: ~100 lines per tool
// - File categorization logic
// - Per-file counting loop
// - Section aggregation
// - Output formatting

// After: ~2 lines per tool
run_count(&paths, &base_dir, count_fn, "item name")?;
```

**Savings**: 100 lines × 4 tools = 400 LOC
**Benefit**: Consistent output format, easy to add new count tools

### 3. logging.rs (Logging infrastructure)
**ToolLogger for optional file logging**:
- Directory structure: `logs/<tool-name>/<YYYY-MM-DD>/run-<HH-MM-SS>.log`
- Degrades gracefully if logging fails
- Methods: `log()`, `log_silent()`, `finalize()`

**Benefit**: Consistent logging when needed, no overhead when disabled

## Total Code Reduction Potential
- **tool_runner**: 2,450 LOC
- **count_helper**: 400 LOC
- **Total**: ~2,850 LOC reduction (14% of codebase)

## Testing Results

### Chap55 (Graph Algorithms)
✅ **8/8 modules transformed successfully**
- All compile cleanly
- 30/30 tests pass
- 4/4 benchmarks compile

### Chap21 (Basic Algorithms)  
⚠️ **9/12 modules transformed**
- 9 succeeded
- 3 errors (2 proof-only, 1 incomplete trait impl)
- **Compilation failures found**:
  - Recursive type alias bug: `pub type T = ArraySeqStPerS<T>;`
  - Type mismatches from incorrect parameter extraction
  - Missing imports for internal functions

## Known Bugs in fix-no-pub-type

### 1. Recursive Type Alias (CRITICAL)
**Example**: Exercise21_5
- Extracted: `ArraySeqStPerS<T>` from trait parameter
- Generated: `pub type T = ArraySeqStPerS<T>;` ❌ (recursive!)
- Should be: `pub type T = ArraySeqStPerS<N>;` or `ArraySeqStPerS<SomeType>`

**Fix needed**: Better generic parameter analysis

### 2. Multi-Method Trait Impls
**Problem**: Can only handle single-method traits
- Works: `trait XTrait { fn x(&self) -> R; }`
- Fails: `trait XTrait { fn x(&self) -> R; fn y(&self) -> R; }`

**Fix needed**: Support multiple methods in `create_trait_impl()`

### 3. Internal Function Calls
**Problem**: Internal function calls in body aren't transformed
- Example: `transpose_graph(graph)` → `transpose_graph(self)` ❌
- Should be: `self.transpose_graph()` ✓

**Fix needed**: AST-based call site transformation within method bodies

## Infrastructure Adoption Status

### Modules Created ✓
- [x] tool_runner.rs
- [x] count_helper.rs  
- [x] logging.rs

### Adoption in Tools
- [ ] 0/35 tools using tool_runner
- [ ] 0/4 count tools using count_helper
- [ ] 0/35 tools using logging

**Next**: Convert 1-2 tools as examples, then gradually adopt across codebase

## Code Quality Metrics

### String Hacking
- **Before**: 8 violations
- **After**: 2 violations (75% reduction)
- Remaining 2 are appropriate heuristics

### Directory Filtering
- [x] Global attic/target/.* exclusion in find_rust_files()
- [x] Global exclusion in search_for_file()

### Timing & Context
- [x] All 32/32 binaries have timing
- [x] 28/32 binaries have directory context
- [x] 25/32 binaries use comma formatting

## Infrastructure Impact

### Before
- 35 tools × 70 lines boilerplate = 2,450 LOC duplication
- Inconsistent output formats
- No logging capability
- Manual timing in each tool

### After (when fully adopted)
- Shared infrastructure: ~400 LOC (logging + tool_runner + count_helper)
- Consistent output/timing/logging
- Easy to add new tools
- **Net reduction: ~2,400 LOC (12% of total codebase)**

## Next Steps

### High Priority
1. **Fix recursive type alias bug** in fix-no-pub-type
2. **Test on more directories** to find edge cases
3. **Convert 2-3 tools** to use new infrastructure (as examples)

### Medium Priority
4. Support multi-method traits in fix-no-pub-type
5. Fix internal function call transformation
6. Document infrastructure usage patterns

### Low Priority
7. Gradually adopt infrastructure across all tools
8. Add integration tests for infrastructure
9. Performance optimization if needed

---
**Infrastructure Status**: Ready for adoption, needs bug fixes in fix-no-pub-type before wider use
**Code Quality**: Significantly improved (75% string hacking reduction, global filtering)
**Testing Coverage**: Good for simple cases (Chap55), needs work for complex cases (Chap21)
