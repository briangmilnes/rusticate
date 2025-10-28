# Fix Plan: review-test-functions (96.3% False Positive Rate → <5%)

## Executive Summary

**Current State**: Tool reports 194 functions as "NO TEST COVERAGE", but 184 (96.3%) are actually tested.

**Root Cause**: Tool only detects direct textual function calls like `function_name()`. It misses:
- Trait methods called via operators (`==`, `+`, `-`)
- Trait methods called via macros (`format!`, `println!`, `assert_eq!`)
- Internal helpers called from tested public methods (transitive coverage)

**Solution**: Enhance AST analysis to detect trait implementations, operator/macro usage, and build call graphs.

---

## Problem Categories

### Category 1: Display/Debug fmt() - 107 functions (56%)

**Example:**
```rust
// src/Chap65/PrimStEph.rs
impl Display for PQEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult { ... }  // ← Tool says "NO TEST COVERAGE"
}

// tests/Chap65/TestPrimStEph.rs
println!("{}", entry);  // ← This DOES call fmt(), but tool doesn't see it!
```

**Fix:**
1. Parse `impl Display for Type` and `impl Debug for Type` blocks (AST: `SyntaxKind::IMPL`)
2. Extract the type name (e.g., `PQEntry`)
3. In test files, search for:
   - `format!("{}", expr)` where `expr` has type `Type`
   - `println!("{}", expr)` where `expr` has type `Type`
   - `format!("{:?}", expr)` for Debug
4. Mark `Type::fmt()` as tested if found

**AST Approach:**
- Traverse `MACRO_CALL` nodes with path = `format`, `println`, `eprintln`
- Extract token tree and look for `{}` or `{:?}` format specifiers
- Extract the variable/expression being formatted
- Match against known Display/Debug implementations

### Category 2: Eq/PartialEq eq() - 33 functions (17%)

**Example:**
```rust
// src/Chap18/ArraySeq.rs
impl PartialEq for ArraySeqS<T> {
    fn eq(&self, other: &Self) -> bool { ... }  // ← Tool says "NO TEST COVERAGE"
}

// tests/Chap18/TestArraySeq.rs
assert_eq!(seq1, seq2);  // ← This DOES call eq(), but tool doesn't see it!
if seq1 == seq2 { ... }  // ← Same here
```

**Fix:**
1. Parse `impl PartialEq for Type` and `impl Eq for Type` blocks
2. Extract the type name
3. In test files, search for:
   - `assert_eq!(a, b)` macro calls
   - `assert_ne!(a, b)` macro calls
   - Binary expressions with `==` or `!=` operators (AST: `SyntaxKind::BIN_EXPR`)
4. Extract types of operands and mark as tested

**AST Approach:**
- Traverse `MACRO_CALL` nodes with path = `assert_eq`, `assert_ne`
- Traverse `BIN_EXPR` nodes with operator = `==`, `!=`
- Extract variable names and match to types

### Category 3: Internal Helpers - 45 functions (24%)

**Example:**
```rust
// src/Chap06/DirGraphMtEph.rs
pub fn ng_of_vertices(&self, vs: &[Vertex]) -> ArraySeqMtEph<Vertex> {
    self.parallel_ng_of_vertices(vs)  // ← Calls helper
}

fn parallel_ng_of_vertices(&self, vs: &[Vertex]) -> ArraySeqMtEph<Vertex> {
    // ← Tool says "NO TEST COVERAGE" but it IS tested via ng_of_vertices()!
}

// tests/Chap06/TestDirGraphMtEph.rs
graph.ng_of_vertices(&vertices);  // ← This transitively tests parallel_ng_of_vertices()
```

**Fix:**
1. Build intra-module call graph:
   - Parse all functions in a module
   - Detect `CALL_EXPR` and `METHOD_CALL_EXPR` within each function body
   - Track: `public_method()` calls `private_helper()`
2. Propagate coverage:
   - If `public_method()` has test coverage
   - AND `public_method()` calls `private_helper()`
   - THEN mark `private_helper()` as "tested (transitively)"

**Detection Patterns:**
- Functions named `parallel_*`, `*_rec`, `*_impl`, `*_internal`
- Functions without `pub` keyword
- Functions only called from within same module

### Category 4: Operator Traits - 4 functions (2%)

**Example:**
```rust
// src/Chap50/Probability.rs
impl Add for Probability {
    fn add(self, other: Self) -> Self { ... }  // ← Tool says "NO TEST COVERAGE"
}

// tests/Chap50/TestProbability.rs
let sum = p1 + p2;  // ← This DOES call add(), but tool doesn't see it!
```

**Fix:**
1. Parse `impl Add/Sub/Mul/Div for Type`
2. In test files, search for binary operators `+`, `-`, `*`, `/`
3. Extract operand types and mark trait methods as tested

---

## Implementation Phases

### Phase 1: Trait Implementation Detection
**File:** `src/bin/review_test_functions.rs`

Add new data structures:
```rust
struct TraitImpl {
    trait_name: String,        // "Display", "PartialEq", "Add"
    type_name: String,         // "PQEntry", "ArraySeqS"
    method_name: String,       // "fmt", "eq", "add"
    file: PathBuf,
    line: usize,
}

fn find_trait_implementations(file: &Path, parsed: &SourceFile) -> Vec<TraitImpl> {
    // Parse: impl TraitName for TypeName { fn method_name(...) }
    // Return list of trait implementations
}
```

**AST Traversal:**
- Find `SyntaxKind::IMPL` nodes
- Check if `impl_def.trait_().is_some()` (trait impl vs inherent impl)
- Extract trait name: `impl_def.trait_().unwrap().syntax().to_string()`
- Extract type name: `impl_def.self_ty().unwrap().syntax().to_string()`
- Extract method names from `impl_def.assoc_item_list()`

### Phase 2: Display/Debug Detection in Tests
**File:** `src/bin/review_test_functions.rs`

Add new function:
```rust
fn find_format_macro_calls(file: &Path, parsed: &SourceFile, trait_impls: &[TraitImpl]) -> HashMap<String, usize> {
    // Search for format!("{}", x), println!("{}", x)
    // Extract variable name 'x'
    // Match to trait_impls where trait_name = "Display"
    // Return: { "Type::fmt" -> call_count }
}
```

**AST Traversal:**
- Find `SyntaxKind::MACRO_CALL` nodes
- Check if macro path is `format`, `println`, `eprintln`, `write`, `writeln`
- Extract token tree and search for:
  - `{}` tokens (Display)
  - `{:?}` tokens (Debug)
- Extract the NEXT identifier after the format string
- Match identifier to known types

### Phase 3: Operator Detection in Tests
**File:** `src/bin/review_test_functions.rs`

Add new function:
```rust
fn find_operator_usage(file: &Path, parsed: &SourceFile, trait_impls: &[TraitImpl]) -> HashMap<String, usize> {
    // Search for: a == b, a != b, a + b, a - b, a * b, a / b
    // Extract types of 'a' and 'b'
    // Match to trait_impls where trait_name = "PartialEq", "Add", etc.
    // Return: { "Type::eq" -> call_count }
}
```

**AST Traversal:**
- Find `SyntaxKind::BIN_EXPR` nodes
- Extract operator: `==`, `!=`, `+`, `-`, `*`, `/`, `<`, `>`, `<=`, `>=`
- Extract LHS and RHS variable names
- Map operators to traits:
  - `==`, `!=` → `PartialEq::eq`
  - `+` → `Add::add`
  - `-` → `Sub::sub`
  - `*` → `Mul::mul`
  - `/` → `Div::div`
  - `<`, `>`, `<=`, `>=` → `PartialOrd::partial_cmp`, `Ord::cmp`

### Phase 4: Assert Macro Detection
**File:** `src/bin/review_test_functions.rs`

Add new function:
```rust
fn find_assert_macro_calls(file: &Path, parsed: &SourceFile, trait_impls: &[TraitImpl]) -> HashMap<String, usize> {
    // Search for: assert_eq!(a, b), assert_ne!(a, b)
    // Extract variable names
    // Mark as calling PartialEq::eq
}
```

**AST Traversal:**
- Find `MACRO_CALL` with path = `assert_eq`, `assert_ne`
- Extract token tree and find first two identifiers (the arguments)
- Mark as using `PartialEq::eq` (and `Debug::fmt` for error messages)

### Phase 5: Intra-Module Call Graph
**File:** `src/bin/review_test_functions.rs`

Add new data structures:
```rust
struct CallGraphEdge {
    caller: String,        // "ng_of_vertices"
    callee: String,        // "parallel_ng_of_vertices"
    caller_is_public: bool,
}

fn build_call_graph(file: &Path, parsed: &SourceFile) -> Vec<CallGraphEdge> {
    // For each function in the file:
    //   - Determine if it's public
    //   - Find all CALL_EXPR and METHOD_CALL_EXPR within body
    //   - Extract callee names
    //   - Record: caller -> callee edge
}
```

**AST Traversal:**
- Find all `SyntaxKind::FN` nodes
- Check visibility (public vs private)
- Traverse function body for `CALL_EXPR` and `METHOD_CALL_EXPR`
- Extract callee names (handle both `foo()` and `self.foo()`)

### Phase 6: Transitive Coverage Propagation
**File:** `src/bin/review_test_functions.rs`

Add new function:
```rust
fn propagate_transitive_coverage(
    call_graph: &[CallGraphEdge],
    tested_functions: &HashSet<String>,
) -> HashSet<String> {
    // Fixed-point iteration:
    // 1. Start with directly tested functions
    // 2. If caller is tested AND caller calls callee, mark callee as tested
    // 3. Repeat until no new functions are marked
}
```

**Algorithm:**
```
tested = { directly_tested_functions }
loop:
    added_any = false
    for edge in call_graph:
        if edge.caller in tested AND edge.callee not in tested:
            tested.add(edge.callee)
            added_any = true
    if not added_any:
        break
return tested
```

### Phase 7: Configuration Flags
**File:** `src/bin/review_test_functions.rs`

Extend `StandardArgs` or add custom args:
```rust
struct TestFunctionArgs {
    standard_args: StandardArgs,
    ignore_trait_impls: bool,      // --ignore-trait-impls
    ignore_internal_helpers: bool,  // --ignore-internal-helpers
    show_transitive: bool,          // --show-transitive
    show_false_positive_likely: bool, // --show-false-positives
}
```

**Flag Behavior:**
- `--ignore-trait-impls`: Don't report Display, Debug, PartialEq, Ord, operator traits
- `--ignore-internal-helpers`: Don't report `parallel_*`, `*_rec`, private functions
- `--show-transitive`: Show "tested transitively via X()" for internal helpers
- `--show-false-positives`: Annotate functions likely to be false positives

### Phase 8: Testing & Validation
**File:** Tests and validation script

1. Run on apas-ai: `review-test-functions -c`
2. Compare with `analyses/false_positives_report_for_tool_tuning.txt`
3. Verify:
   - All 107 Display/Debug fmt() marked as tested
   - All 33 PartialEq eq() marked as tested
   - All 45 internal helpers marked as tested (transitively)
   - All 4 operator traits marked as tested
4. Target: False positive rate < 5% (down from 96.3%)

---

## Expected Output Format (After Fix)

### Current Output:
```
src/Chap65/PrimStEph.rs:63:  fmt - NO TEST COVERAGE
```

### New Output (Option 1 - Hide):
```
(not shown - trait implementation automatically tested)
```

### New Output (Option 2 - Show with annotation):
```
src/Chap65/PrimStEph.rs:63:  fmt - 12 call(s) in 1 test file(s) (via Display trait, format!() macro) ✓
```

### New Output (Option 3 - Transitive):
```
src/Chap06/DirGraphMtEph.rs:100:  parallel_ng_of_vertices - tested transitively via ng_of_vertices() ✓
```

---

## Success Criteria

1. **False Positive Rate**: < 5% (down from 96.3%)
2. **Compilation**: No errors, no warnings
3. **Performance**: < 30s for full apas-ai codebase
4. **Maintainability**: Pure AST, zero string hacking
5. **Validation**: Run on apas-ai and compare with manual analysis

---

## Estimated Effort

- **Phase 1 (Trait Detection)**: 2 hours
- **Phase 2 (Display/Debug)**: 2 hours
- **Phase 3 (Operators)**: 1 hour
- **Phase 4 (Assert Macros)**: 1 hour
- **Phase 5 (Call Graph)**: 3 hours
- **Phase 6 (Transitive)**: 2 hours
- **Phase 7 (Config Flags)**: 1 hour
- **Phase 8 (Testing)**: 2 hours

**Total**: ~14 hours of focused work

---

## Risks & Mitigations

**Risk 1**: Type inference is hard without full semantic analysis
- **Mitigation**: Use heuristics (variable names often match type names) and pattern matching

**Risk 2**: Macro expansion is complex
- **Mitigation**: Focus on common patterns (format!, assert_eq!) and use token analysis

**Risk 3**: Call graph across modules is expensive
- **Mitigation**: Phase 5 only does intra-module call graph (same file)

**Risk 4**: May still have false positives for complex cases
- **Mitigation**: Add `--show-false-positives` flag to help users identify and report edge cases

