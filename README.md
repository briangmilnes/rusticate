# rusticate

**Def. Rusticate** - go to, live in, or spend time in the country or particularly suspend a student from an Oxbridge university as a punishment.

Rusticating Python as a method for code review and fix of Rust files in LLMs by using abstract syntax trees in Rust code instead of Python and regexps.

Python will be sent back to the family estate for not working well!

---

## Table of Contents

1. [Overview](#overview)
2. [Design Principles](#design-principles)
3. [Installation](#installation)
4. [Tool Categories](#tool-categories)
   - [Compilation & Testing](#compilation--testing)
   - [Code Analysis (Review)](#code-analysis-review)
   - [Code Fixing](#code-fixing)
   - [Code Metrics](#code-metrics)
   - [Parsing & Utilities](#parsing--utilities)
5. [Standard Arguments](#standard-arguments)
6. [Tool Reference](#tool-reference)
7. [Development](#development)

---

## Overview

**Rusticate** is a suite of 76+ AST-based tools for analyzing and automatically fixing Rust codebases. Unlike traditional linters that rely on string manipulation or regex, Rusticate uses the Rust Analyzer syntax tree (`ra_ap_syntax`) for precise, reliable code transformations.

**Note:** While Rusticate is a general-purpose Rust analysis toolkit, some tools are specific to the [APAS-AI](https://github.com/briangmilnes/APAS-AI) project (Algorithm Performance Analysis System - a comprehensive Rust implementation of algorithms and data structures). These APAS-specific tools handle chapter migration (Chap18 → Chap19), module naming conventions, and parallelism patterns specific to that codebase.

### Key Features

- **Zero String Hacking:** All tools use proper AST parsing for accuracy
- **Automatic Fixes:** Many review tools have corresponding fix tools
- **Emacs Integration:** All output is Emacs-clickable for easy navigation
- **Consistent Interface:** Standard arguments across all tools
- **Comprehensive Logging:** Three-level logging system (stdout, per-tool logs, comprehensive log)
- **Deterministic Output:** Sorted file lists and error messages for CI/CD

### Use Cases

- **Code Quality Enforcement:** Detect naming conventions, structure violations, and anti-patterns
- **Automated Refactoring:** Bulk transformations (imports, UFCS, type bounds, etc.)
- **Test Coverage Analysis:** Identify untested public functions (99.6% accuracy)
- **Parallelism Auditing:** Detect inherent and transitive parallel operations
- **Codebase Migration:** Automated migration between module versions (e.g., Chap18 → Chap19)

---

## Design Principles

### 1. AST-Only Analysis
**No string hacking.** All code analysis uses `SyntaxKind`, `SyntaxNode`, and `TextRange` from `ra_ap_syntax`. String methods like `.contains()`, `.find()`, `.replace()` are forbidden for code detection.

**Why?** String hacking leads to:
- False positives (matching in comments, strings, identifiers)
- Missed cases (whitespace variations, formatting differences)
- Incorrect transformations (byte boundary errors, scope issues)

### 2. Review + Fix Pairs
Most `review-*` tools have a corresponding `fix-*` tool:
- `review-grouped-imports` → `fix-grouped-imports`
- `review-merge-imports` → `fix-merge-imports`
- `review-min-typing` → `fix-min-typing`

**Workflow:**
1. Run `review-*` to identify issues
2. Review the report
3. Run `fix-*` to automatically apply fixes
4. Run `compile-and-test -c` to verify correctness

### 3. Standard Interface
All tools support:
- `-c` (codebase mode): Analyze `src/`, `tests/`, `benches/`
- `-d <dir>`: Analyze specific directory
- `-f <file>`: Analyze specific file
- `-m <module>`: Search for module by name

Output format:
```
path/to/file.rs:LINE:  Description of issue
```

### 4. Review Tool Dispatcher
Run all review tools at once or individually through a single interface:

```bash
# Run all review tools
rusticate-review all -c

# Run specific tool
rusticate-review string-hacking -c
rusticate-review test-functions -f src/lib.rs
```

**Dual Logging:**
1. **stdout** - Real-time output as tools run
2. **`analyses/rusticate-review.log`** - Complete log with ALL output from ALL tools (tool details + timing + summary)
3. **`analyses/review_<tool>.log`** - Individual tool logs (when tools run standalone)

The comprehensive review log captures everything so you can easily review findings from a full `rusticate-review all -c` run without scrolling through terminal output.

### 5. Deterministic Output
- File lists are sorted
- Error messages are sorted by file path and line number
- Consistent for CI/CD and version control

---

## Installation

### Prerequisites
- Rust 1.70+ (uses `ra_ap_syntax` for AST parsing)
- Cargo

### Build from Source
```bash
git clone https://github.com/yourusername/rusticate.git
cd rusticate
cargo build --release
```

Binaries will be in `target/release/`.

### Add to PATH (Optional)
```bash
export PATH="$PATH:/path/to/rusticate/target/release"
```

---

## Tool Categories

### Compilation & Testing

Tools for building and testing Rust codebases.

| Tool | Purpose |
|------|---------|
| `compile` | Compile `src/` with detailed error reporting |
| `compile-and-test` | Compile `src/` + run tests with colored output |
| `compile-src-tests-benches-run-tests` | Full grind: compile all + run tests |

**Example:**
```bash
compile -c                    # Compile entire codebase
compile-and-test -d src/      # Compile and test src/
```

---

### Code Analysis (Review)

Tools that detect issues but don't modify code. All output to `analyses/tool-name.log`.

#### Import & Module Structure

| Tool | Description |
|------|-------------|
| `review-grouped-imports` | Detect grouped imports `use mod::{A, B};` |
| `review-merge-imports` | Find mergeable single-line imports from same module |
| `review-import-order` | Check import statement ordering |
| `review-no-extern-crate` | Detect deprecated `extern crate` usage |
| `review-pub-mod` | Check for `pub mod X {}` declarations (general Rust) |
| `review-non-wildcard-uses` | Analyze non-wildcard `use` statements |
| `review-module-encapsulation` | Check module visibility and encapsulation |

#### Naming Conventions

| Tool | Description |
|------|-------------|
| `review-snake-case-filenames` | Enforce `snake_case` file names |
| `review-pascal-case-filenames` | Enforce `PascalCase` file names |
| `review-struct-file-naming` | Check struct name matches file name |
| `review-variable-naming` | Check variable naming conventions |

#### Type System & Traits

| Tool | Description |
|------|-------------|
| `review-impl-trait-bounds` | Detect unnecessary trait bounds in impl blocks |
| `review-trait-bound-mismatches` | Find inconsistent trait bounds |
| `review-where-clause-simplification` | Identify simplifiable `where` clauses |
| `review-min-typing` | Detect redundant type annotations |
| `review-impl-order` | Check impl block ordering |
| `review-inherent-and-trait-impl` | Analyze inherent vs trait implementations |
| `review-inherent-plus-trait-impl` | Detect both inherent and trait impls in same file |
| `review-public-only-inherent-impls` | Check public inherent impl restrictions |
| `review-single-trait-impl` | Enforce one trait per impl block |
| `review-redundant-inherent-impls` | Find duplicate inherent implementations |
| `review-no-trait-method-duplication` | Detect method name conflicts in traits |
| `review-trait-definition-order` | Check trait definition ordering |
| `review-trait-method-conflicts` | Find conflicting trait method names |
| `review-trait-self-usage` | Analyze `Self` usage in traits |
| `review-typeclasses` | Analyze typeclass patterns |
| `analyze-review-typeclasses` | Deep typeclass analysis |

#### UFCS & Qualified Paths

| Tool | Description |
|------|-------------|
| `review-unnecessary-ufcs-and-qualified-paths` | Detect simplifiable UFCS calls |
| `review-simplifiable-ufcs` | Identify UFCS that could be method calls |
| `review-minimize-ufcs-call-sites` | Find excessive UFCS usage |
| `review-qualified-paths` | Analyze qualified path usage |

#### Code Structure

| Tool | Description |
|------|-------------|
| `review-stub-delegation` | Detect stub methods that delegate |
| `review-internal-method-impls` | Check internal method implementations |
| `review-duplicate-methods` | Find duplicate method definitions |
| `review-comment-placement` | Check comment placement conventions |

#### Testing

| Tool | Description |
|------|-------------|
| `review-test-modules` | Analyze test module structure |
| `review-test-functions` | **Check test coverage for public functions (99.6% accuracy)** |
| `review-integration-test-structure` | Validate integration test organization |
| `review-bench-modules` | Analyze benchmark module structure |
| `review-duplicate-bench-names` | Find duplicate benchmark names |

#### Parallelism & Concurrency (Mt/St Analysis)

| Tool | Description |
|------|-------------|
| `review-inherent-and-transitive-mt` | **Detect inherent/transitive parallel operations in Mt modules** |
| `review-st-mt-consistency` | Check Single-threaded (St) vs Multi-threaded (Mt) consistency |
| `review-mt-per` | Analyze Mt/Per module relationships |
| `review-stt-compliance` | Check StT trait compliance |

#### Doctests

| Tool | Description |
|------|-------------|
| `review-doctests` | Analyze doctest completeness and correctness |

#### Chapter Migration (APAS-specific)

| Tool | Description |
|------|-------------|
| `review-chap18-chap19` | Identify Chap18/Chap19 import issues |

#### Meta-Analysis

| Tool | Description |
|------|-------------|
| `review-string-hacking` | **Detect string manipulation that should use AST** |
| `review-summary-accuracy` | Verify summary counts match actual violations |
| `review-logging` | Check that binaries have dual stdout+file logging |

---

### Code Fixing

Tools that automatically modify code. **Always run tests after fixing!**

#### Import Management

| Tool | Description |
|------|-------------|
| `fix-grouped-imports` | Expand `use mod::{A, B};` to individual imports |
| `fix-merge-imports` | Merge single-line imports from same module |
| `fix-import-order` | Sort import statements |
| `fix-our-uses-to-wildcards` | Convert to wildcard imports for crate modules |
| `fix-non-wildcard-uses` | Fix non-wildcard use statements |
| `fix-test-trait-imports` | Fix trait imports in tests |

#### Type System

| Tool | Description |
|------|-------------|
| `fix-min-typing` | Remove redundant type annotations |
| `fix-no-pub-type` | Fix public type visibility issues |

#### UFCS & Qualified Paths

| Tool | Description |
|------|-------------|
| `fix-unnecessary-ufcs` | Simplify UFCS to method calls (use cautiously!) |

#### Code Structure

| Tool | Description |
|------|-------------|
| `fix-stub-delegation` | Fix stub delegation patterns |
| `fix-duplicate-methods` | Remove duplicate method definitions |
| `fix-duplicate-method-call-sites` | Deduplicate method call sites |

#### Doctests

| Tool | Description |
|------|-------------|
| `fix-doctests` | Add missing `use` statements to doctests |

#### Logging & Infrastructure

| Tool | Description |
|------|-------------|
| `fix-logging` | Add dual stdout+file logging to binaries |

#### Chapter Migration (APAS-specific)

| Tool | Description |
|------|-------------|
| `fix-chap18-to-chap19` | Migrate imports from Chap18 to Chap19 |
| `fix-chap18-to-chap19-per` | Migrate `ArraySeqStPer` imports |
| `fix-chap18-chap19-both` | Remove Chap18 imports when Chap19 exists |

#### Module Structure

| Tool | Description |
|------|-------------|
| `fix-pub-mod` | Add missing `pub mod X {}` declarations (general Rust) |

#### General

| Tool | Description |
|------|-------------|
| `fix` | General-purpose fix tool (delegates to specific fixers) |

---

### Code Metrics

Tools that count or measure code properties.

| Tool | Description |
|------|-------------|
| `count-as` | Count UFCS (Universal Function Call Syntax) usage: `<Type as Trait>::method` patterns |
| `count-loc` | Count lines of code (excluding comments, blanks) |
| `count-vec` | Count `Vec` usage |
| `count-where` | Count `where` clause usage |

**Example:**
```bash
count-loc -c                  # Count LOC in entire codebase
count-where -d src/Chap19/    # Count where clauses in Chap19
```

---

### Parsing & Utilities

| Tool | Description |
|------|-------------|
| `parse` | Parse Rust file and dump AST for debugging |
| `review` | General-purpose review tool (delegates to specific reviewers) |
| `rusticate` | Main CLI entry point |

---

## Standard Arguments

All tools support these common arguments:

### `-c` (Codebase Mode)
Analyze `src/`, `tests/`, and `benches/` directories.

```bash
review-test-functions -c
```

### `-d <directory>` (Directory Mode)
Analyze a specific directory recursively.

```bash
review-merge-imports -d src/Chap19/
```

### `-f <file>` (File Mode)
Analyze a single file.

```bash
fix-grouped-imports -f src/lib.rs
```

### `-m <module>` (Module Search Mode)
Search for files matching a module name pattern.

```bash
review-impl-order -m ArraySeq
```

Searches for files containing "ArraySeq" in their name.

### `-l <language>` (Language Mode)
Specify the target language for analysis. Defaults to `Rust`.

```bash
count-loc -c                           # Rust (default)
count-loc -l Rust -d src/             # Explicit Rust
count-loc -l Verus -d ~/verus-code/   # Verus language
```

**Supported Languages:**
- **`Rust`** (default) - Standard Rust code
- **`Verus`** - Verus verification language (distinguishes `spec`/`proof`/`exec` functions)

See [Tools with Verus Support](#tools-with-verus-support) for which tools support Verus analysis.

### `-p <project>` (Project Mode)
Enable project-specific tools (e.g., APAS-specific checks).

```bash
rusticate-review all -c -p APAS   # Run all tools including APAS-specific
```

Currently supports: `APAS` (for APAS-AI project)

---

## Tool Reference

### Key Tools by Use Case

#### 🔥 Most Used

**Test Coverage Analysis**
```bash
review-test-functions -c
```
- Detects untested public functions
- 99.6% accuracy on large codebases
- Handles trait implementations (Display, Debug, PartialEq)
- Filters out nested helper functions
- Output: `analyses/review_test_functions.txt`
- **Limitations:**
  - **Inlining:** Measures syntactic calls in AST, not actual execution. Trivial functions (e.g., `new()`, `into_iter()`) may be inlined by the compiler and show as "tested" even though llvm-cov reports 0 executions.
  - **Type Disambiguation:** Cannot distinguish method calls across types without a typed AST (requires full type inference from rustc). Method names like `vertices()` or `edges()` match across ALL types that define them, causing false positives when multiple types share method names. Use `llvm-cov` for execution-based ground truth.

**Parallelism Detection**
```bash
review-inherent-and-transitive-mt -c
```
- Identifies Mt modules with inherent parallelism (`spawn`, `par_iter`, `ParaPair!`)
- Detects transitive parallelism (calls to other parallel methods)
- Fixed-point iteration for intra-module transitivity
- Output: `analyses/review_inherent_and_transitive_mt.log`

**Import Cleanup**
```bash
# 1. Find mergeable imports
review-merge-imports -c

# 2. Merge them
fix-merge-imports -c

# 3. Verify
compile-and-test -c
```

**String Hacking Detection**
```bash
review-string-hacking -c
```
- Finds `.contains()`, `.find()`, `.replace()` on code content
- Ensures tools use AST instead of string manipulation

**Type Annotation Cleanup**
```bash
# 1. Find redundant type annotations
review-min-typing -c

# 2. Remove them
fix-min-typing -c

# 3. Verify
compile-and-test -c
```

#### 🧪 Testing Workflow

```bash
# Full grind cycle
compile-src-tests-benches-run-tests -c

# Or step-by-step
compile -c                    # Compile src/
compile-and-test -c           # Compile + run tests
```

#### 📊 Metrics & Reporting

```bash
# Code metrics
count-loc -c                  # Lines of code
count-where -c                # Where clause usage
count-vec -c                  # Vec usage

# Quality checks
review-summary-accuracy       # Verify report accuracy
review-test-functions -c      # Test coverage
```

#### 🔧 Automated Fixes

```bash
# Import management
fix-merge-imports -c          # Merge imports
fix-grouped-imports -c        # Expand grouped imports
fix-import-order -c           # Sort imports

# Type system
fix-min-typing -c             # Remove redundant types
fix-no-pub-type -c            # Fix public type issues

# Doctests
fix-doctests -c               # Add missing use statements
```

---

## Development

### Adding a New Tool

1. **Create the binary:**
   ```bash
   touch src/bin/review_my_feature.rs
   ```

2. **Add to `Cargo.toml`:**
   ```toml
   [[bin]]
   name = "review-my-feature"
   path = "src/bin/review_my_feature.rs"
   ```

3. **Use StandardArgs:**
   ```rust
   use rusticate::StandardArgs;
   use anyhow::Result;
   
   fn main() -> Result<()> {
       let args = StandardArgs::parse()?;
       // Use args.paths, args.is_module_search, args.base_dir
       Ok(())
   }
   ```

4. **Use AST parsing (never string hacking):**
   ```rust
   use ra_ap_syntax::{SourceFile, SyntaxKind, ast};
   
   let parsed = SourceFile::parse(&content, Edition::Edition2021);
   for node in parsed.tree().syntax().descendants() {
       if node.kind() == SyntaxKind::USE {
           // Process with AST methods
       }
   }
   ```

5. **Add logging:**
   ```rust
   use std::fs::File;
   use std::io::Write;
   
   macro_rules! log {
       ($logger:expr, $($arg:tt)*) => {{
           let msg = format!($($arg)*);
           println!("{}", msg);
           if let Some(ref mut f) = $logger {
               let _ = writeln!(f, "{}", msg);
           }
       }};
   }
   ```

6. **Sort output:**
   ```rust
   let mut files = rusticate::find_rust_files(&paths, base_dir, is_module_search)?;
   // Already sorted by find_rust_files()
   ```

### Testing a Tool

```bash
# Build
cargo build --release --bin review-my-feature

# Test on single file
./target/release/review-my-feature -f src/lib.rs

# Test on directory
./target/release/review-my-feature -d src/

# Test on full codebase
./target/release/review-my-feature -c

# Verify compilation still works
compile-and-test -c
```

### Code Quality Rules

1. **NO STRING HACKING:** Use `SyntaxKind`, not `.contains()` or `.find()`
2. **Sort output:** Use `find_rust_files()` which returns sorted paths
3. **Emacs-clickable:** Format as `path/to/file.rs:LINE:  message`
4. **Dual logging:** Write to both stdout and `analyses/tool-name.log`
5. **Test after fixing:** Always run `compile-and-test -c` after auto-fixes

---

## Examples

### Example 1: Find and Fix Mergeable Imports

```bash
# Step 1: Identify mergeable imports
$ review-merge-imports -d src/Chap19/

src/Chap19/ArraySeqStEph.rs:5:  Mergeable imports:
  use std::fmt::Display;
  use std::fmt::Formatter;

# Step 2: Apply fix
$ fix-merge-imports -d src/Chap19/

Fixed src/Chap19/ArraySeqStEph.rs (merged 2 imports)

# Step 3: Verify
$ compile-and-test -c
✅ All tests passed
```

### Example 2: Check Test Coverage

```bash
$ review-test-functions -c

================================================================================
SUMMARY:
  Total public functions: 2,360
  Functions with test coverage: 2,350 (99.6%)
  Functions without test coverage: 10 (0.4%)
================================================================================

src/Chap50/Probability.rs:118:  Probability::add - NO TEST COVERAGE
src/Chap50/Probability.rs:124:  Probability::sub - NO TEST COVERAGE
...
```

### Example 3: Detect Parallel Operations

```bash
$ review-inherent-and-transitive-mt -c

INHERENT PARALLELISM:
  Chap19/ArraySeqMtEph.rs - scan() [line 159] (uses ParaPair!)
  Chap19/ArraySeqMtEph.rs - reduce() [line 186] (uses ParaPair!)

TRANSITIVE PARALLELISM:
  Chap41/AVLTreeSetMtEph.rs - filter() calls ArraySeqMtEph::scan()
  
NON-PARALLEL Mt MODULES:
  Chap06/DirGraphMtEph.rs - No parallel operations detected
```

### Example 4: Remove Redundant Type Annotations

```bash
# Find redundant annotations
$ review-min-typing -d src/

src/lib.rs:45:  Redundant type annotation: let x: i32 = 5;

# Fix automatically
$ fix-min-typing -d src/

Fixed src/lib.rs (removed 3 redundant type annotations)

# Verify
$ compile-and-test -c
✅ All tests passed
```

---

## Tools Tested on APAS-AI Codebase

The following tools have been validated on the large-scale [APAS-AI](https://github.com/briangmilnes/APAS-AI) codebase (2,780+ public functions, 114K+ LOC):

### Compilation & Testing
- ✓ `compile` - Validated across entire codebase
- ✓ `compile-and-test` - Full test suite validation
- ✓ `compile-src-tests-benches-run-tests` - Complete build pipeline

### Code Metrics
- ✓ `count-loc` - Counted 114,561 LOC: 45,462 src + 55,213 tests + 13,886 benches (655 files total)
- ✓ `count-as` - Found 1,456 UFCS patterns: 256 src + 942 tests + 258 benches
- ✓ `count-vec` - Found 721 Vec usages: 453 src + 198 tests + 70 benches
- ✓ `count-where` - Found 213 where clauses: 213 src + 0 tests + 0 benches

### Review Tools (Extensively Tested)
- ✓ `review-test-functions` - Analyzed 2,780 functions, 88.9% coverage detection
- ✓ `review-inherent-and-transitive-mt` - Detected parallelism in 52+ Mt modules
- ✓ `review-string-hacking` - Found violations in 11 rusticate tools
- ✓ `review-summary-accuracy` - Validated all analysis reports
- ✓ `review-merge-imports` - Identified mergeable imports
- ✓ `review-grouped-imports` - Found grouped imports to expand
- ✓ `review-unnecessary-ufcs-and-qualified-paths` - Detected simplifiable UFCS
- ✓ `review-simplifiable-ufcs` - Analyzed UFCS patterns
- ✓ `review-non-wildcard-uses` - Categorized import patterns
- ✓ `review-chap18-chap19` - Found module migration issues

### Fix Tools (Production Tested)
- ✓ `fix-grouped-imports` - Expanded 100+ grouped imports successfully
- ✓ `fix-merge-imports` - Merged imports, preserving aliases
- ✓ `fix-chap18-to-chap19-per` - Migrated 13 modules
- ✓ `fix-chap18-chap19-both` - Resolved trait ambiguities
- ✓ `fix-logging` - Added dual logging to rusticate tools
- ✓ `fix-doctests` - Fixed doctest compilation issues
- ✓ `fix-binary-logging` - Injected logging boilerplate
- ✓ `fix-unnecessary-ufcs` - Tested but reverted (ambiguity issues)

### Results Summary
- **Test Coverage Detection:** 88.9% (2,471/2,780 functions)
- **Parallel Methods Detected:** 156 inherent, 340+ transitive
- **Code Transformations:** 500+ automated fixes applied
- **Zero Regressions:** All fixes validated with `compile-and-test -c`

---

## Tools with Verus Support

The following tools support Verus language analysis via the `-l Verus` flag. Verus is a verification-aware Rust dialect that uses `spec`, `proof`, and `exec` modifiers on functions.

| Tool | Verus Support | Description | Tested On |
|------|---------------|-------------|-----------|
| `count-loc` | ✓ Yes | Counts lines of code, distinguishing `spec`/`proof`/`exec` functions and `proof {}` blocks in Verus code | human-eval-verus (264 files, 22K LOC): 866 Spec / 1,295 Proof / 5,185 Exec |

### Usage Example

```bash
# Count LOC in Verus codebase
rusticate-count-loc -l Verus -d ~/verus-project/

# Output format for Verus:
# Verus LOC (Spec/Proof/Exec)
#   36/  34/  83 filename.rs
#   ...
#   866/ 1,295/ 5,185 total
#   22,329 total lines
```

### Implementation Details

**Verus Function Modifiers:**
- `spec fn` → Specification functions (counted as Spec LOC)
- `proof fn` → Proof functions (counted as Proof LOC)
- `global fn` → Global specifications (counted as Spec LOC)
- `layout fn` → Layout specifications (counted as Spec LOC)
- Regular `fn` → Executable functions (counted as Exec LOC)
- `proof { }` blocks → Proof blocks within exec functions (counted as Proof LOC)

**AST Parsing:** Walks the `verus! {}` macro's token tree directly, using `SyntaxKind::FN_KW` and `SyntaxKind::IDENT` to identify function types without string hacking.

---

## License

GPL-3.0 - See LICENSE file for details.

Copyright (C) Brian G. Milnes 2025

## Development

### Testing with APAS-AI

Many rusticate tools have been extensively tested on the [APAS-AI](https://github.com/briangmilnes/APAS-AI) codebase. To run tests against APAS-AI:

```bash
# From rusticate project root, check out APAS-AI
mkdir -p APAS-AI-copy
cd APAS-AI-copy
git clone https://github.com/briangmilnes/APAS-AI.git apas-ai
cd ..

# Run count tools on APAS-AI
target/release/rusticate-count-loc -c APAS-AI-copy/apas-ai
target/release/rusticate-count-as -c APAS-AI-copy/apas-ai
target/release/rusticate-count-vec -c APAS-AI-copy/apas-ai
target/release/rusticate-count-where -c APAS-AI-copy/apas-ai

# Run review tools on APAS-AI
target/release/rusticate-review string-hacking -c APAS-AI-copy/apas-ai
target/release/rusticate-review test-functions -c APAS-AI-copy/apas-ai
```

The APAS-AI codebase is checked out in `APAS-AI-copy/apas-ai/` by convention to avoid confusion with the main rusticate project.

## Contributing

Contributions welcome! Please ensure:
1. All tools use AST parsing (no string hacking)
2. Output is deterministic (sorted)
3. Add corresponding fix tool for each review tool
4. Tests pass after automatic fixes
5. Run `rusticate-review string-hacking` on your changes

## Authors

Brian G. Milnes
