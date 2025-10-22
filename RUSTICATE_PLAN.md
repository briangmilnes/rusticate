# Rusticate Plan: Convert Python Review Scripts to Rust

**Goal:** Convert all Python `review_*.py` scripts to high-quality Rust tools using AST traversal.

**Date:** 2025-10-22

---

## Current Status

### ✅ Already Converted (27 Rust Tools)
1. `review-bench-modules` 
2. `review-comment-placement`
3. `review-doctests`
4. `review-duplicate-bench-names`
5. `review-duplicate-methods`
6. `review-impl-order`
7. `review-import-order`
8. `review-inherent-and-trait-impl`
9. `review-integration-test-structure`
10. `review-internal-method-impls`
11. `review-minimize-ufcs-call-sites`
12. `review-module-encapsulation`
13. `review-no-extern-crate`
14. `review-non-wildcard-uses`
15. `review-pascal-case-filenames`
16. `review-public-only-inherent-impls`
17. `review-single-trait-impl`
18. `review-snake-case-filenames`
19. `review-string-hacking` (meta-tool)
20. `review-struct-file-naming`
21. `review-stub-delegation`
22. `review-test-modules`
23. `review-trait-bound-mismatches`
24. `review-typeclasses`
25. `review-variable-naming`
26. `review-where-clause-simplification`
27. `review` (main dispatcher)

**All 27 tools pass `review-string-hacking` with 0 or minimal violations!**

---

## Conversion Plan

### Phase 1: CRITICAL (10 tools) - Core Structural Analysis

**Priority:** These are fundamental code quality tools used frequently.

#### 1. review_redundant_inherent_impls.py → review-redundant-inherent-impls
- **Python:** `scripts/rust/src/review_redundant_inherent_impls.py`
- **Rust:** `src/bin/review_redundant_inherent_impls.rs` (TO CREATE)
- **Importance:** HIGH - Extends `stub-delegation` to find all unnecessary inherent impls
- **Complexity:** Medium - Similar to existing `stub-delegation` tool
- **Estimated:** 2-3 hours
- **Dependencies:** None
- **Steps:**
  1. Git checkout APAS-AI-copy clean state
  2. Analyze Python script: `cat scripts/rust/src/review_redundant_inherent_impls.py`
  3. Create Rust tool using AST traversal
  4. Run `review-string-hacking` (target: 0 violations)
  5. Test on APAS-AI-copy
  6. Commit

#### 2. review_trait_method_conflicts.py → review-trait-method-conflicts
- **Python:** `scripts/rust/src/review_trait_method_conflicts.py`
- **Rust:** `src/bin/review_trait_method_conflicts.rs` (TO CREATE)
- **Importance:** HIGH - Prevents trait method ambiguity
- **Complexity:** Medium - Method name analysis across traits
- **Estimated:** 2 hours
- **Dependencies:** None

#### 3. review_no_trait_method_duplication.py → review-no-trait-method-duplication
- **Python:** `scripts/rust/src/review_no_trait_method_duplication.py`
- **Rust:** `src/bin/review_no_trait_method_duplication.rs` (TO CREATE)
- **Importance:** HIGH - Similar to `duplicate-methods` but trait-specific
- **Complexity:** Low - Can reuse `duplicate-methods` logic
- **Estimated:** 1-2 hours
- **Dependencies:** `review-duplicate-methods` (for reference)

#### 4. review_qualified_paths.py → review-qualified-paths
- **Python:** `scripts/rust/src/review_qualified_paths.py`
- **Rust:** `src/bin/review_qualified_paths.rs` (TO CREATE)
- **Importance:** MEDIUM - Code style consistency
- **Complexity:** Medium - UFCS pattern detection
- **Estimated:** 2-3 hours
- **Dependencies:** Related to `minimize-ufcs-call-sites`

#### 5. review_trait_definition_order.py → review-trait-definition-order
- **Python:** `scripts/rust/src/review_trait_definition_order.py`
- **Rust:** `src/bin/review_trait_definition_order.rs` (TO CREATE)
- **Importance:** MEDIUM - Consistency enforcement
- **Complexity:** Low - Simple ordering check
- **Estimated:** 1 hour
- **Dependencies:** None

#### 6. review_stt_compliance.py → review-stt-compliance
- **Python:** `scripts/rust/src/review_stt_compliance.py`
- **Rust:** `src/bin/review_stt_compliance.rs` (TO CREATE)
- **Importance:** HIGH - APAS-specific type system check
- **Complexity:** High - Domain-specific logic
- **Estimated:** 3-4 hours
- **Dependencies:** Understanding of APAS StT/MtT types

#### 7. review_inherent_plus_trait_impl.py → review-inherent-plus-trait-impl  
- **Python:** `scripts/rust/src/review_inherent_plus_trait_impl.py`
- **Rust:** `src/bin/review_inherent_plus_trait_impl.rs` (TO CREATE)
- **Importance:** HIGH - Redundancy detection
- **Complexity:** Medium - Extends `inherent-and-trait-impl`
- **Estimated:** 2 hours
- **Dependencies:** `review-inherent-and-trait-impl`

#### 8. review_private_inherent_methods.py → review-private-inherent-methods
- **Python:** `scripts/rust/src/review_private_inherent_methods.py`
- **Rust:** `src/bin/review_private_inherent_methods.rs` (TO CREATE)
- **Importance:** MEDIUM - Code organization
- **Complexity:** Medium - Visibility analysis
- **Estimated:** 2 hours
- **Dependencies:** None

#### 9. review_trait_self_usage.py → review-trait-self-usage
- **Python:** `scripts/rust/src/review_trait_self_usage.py`
- **Rust:** `src/bin/review_trait_self_usage.rs` (TO CREATE)
- **Importance:** MEDIUM - API consistency
- **Complexity:** Medium - Self type analysis
- **Estimated:** 2 hours
- **Dependencies:** None

#### 10. review_impl_trait_bounds.py → review-impl-trait-bounds
- **Python:** `scripts/rust/src/review_impl_trait_bounds.py`
- **Rust:** `src/bin/review_impl_trait_bounds.rs` (TO CREATE)
- **Importance:** HIGH - Type system correctness
- **Complexity:** High - Generic bounds analysis
- **Estimated:** 3-4 hours
- **Dependencies:** None

**Phase 1 Total:** ~20-25 hours

---

### Phase 2: HIGH (15 tools) - Important Quality Checks

#### 11. review_cargo.py → review-cargo
- **Python:** `scripts/rust/src/review_cargo.py`
- **Rust:** `src/bin/review_cargo.rs` (TO CREATE)
- **Importance:** HIGH - Build system validation
- **Complexity:** Low - TOML parsing
- **Estimated:** 1-2 hours

#### 12. review_lib.py → review-lib
- **Python:** `scripts/rust/src/review_lib.py`
- **Rust:** `src/bin/review_lib.rs` (TO CREATE)
- **Importance:** MEDIUM - Library structure
- **Complexity:** Low
- **Estimated:** 1 hour

#### 13. review_rust_src.py → review-rust-src
- **Python:** `scripts/rust/src/review_rust_src.py`
- **Rust:** `src/bin/review_rust_src.rs` (TO CREATE)
- **Importance:** MEDIUM - Orchestration
- **Estimated:** 1 hour

#### 14. review_rust_tests.py → review-rust-tests
- **Python:** `scripts/rust/tests/review_rust_tests.py`
- **Rust:** `src/bin/review_rust_tests.rs` (TO CREATE)
- **Importance:** MEDIUM - Orchestration
- **Estimated:** 1 hour

#### 15. review_rust_benches.py → review-rust-benches
- **Python:** `scripts/rust/benches/review_rust_benches.py`
- **Rust:** `src/bin/review_rust_benches.rs` (TO CREATE)
- **Importance:** MEDIUM - Orchestration
- **Estimated:** 1 hour

#### 16. review_rust.py → review-rust
- **Python:** `scripts/rust/review_rust.py`
- **Rust:** `src/bin/review_rust.rs` (TO CREATE)
- **Importance:** HIGH - Main Rust dispatcher
- **Estimated:** 1 hour

#### 17. review_trait_default_pattern.py → review-trait-default-pattern
- **Python:** `scripts/rust/src/review_trait_default_pattern.py`
- **Rust:** `src/bin/review_trait_default_pattern.rs` (TO CREATE)
- **Importance:** MEDIUM - Pattern detection
- **Complexity:** Medium
- **Estimated:** 2 hours

#### 18. review_inherent_method_lengths.py → review-inherent-method-lengths
- **Python:** `scripts/rust/src/review_inherent_method_lengths.py`
- **Rust:** `src/bin/review_inherent_method_lengths.rs` (TO CREATE)
- **Importance:** LOW - Code metrics
- **Complexity:** Low - Line counting
- **Estimated:** 1 hour

#### 19. review_macro_method_calls.py → review-macro-method-calls
- **Python:** `scripts/rust/src/review_macro_method_calls.py`
- **Rust:** `src/bin/review_macro_method_calls.rs` (TO CREATE)
- **Importance:** MEDIUM - Macro validation
- **Complexity:** High - Macro expansion analysis
- **Estimated:** 3-4 hours

#### 20. review_external_type_calls.py → review-external-type-calls
- **Python:** `scripts/rust/src/review_external_type_calls.py`
- **Rust:** `src/bin/review_external_type_calls.rs` (TO CREATE)
- **Importance:** LOW - Dependency analysis
- **Complexity:** Medium
- **Estimated:** 2 hours

#### 21. review_all_forwarding.py → review-all-forwarding
- **Python:** `scripts/rust/src/review_all_forwarding.py`
- **Rust:** `src/bin/review_all_forwarding.rs` (TO CREATE)
- **Importance:** MEDIUM - Related to stub-delegation
- **Complexity:** Medium
- **Estimated:** 2 hours

#### 22. review_change_to_snake_case.py (SKIP - overlap with existing)
- **Python:** `scripts/rust/review_change_to_snake_case.py`
- **Rust:** `src/bin/review_snake_case_filenames.rs` (ALREADY EXISTS - partial)
- **Note:** May be duplicate of existing tool

#### 23. review_camelcase.py (SKIP - overlap with existing)
- **Python:** `scripts/rust/review_camelcase.py`
- **Rust:** `src/bin/review_pascal_case_filenames.rs` (ALREADY EXISTS - partial)
- **Note:** May be duplicate of existing tool

#### 24. review_APAS.py → review-APAS
- **Python:** `scripts/APAS/review_APAS.py`
- **Rust:** `src/bin/review_APAS.rs` (TO CREATE)
- **Importance:** HIGH - Main APAS dispatcher
- **Estimated:** 2 hours

**Phase 2 Total:** ~18-22 hours

---

### Phase 3: MEDIUM (20 tools) - APAS-Specific Domain Logic

These are APAS-specific checks. Lower priority unless actively working on APAS codebase.

**Estimated:** 30-40 hours total

---

### Phase 4: LOW (8 tools) - Nice-to-Have

Benchmark-specific and utility scripts.

**Estimated:** 10-15 hours

---

## Execution Steps (Per Tool)

### Standard Workflow

```bash
# 1. Setup rusticate repo
cd /home/milnes/projects/rusticate
git checkout main
git pull

# 2. Find the git commit the Python script was tested on
# Check script comments, git log, or ask user for the target commit
cat scripts/<path>/review_<name>.py | head -20
# Look for: "Tested on commit: <sha>" or similar

# 3. Checkout APAS-AI-copy to the correct commit
cd APAS-AI-copy/apas-ai
git log --oneline -20  # Review recent history
git checkout <commit-sha>  # The commit the Python script was designed for
cd ../..

# 4. Run Python script to get expected output
cd APAS-AI-copy/apas-ai
python3 ~/projects/rusticate/scripts/<path>/review_<name>.py > /tmp/python_output.txt
# Save this as the "gold standard" output

# 5. Analyze Python script logic
cd ~/projects/rusticate
cat scripts/<path>/review_<name>.py
# Understand: What patterns does it detect? What's the output format?

# 6. Create Rust tool
# File: src/bin/review_<name>.rs
# - Use ra_ap_syntax for AST traversal
# - Use StandardArgs for argument parsing
# - Follow existing tool patterns (e.g., review-typeclasses)
# - Emacs-clickable output: file:line: message
# - Include Pareto analysis if useful

# 7. Build
cargo build --release --bin rusticate-review-<name>

# 8. Test on APAS-AI-copy (SAME commit as Python script)
cd APAS-AI-copy/apas-ai
~/projects/rusticate/target/release/rusticate-review-<name> -c > /tmp/rust_output.txt

# 9. Compare outputs
diff /tmp/python_output.txt /tmp/rust_output.txt
# Outputs should match (or Rust should be superset with improvements)

# 10. Run string hacking review
cd ~/projects/rusticate
./target/release/rusticate-review-string-hacking -f src/bin/review_<name>.rs
# Target: 0 violations (or document acceptable ones)

# 11. Commit Rust tool
git add src/bin/review_<name>.rs
git commit -m "Add review-<name>: [description]

- Pure AST traversal using ra_ap_syntax
- Emacs-clickable output
- Pareto analysis [if applicable]
- Passes review-string-hacking with 0 violations
- Replaces scripts/<path>/review_<name>.py
- Tested against APAS-AI commit: <sha>"

# 12. Update Cargo.toml
# Add [[bin]] entry for new tool

# 13. Return APAS-AI-copy to main (if needed)
cd APAS-AI-copy/apas-ai
git checkout main
cd ../..
```

---

## Quality Standards

### ✅ Acceptance Criteria

Every new tool must:

1. **Pure AST:** Use `ra_ap_syntax` for code analysis (no string hacking)
2. **Zero Violations:** Pass `review-string-hacking` with 0 violations (or document exceptions)
3. **Standard Args:** Use `StandardArgs::parse()` for `-c`, `-m`, `-d`, `-f` flags
4. **Emacs Output:** Format: `file:line: message` for clickability
5. **Pareto Analysis:** Include for bug categorization (where useful)
6. **Performance:** Process full APAS-AI codebase in < 1 second
7. **Documentation:** Clear doc comments explaining what it detects
8. **Tested:** Run on APAS-AI-copy and verify results

---

## Progress Tracking

### Completed: 27/79 (34%)

### Phase 1 (CRITICAL): 0/10 (0%)
- [ ] review_redundant_inherent_impls
- [ ] review_trait_method_conflicts
- [ ] review_no_trait_method_duplication
- [ ] review_qualified_paths
- [ ] review_trait_definition_order
- [ ] review_stt_compliance
- [ ] review_inherent_plus_trait_impl
- [ ] review_private_inherent_methods
- [ ] review_trait_self_usage
- [ ] review_impl_trait_bounds

### Phase 2 (HIGH): 0/15 (0%)
(Listed above)

### Phase 3 (MEDIUM): 0/20 (0%)
(APAS-specific)

### Phase 4 (LOW): 0/8 (0%)
(Utilities)

---

## Notes

- **String Hacking Elimination:** All Phase 1-2 tools completed successfully eliminated 10 string hacking violations
- **Pattern Library:** Emerging patterns in `src/args.rs` and shared traversal utilities
- **Performance:** Current tools average 200-400ms on 661 files (APAS-AI)
- **Test Coverage:** Tools are integration-tested on live APAS-AI-copy codebase

---

## Next Actions

1. ✅ Create this plan
2. Start Phase 1, tool #1: `review-redundant-inherent-impls`
3. Work through CRITICAL tier before moving to HIGH tier
4. After Phase 1 complete, reassess priorities based on actual APAS development needs

