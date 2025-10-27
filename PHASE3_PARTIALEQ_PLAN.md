# Phase 3 Plan: PartialEq Trait Detection

## Executive Summary

**Goal:** Detect `PartialEq::eq` usage via `==`, `!=` operators and `assert_eq!()`, `assert_ne!()` macros  
**Impact:** Fix 41 false positives (45% of remaining 90)  
**Effort:** ~2 hours  
**Expected Outcome:** Coverage 96.3% → 98.0%, Uncovered 90 → 49

---

## Current Problem

**41 `eq()` functions reported as "NO TEST COVERAGE":**
```
src/Chap05/MappingStEph.rs:98:  eq - NO TEST COVERAGE
src/Chap05/RelationStEph.rs:104:  eq - NO TEST COVERAGE
src/Chap05/SetStEph.rs:142:  eq - NO TEST COVERAGE
... (38 more)
```

**But tests DO use these via operators:**
```rust
// Test code uses:
assert_eq!(seq1, seq2);        // Calls PartialEq::eq implicitly
if mapping1 == mapping2 { }    // Calls PartialEq::eq via == operator
assert_ne!(set1, set2);        // Calls PartialEq::eq implicitly (negated)
```

**Tool currently only detects direct calls like:** `obj.eq(&other)`

---

## Implementation Strategy

### Step 1: Add PartialEqTrait to CoverageSource

**File:** `src/bin/review_test_functions.rs`

```rust
#[derive(Debug, Clone)]
enum CoverageSource {
    Direct,
    DisplayTrait,
    DebugTrait,
    PartialEqTrait,    // NEW
}
```

---

### Step 2: Create find_operator_usage() Function

**Signature:**
```rust
fn find_operator_usage(
    test_file: &Path,
    trait_impls: &[TraitImpl]
) -> Result<HashMap<String, (usize, CoverageSource)>>
```

**Detection Patterns:**

#### Pattern 1: Binary Expressions (== and !=)
```rust
// AST: SyntaxKind::BIN_EXPR
for node in root.descendants() {
    if node.kind() == SyntaxKind::BIN_EXPR {
        if let Some(bin_expr) = ast::BinExpr::cast(node) {
            // Check operator
            if let Some(op) = bin_expr.op_kind() {
                if op == BinOp::EqualityTest || op == BinOp::NegatedEqualityTest {
                    // Extract LHS and RHS
                    let lhs = bin_expr.lhs();
                    let rhs = bin_expr.rhs();
                    
                    // Extract variable names from expressions
                    // Match to types implementing PartialEq
                }
            }
        }
    }
}
```

**AST Structure:**
```
BIN_EXPR
  └─ lhs: Expr (e.g., PATH_EXPR with ident "seq1")
  └─ op: == or !=
  └─ rhs: Expr (e.g., PATH_EXPR with ident "seq2")
```

#### Pattern 2: assert_eq! and assert_ne! Macros
```rust
// AST: SyntaxKind::MACRO_CALL
if let Some(macro_call) = ast::MacroCall::cast(node) {
    let macro_name = macro_call.path()
        .and_then(|p| p.segments().last())
        .and_then(|s| s.name_ref())
        .map(|n| n.text().to_string());
    
    if macro_name == Some("assert_eq") || macro_name == Some("assert_ne") {
        // Extract token tree
        // Find first two identifiers (the arguments being compared)
        // Match to types implementing PartialEq
    }
}
```

---

### Step 3: Variable-to-Type Matching

**Same heuristic as Display/Debug (Phase 2):**

```rust
fn match_variable_to_types(
    variable: &str,
    type_to_traits: &HashMap<String, Vec<(String, String)>>
) -> Vec<String> {
    let var_lower = variable.to_lowercase();
    let mut matches = Vec::new();
    
    for (type_name, traits) in type_to_traits {
        // Check if type implements PartialEq or Eq
        let has_eq = traits.iter().any(|(t, m)| {
            (t.contains("PartialEq") || t.contains("Eq")) && m == "eq"
        });
        
        if !has_eq {
            continue;
        }
        
        let type_lower = type_name.to_lowercase();
        
        // Match heuristics (same as Phase 2):
        if type_lower.contains(&var_lower) ||
           var_lower == type_lower ||
           type_lower.starts_with(&var_lower) {
            matches.push(type_name.clone());
        }
    }
    
    matches
}
```

**Examples:**
- Variable `seq1` → Type `ArraySeq`, `ArraySeqStEph`, etc.
- Variable `mapping` → Type `Mapping`, `MappingStEph`, etc.
- Variable `s` → Type `Set`, `SetStEph`, etc.

---

### Step 4: Build Return HashMap

```rust
let mut operator_calls: HashMap<String, (usize, CoverageSource)> = HashMap::new();

// For each == or != operator found:
for matched_type in matched_types {
    let key = format!("{}::eq", matched_type);
    let entry = operator_calls.entry(key).or_insert((0, CoverageSource::PartialEqTrait));
    entry.0 += 1;
}

// For each assert_eq!/assert_ne! found:
// (same logic)
```

---

### Step 5: Integration into main()

**Add call to find_operator_usage():**

```rust
// In main(), inside the test file loop:
if tests_dir.exists() {
    for entry in WalkDir::new(&tests_dir) {
        // ... existing code ...
        
        // Find format macro calls (Display/Debug trait usage)
        match find_format_macro_calls(entry.path(), &all_trait_impls) {
            // ... existing code ...
        }
        
        // NEW: Find operator usage (PartialEq trait usage)
        match find_operator_usage(entry.path(), &all_trait_impls) {
            Ok(operator_calls) => {
                for (method_key, (count, source)) in operator_calls {
                    let entry_data = trait_method_calls.entry(method_key)
                        .or_insert((0, Vec::new(), source));
                    entry_data.0 += count;
                    if !entry_data.1.contains(&entry.path().to_path_buf()) {
                        entry_data.1.push(entry.path().to_path_buf());
                    }
                }
            }
            Err(e) => logger.log(&format!("Warning: Failed to parse operators in test file {}: {}", entry.path().display(), e)),
        }
    }
}
```

---

### Step 6: Update Output Annotation

**Modify output reporting:**

```rust
// In the output section:
let coverage_annotation = match cov.coverage_source {
    CoverageSource::DisplayTrait => " (via Display trait)",
    CoverageSource::DebugTrait => " (via Debug trait)",
    CoverageSource::PartialEqTrait => " (via PartialEq trait)",  // NEW
    CoverageSource::Direct => "",
};
```

**Expected output:**
```
src/Chap05/MappingStEph.rs:98:  MappingStEph::eq - 45 call(s) in 12 test file(s) (via PartialEq trait)
src/Chap05/SetStEph.rs:142:  SetStEph::eq - 38 call(s) in 10 test file(s) (via PartialEq trait)
```

---

## Testing Strategy

### Test 1: Verify Binary Expression Detection
```bash
# Check a known file with == operators
grep -A 3 "==" /path/to/test/file.rs
# Should find: if seq1 == seq2 { ... }
```

### Test 2: Verify assert_eq! Detection
```bash
# Check for assert_eq! usage
grep "assert_eq!" tests/Chap05/TestSetStEph.rs
# Should find: assert_eq!(s1, s2);
```

### Test 3: Run on apas-ai
```bash
cd APAS-AI-copy/apas-ai
review-test-functions -c
```

**Expected:**
- Uncovered: 90 → ~49 (41 functions fixed)
- Coverage: 96.3% → ~98.0%
- 41 eq functions show "(via PartialEq trait)"

---

## Edge Cases to Handle

### 1. Method Call Expressions
```rust
if obj.clone() == other.clone() { }
```
Need to extract the base object name, not the method result.

### 2. Chained Comparisons
```rust
if a == b && b == c { }
```
Each `==` is a separate BIN_EXPR, count both.

### 3. Negated Comparisons
```rust
if a != b { }  // Still calls PartialEq::eq (then negates)
```
Treat `!=` same as `==` for coverage purposes.

### 4. Generic Assert Macros
```rust
assert_eq!(result, expected, "custom message");
```
Extract only first two arguments (before any commas after them).

---

## Success Criteria

✅ Binary operators (`==`, `!=`) detected via AST  
✅ `assert_eq!()`, `assert_ne!()` macros detected  
✅ Variable names matched to types implementing PartialEq/Eq  
✅ Output shows "(via PartialEq trait)" annotation  
✅ 41 eq functions moved from "NO TEST COVERAGE" to covered  
✅ Coverage increases from 96.3% to ~98.0%  
✅ No compilation errors or warnings  
✅ Zero string hacking (pure AST)  

---

## Estimated Timeline

| Task | Estimated Time |
|------|----------------|
| Add enum variant | 5 min |
| Implement find_operator_usage() | 60 min |
| Binary expression detection | 30 min |
| assert_eq!/assert_ne! detection | 20 min |
| Variable-to-type matching | 10 min (reuse from Phase 2) |
| Integration into main() | 10 min |
| Output annotation | 5 min |
| Testing on apas-ai | 10 min |
| Debug and fixes | 30 min |
| **Total** | **~2 hours** |

---

## Risks & Mitigations

**Risk 1:** BinOp enum values might be named differently  
**Mitigation:** Check ra_ap_syntax docs for correct enum variant names

**Risk 2:** Variable matching heuristic may miss some types  
**Mitigation:** Same heuristic achieved 94% success in Phase 2, acceptable

**Risk 3:** Complex expressions (method chains) may not extract variable names  
**Mitigation:** Start with simple PATH_EXPR, can enhance later if needed

---

## After Phase 3

**Remaining 49 uncovered functions will be:**
- 6 fmt (Display/Debug) - heuristic didn't match
- 19 parallel_* helpers - need call graph (Phase 5+6)
- 13 *_rec helpers - need call graph (Phase 5+6)
- 11 other - edge cases

**Next decision point:** Phase 4 (4 operator traits) or Phases 5+6 (call graph)?

