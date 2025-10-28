# Phases 5+6 Plan: Call Graph & Transitive Coverage

## Executive Summary

**Goal:** Detect transitive test coverage for internal helper functions  
**Impact:** Fix 32 false positives (56% of remaining 57)  
**Effort:** ~3-5 hours  
**Expected Outcome:** Coverage 97.6% → 98.9%, Uncovered 57 → 25

---

## Current Problem

**32 helper functions reported as "NO TEST COVERAGE":**
```
src/Chap06/DirGraphMtEph.rs:100:  parallel_ng_of_vertices - NO TEST COVERAGE
src/Chap37/BSTPlainMtEph.rs:111:  find_rec - NO TEST COVERAGE
src/Chap39/BSTTreapMtEph.rs:201:  height_rec - NO TEST COVERAGE
... (29 more)
```

**But these ARE tested transitively:**
```rust
// Public method (HAS test coverage)
pub fn ng_of_vertices(&self, vs: &[Vertex]) -> ArraySeqMtEph<Vertex> {
    self.parallel_ng_of_vertices(vs)  // Calls private helper
}

// Private helper (NO direct test, but tested via public method)
fn parallel_ng_of_vertices(&self, vs: &[Vertex]) -> ArraySeqMtEph<Vertex> {
    // Implementation
}
```

**Test code:**
```rust
// tests/Chap06/TestDirGraphMtEph.rs
graph.ng_of_vertices(&vertices);  // Tests public method
// This TRANSITIVELY tests parallel_ng_of_vertices()!
```

**Tool currently:** Only counts direct calls, misses transitive coverage.

---

## Implementation Strategy

### Phase 5: Build Intra-Module Call Graph

**Goal:** Map which functions call which other functions within each module.

#### Step 1: Define Data Structures

```rust
#[derive(Debug, Clone)]
struct CallGraphEdge {
    caller_name: String,        // Function that makes the call
    callee_name: String,        // Function being called
    caller_is_public: bool,     // Is caller a public function?
    caller_file: PathBuf,       // Source file
    caller_line: usize,         // Line number for debugging
}
```

**Example edges:**
```
CallGraphEdge {
    caller_name: "ng_of_vertices",
    callee_name: "parallel_ng_of_vertices",
    caller_is_public: true,
}

CallGraphEdge {
    caller_name: "find",
    callee_name: "find_rec",
    caller_is_public: true,
}
```

---

#### Step 2: Implement build_call_graph()

**Function signature:**
```rust
fn build_call_graph(file: &Path) -> Result<Vec<CallGraphEdge>>
```

**Algorithm:**
1. Parse the source file
2. Find all `FN` nodes (function definitions)
3. For each function:
   - Extract function name
   - Check if it's public (has `pub` keyword, not `pub(crate)`)
   - Traverse function body for calls
4. Return list of edges

**Detecting Function Calls:**

**Pattern 1: Static calls** (`Type::method()` or `function()`)
```rust
// AST: SyntaxKind::CALL_EXPR
for node in function_body.descendants() {
    if node.kind() == SyntaxKind::CALL_EXPR {
        if let Some(call_expr) = ast::CallExpr::cast(node) {
            if let Some(expr) = call_expr.expr() {
                if let ast::Expr::PathExpr(path_expr) = expr {
                    if let Some(path) = path_expr.path() {
                        if let Some(segment) = path.segments().last() {
                            let callee = segment.name_ref()
                                .map(|n| n.text().to_string());
                            // Record edge: caller → callee
                        }
                    }
                }
            }
        }
    }
}
```

**Pattern 2: Method calls** (`self.method()` or `obj.method()`)
```rust
// AST: SyntaxKind::METHOD_CALL_EXPR
if node.kind() == SyntaxKind::METHOD_CALL_EXPR {
    if let Some(method_call) = ast::MethodCallExpr::cast(node) {
        if let Some(name_ref) = method_call.name_ref() {
            let callee = name_ref.text().to_string();
            // Record edge: caller → callee
        }
    }
}
```

**Visibility Detection:**
```rust
fn is_function_public(fn_node: &SyntaxNode) -> bool {
    let visibility = fn_node.children_with_tokens()
        .find_map(|child| {
            if child.kind() == SyntaxKind::VISIBILITY {
                child.as_node().cloned()
            } else {
                None
            }
        });
    
    if let Some(vis_node) = visibility {
        let has_pub = vis_node.children_with_tokens()
            .any(|t| t.kind() == SyntaxKind::PUB_KW);
        let has_restriction = vis_node.children_with_tokens()
            .any(|t| t.kind() == SyntaxKind::L_PAREN);
        has_pub && !has_restriction
    } else {
        false
    }
}
```

---

#### Step 3: Integrate into main()

```rust
// In main(), after finding all public functions:
let mut call_graphs: HashMap<PathBuf, Vec<CallGraphEdge>> = HashMap::new();

for entry in WalkDir::new(&src_dir) {
    if is_rust_file(entry) {
        match build_call_graph(entry.path()) {
            Ok(edges) => {
                call_graphs.insert(entry.path().to_path_buf(), edges);
            }
            Err(e) => logger.log(&format!("Warning: Failed to build call graph for {}: {}", entry.path().display(), e)),
        }
    }
}
```

---

### Phase 6: Transitive Coverage Propagation

**Goal:** If a public method is tested and calls a private helper, mark the helper as tested.

#### Step 1: Add TransitiveCoverage Enum Variant

```rust
#[derive(Debug, Clone)]
enum CoverageSource {
    Direct,
    DisplayTrait,
    DebugTrait,
    PartialEqTrait,
    TransitiveCoverage(String),  // NEW: String = caller name
}
```

**Example:**
```rust
CoverageSource::TransitiveCoverage("ng_of_vertices".to_string())
```

---

#### Step 2: Implement propagate_transitive_coverage()

**Function signature:**
```rust
fn propagate_transitive_coverage(
    call_graph: &[CallGraphEdge],
    tested_functions: &HashMap<String, usize>,  // function_name -> call_count
) -> HashMap<String, (usize, String)>  // helper_name -> (count, caller_name)
```

**Algorithm (Fixed-Point Iteration):**
```rust
fn propagate_transitive_coverage(
    call_graph: &[CallGraphEdge],
    tested_functions: &HashMap<String, usize>,
) -> HashMap<String, (usize, String)> {
    let mut transitively_tested: HashMap<String, (usize, String)> = HashMap::new();
    
    // Fixed-point iteration
    loop {
        let mut added_any = false;
        
        for edge in call_graph {
            // Check if caller is tested (directly or transitively)
            let caller_is_tested = 
                tested_functions.contains_key(&edge.caller_name) ||
                transitively_tested.contains_key(&edge.caller_name);
            
            // Check if callee is not yet marked as tested
            let callee_not_tested = 
                !tested_functions.contains_key(&edge.callee_name) &&
                !transitively_tested.contains_key(&edge.callee_name);
            
            if caller_is_tested && callee_not_tested {
                // Mark callee as transitively tested via caller
                let caller_count = tested_functions.get(&edge.caller_name)
                    .copied()
                    .or_else(|| transitively_tested.get(&edge.caller_name).map(|(c, _)| *c))
                    .unwrap_or(1);
                
                transitively_tested.insert(
                    edge.callee_name.clone(),
                    (caller_count, edge.caller_name.clone())
                );
                added_any = true;
            }
        }
        
        // Stop when no new functions are marked
        if !added_any {
            break;
        }
    }
    
    transitively_tested
}
```

**Example execution:**
```
Iteration 1:
  - ng_of_vertices is tested (direct)
  - ng_of_vertices calls parallel_ng_of_vertices
  - Mark parallel_ng_of_vertices as tested via ng_of_vertices
  
Iteration 2:
  - parallel_ng_of_vertices is now tested (transitive)
  - parallel_ng_of_vertices calls some_deeper_helper
  - Mark some_deeper_helper as tested via parallel_ng_of_vertices
  
Iteration 3:
  - No new functions marked → STOP
```

---

#### Step 3: Integrate into Coverage Report

```rust
// In main(), after building initial coverage report:
for func in &mut coverage {
    if func.call_count == 0 {  // Currently shows as untested
        // Check if it has transitive coverage
        let key = if let Some(ref impl_type) = func.function.impl_type {
            format!("{}::{}", impl_type, func.function.name)
        } else {
            func.function.name.clone()
        };
        
        if let Some((count, caller)) = transitively_tested.get(&key) {
            func.call_count = *count;
            func.coverage_source = CoverageSource::TransitiveCoverage(caller.clone());
            // Keep test_files empty (no direct test)
        }
    }
}
```

---

#### Step 4: Update Output Annotation

```rust
let coverage_annotation = match &cov.coverage_source {
    CoverageSource::DisplayTrait => " (via Display trait)",
    CoverageSource::DebugTrait => " (via Debug trait)",
    CoverageSource::PartialEqTrait => " (via PartialEq trait)",
    CoverageSource::TransitiveCoverage(caller) => {
        format!(" (tested transitively via {})", caller)
    }
    CoverageSource::Direct => "",
};
```

**Expected output:**
```
src/Chap06/DirGraphMtEph.rs:100:  parallel_ng_of_vertices - 45 call(s) (tested transitively via ng_of_vertices)
src/Chap37/BSTPlainMtEph.rs:111:  find_rec - 23 call(s) (tested transitively via find)
```

---

## Edge Cases & Challenges

### Challenge 1: Function Name Matching

**Problem:** Function names might not match exactly due to:
- Methods: `impl_type::method_name` vs just `method_name`
- Generics: `function<T>` in signature vs `function` in call

**Solution:**
```rust
fn normalize_function_name(name: &str, impl_type: Option<&str>) -> String {
    if let Some(type_name) = impl_type {
        format!("{}::{}", type_name, name)
    } else {
        name.to_string()
    }
}
```

### Challenge 2: Self Method Calls

**Problem:** `self.parallel_ng_of_vertices()` - need to know we're in an impl block

**Solution:**
- When building call graph, track which impl block we're in
- Prefix callee names with impl type when inside impl block

### Challenge 3: Cross-Module Calls

**Problem:** `OtherModule::function()` - might not be in same file

**Solution:**
- Phase 5+6 only handle **intra-module** calls (same file)
- This is acceptable - most helper patterns are within same module
- Cross-module would require global call graph (much more complex)

### Challenge 4: Recursive Functions

**Problem:** `find_rec` calls itself recursively

**Solution:**
- Fixed-point iteration naturally handles this
- If `find_rec` is tested, it marks itself (no-op)
- Algorithm converges correctly

---

## Testing Strategy

### Test 1: Verify Call Graph Detection
```bash
# Pick a file with known helper pattern
cat src/Chap06/DirGraphMtEph.rs | grep -A 10 "pub fn ng_of_vertices"
# Should see: self.parallel_ng_of_vertices
```

### Test 2: Verify Public/Private Detection
```bash
# Check function visibility
grep -n "fn parallel_ng_of_vertices" src/Chap06/DirGraphMtEph.rs
# Should NOT have 'pub' keyword
```

### Test 3: Run on apas-ai
```bash
cd APAS-AI-copy/apas-ai
review-test-functions -c
grep "tested transitively" analyses/review_test_functions.txt | wc -l
# Should show ~32 functions
```

### Test 4: Verify Specific Examples
```bash
# Check parallel_ng_of_vertices is now marked as tested
grep "parallel_ng_of_vertices" analyses/review_test_functions.txt
# Should show: "tested transitively via ng_of_vertices"
```

---

## Expected Results

### Before Phases 5+6
```
src/Chap06/DirGraphMtEph.rs:100:  parallel_ng_of_vertices - NO TEST COVERAGE
src/Chap37/BSTPlainMtEph.rs:111:  find_rec - NO TEST COVERAGE
```

### After Phases 5+6
```
src/Chap06/DirGraphMtEph.rs:100:  parallel_ng_of_vertices - 45 call(s) (tested transitively via ng_of_vertices)
src/Chap37/BSTPlainMtEph.rs:111:  find_rec - 23 call(s) (tested transitively via find)
```

### Summary
```
BEFORE:
  Total public functions: 2411
  Functions with test coverage: 2354 (97.6%)
  Functions without test coverage: 57 (2.4%)

AFTER:
  Total public functions: 2411
  Functions with test coverage: 2386 (98.9%)
  Functions without test coverage: 25 (1.0%)
```

---

## Implementation Checklist

### Phase 5: Call Graph (TODOs 1-5)
- [ ] Define `CallGraphEdge` struct
- [ ] Implement `build_call_graph()` function
- [ ] Detect `CALL_EXPR` (static calls)
- [ ] Detect `METHOD_CALL_EXPR` (instance calls)
- [ ] Check function visibility (`is_function_public`)
- [ ] Build per-module call graphs
- [ ] Handle `self.method()` calls correctly
- [ ] Test call graph on sample file

### Phase 6: Transitive Coverage (TODOs 6-9)
- [ ] Add `TransitiveCoverage(String)` to `CoverageSource`
- [ ] Implement `propagate_transitive_coverage()`
- [ ] Fixed-point iteration algorithm
- [ ] Integrate into main coverage report
- [ ] Update output annotation format
- [ ] Test on apas-ai

### Testing & Validation (TODOs 10-11)
- [ ] Verify 32 helpers now detected as tested
- [ ] Check specific examples (parallel_*, *_rec)
- [ ] Validate coverage: 97.6% → 98.9%
- [ ] Validate uncovered: 57 → 25

---

## Success Criteria

✅ Call graph correctly detects function calls within modules  
✅ Public/private visibility correctly identified  
✅ Fixed-point iteration converges (no infinite loops)  
✅ 32 helper functions marked as tested transitively  
✅ Coverage increases from 97.6% to ~98.9%  
✅ Output shows clear "tested transitively via X" annotations  
✅ No compilation errors or warnings  
✅ Zero string hacking (pure AST)  
✅ Performance acceptable (<5s for full apas-ai scan)

---

## Estimated Timeline

| Task | Estimated Time |
|------|----------------|
| Define CallGraphEdge struct | 5 min |
| Implement build_call_graph() | 90 min |
| - CALL_EXPR detection | 30 min |
| - METHOD_CALL_EXPR detection | 30 min |
| - Visibility checking | 15 min |
| - Handle self.method() | 15 min |
| Add TransitiveCoverage enum | 5 min |
| Implement propagate_transitive_coverage() | 60 min |
| - Fixed-point iteration | 30 min |
| - Function name matching | 20 min |
| - Debug/testing | 10 min |
| Integration into main() | 20 min |
| Output annotation | 10 min |
| Testing on apas-ai | 15 min |
| Debug and fixes | 60 min |
| **Total** | **~4-5 hours** |

---

## Risks & Mitigations

**Risk 1:** Function name matching fails due to generics/impl types  
**Mitigation:** Normalize names, match by base name if qualified match fails

**Risk 2:** Self method calls not detected correctly  
**Mitigation:** Track current impl context during call graph traversal

**Risk 3:** Fixed-point iteration doesn't converge  
**Mitigation:** Add max iteration limit (e.g., 100) with warning

**Risk 4:** Performance issues with large call graphs  
**Mitigation:** Call graphs are per-module (small), not global

**Risk 5:** False positives where helper is exported elsewhere  
**Mitigation:** Acceptable - if helper is called, it's likely tested

---

## After Phases 5+6

**Remaining 25 uncovered functions:**
- 6 fmt (Display/Debug) - heuristic mismatches
- 8 eq (PartialEq) - heuristic mismatches  
- 11 other - genuine edge cases

**98.9% coverage** - production quality!

**Tool would be essentially complete.**

