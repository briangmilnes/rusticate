# rusticate-analyze-modules: MIR-Based Stdlib Usage Analysis

## Overview

`rusticate-analyze-modules` analyzes Rust standard library usage across large codebases by
parsing MIR (Mid-level Intermediate Representation) files generated during compilation. It
provides detailed statistics on which stdlib modules, types, methods, and traits are
actually used in real-world Rust code.

**Primary Use Case:** Prioritizing verification work for
[Verus](https://github.com/verus-lang/verus) by identifying which stdlib items need formal
specifications to cover the most real-world code.

## Quick Start

```bash
# Step 1: Generate MIR files for your codebases
rusticate-mirify -C ~/projects/RustCodebases -j 6

# Step 2: Analyze stdlib usage
rusticate-analyze-modules -M ~/projects/RustCodebases
```

## The `-M` (MIR Analysis) Mode

The `-M` flag enables multi-project MIR analysis, the primary analysis mode for large-scale stdlib usage studies.

```bash
rusticate-analyze-modules -M /path/to/projects
```

### What It Does

1. **Discovers Projects:** Finds all projects with MIR files in `target/debug/deps/*.mir`
2. **Parses MIR:** Extracts stdlib usage from human-readable MIR text files
3. **Aggregates Statistics:** Counts usage per unique crate (not per call)
4. **Computes Greedy Covers:** Finds minimum stdlib items to cover N% of crates
5. **Generates Report:** Outputs detailed analysis with TOC, abstract, and sections

### Output Sections

The analysis generates an 11-section report:

| Section | Description |
|---------|-------------|
| **i. ABSTRACT** | Key findings: crate counts, coverage statistics |
| **1. OVERVIEW** | What MIR provides and its limitations |
| **2. STDLIB MODULES** | All modules ranked by crate count |
| **3. CRATES WITHOUT STDLIB** | Proc-macros, FFI stubs excluded from analysis |
| **4. DATA TYPES** | Result, Option, Vec, etc. by crate count |
| **5. ALL METHODS/FUNCTIONS** | 11,000+ methods ranked by usage |
| **6. GREEDY COVER: MODULES** | Minimum modules for 70-100% coverage |
| **7. GREEDY COVER: DATA TYPES** | Minimum types for 70-100% coverage |
| **8. GREEDY COVER: METHODS** | Minimum methods for 70-100% coverage |
| **9. GREEDY COVER: METHODS PER TYPE** | Per-type method coverage analysis |
| **10. TRAIT IMPLEMENTATIONS** | Iterator, Clone, Debug, etc. with greedy covers |
| **11. SUMMARY** | Final statistics and timing |

## Understanding MIR Analysis

### What MIR Captures

MIR is Rust's fully-typed intermediate representation. From MIR we extract:

- **Direct method calls:** `Vec::new()`, `Result::unwrap()`
- **Trait method calls:** `<Vec as IntoIterator>::into_iter`
- **Type annotations:** `std::result::Result<T, E>` in locals
- **Constructor calls:** `Option::<T>::Some(...)`
- **Trait implementations:** Which types implement which traits

### MIR Limitations

MIR has inherent limitations for usage analysis:

| Pattern | MIR Representation | Impact |
|---------|-------------------|--------|
| `match result { Ok(x) => ... }` | Numeric discriminant check | Can't detect variant matching |
| `result?` | `<Result as Try>::branch()` | Shows as Try trait, not Result method |
| Type inference | Implicit, not in MIR | Many uses not visible |
| Macros | Expanded before MIR | Macro code appears as regular calls |

**Consequence:** Our coverage percentages are a **lower bound**. Actual stdlib usage is higher than detected.

### Counting Methodology

- **One count per crate:** If crate X calls `Vec::push` 1000 times, it counts as 1 crate using `Vec::push`
- **Crate identification:** MIR filenames include crate names (e.g., `serde-abc123.mir` → `serde`)
- **Hash stripping:** Build hashes removed for deduplication
- **Generic stripping:** `Vec::<T>::push` → `Vec::push` for grouping

## Greedy Set Cover Algorithm

For verification prioritization, we compute the minimum set of stdlib items to cover N% of crates.

### Algorithm

```
1. Start with all crates as "uncovered"
2. While coverage < target:
   a. Find the item covering the most uncovered crates
   b. Add it to the cover set
   c. Mark those crates as "covered"
3. Report cumulative coverage after each addition
```

### Example Output

```
=== 8. GREEDY COVER: METHODS/FUNCTIONS ===

--- Target: 90% (3009 of 3343 crates) ---
   1. std::result::Result::unwrap        + 1893 ( 56.6257%)
   2. alloc::vec::Vec::push              +  412 ( 68.9501%)
   3. core::option::Option::unwrap       +  298 ( 77.8642%)
   4. alloc::string::String::from        +  189 ( 83.5177%)
   5. alloc::vec::Vec::new               +  102 ( 86.5689%)
   6. core::option::Option::is_some      +   67 ( 88.5732%)
   7. alloc::vec::Vec::len               +   31 ( 89.5004%)
   8. std::io::Write::write_all          +   22 ( 90.1585%)
   9. alloc::string::String::new         +   11 ( 90.4873%)

=> 9 methods achieve 90.49% coverage
```

**Insight:** Just 9 methods cover 90% of crates. For Verus verification, these are the highest-priority methods to specify.

## Trait Implementation Analysis

Section 10 shows which traits are most commonly implemented and which methods on those traits are most used.

### Example: `core::iter::Iterator`

```
TRAIT: core::iter::Iterator (104 crates, 697 type::method impls)

Top implementations by type:
  std::ops::Range<usize>::next           10 crates
  core::ops::Range<usize>::next           9 crates
  std::iter::Enumerate<I>::try_fold       7 crates
  ...

Greedy cover: 35 methods to verify

  --- Target: 90% (94 crates) ---
      1. next                           +   86 ( 82.6923%)
      2. try_fold                       +   10 ( 92.3077%)
  => 2 methods achieve 92.3077%
```

**Insight:** Iterator has 70+ methods, but just 2 (`next`, `try_fold`) cover 92% of real usage.

## Command Reference

### `rusticate-mirify`

Generate MIR files for analysis:

```bash
# Basic usage
rusticate-mirify -C ~/projects/RustCodebases

# With options
rusticate-mirify -C ~/projects/RustCodebases \
    -j 6              \  # Cargo parallelism
    --clean           \  # Clean before build
    --clean-artifacts \  # Remove artifacts after, keep MIR
    -m 100               # Limit to 100 projects
```

| Flag | Description |
|------|-------------|
| `-C, --codebase` | Directory of projects (required) |
| `-j, --jobs` | Cargo internal parallelism (default: 1) |
| `-m, --max-codebases` | Limit number of projects |
| `--clean` | Run `cargo clean` before building |
| `--clean-artifacts` | Remove build artifacts after, keep MIR |

**Logs to:** `analyses/rusticate-mirify.log` and `rusticate-mirify.errs`

### `rusticate-analyze-modules`

Analyze stdlib usage:

```bash
# MIR analysis mode
rusticate-analyze-modules -M ~/projects/RustCodebases

# Rust stdlib source analysis
rusticate-analyze-modules -R
```

| Flag | Description |
|------|-------------|
| `-M, --mir` | MIR analysis mode (primary) |
| `-R, --rust-libs` | Count functions in Rust stdlib source |
| `-C, --codebase` | AST-based analysis (without MIR) |
| `-j, --jobs` | Thread count (default: CPU count) |
| `-m, --max-codebases` | Limit number of projects |

**Logs to:** `analyses/analyze_modules_mir.log`

## Sample Results (1036 Projects, 3636 Crates)

From the RustCodebases analysis:

### Coverage Summary

| Category | Total Items | Items for 90% | Items for 99% |
|----------|-------------|---------------|---------------|
| Modules | 1,651 | 5 | 18 |
| Types | 56 | 1 | 3 |
| Methods | 7,039 | 9 | 47 |

### Top Traits by Usage

| Trait | Crates | % Usage |
|-------|--------|---------|
| `core::fmt::Display` | 941 | 28.1% |
| `core::fmt::Debug` | 800 | 23.9% |
| `std::io::Write` | 413 | 12.4% |
| `core::default::Default` | 320 | 9.6% |
| `std::io::Read` | 317 | 9.5% |
| `core::iter::Iterator` | 104 | 3.1% |

### Key Insights for Verus

1. **Result and Option dominate:** `Result::unwrap` alone covers 57% of crates
2. **Iterator is used but few methods:** 104 crates use Iterator, but `next` alone covers 83%
3. **io::Write is critical:** 413 crates, 4 methods cover 100% (`write_all`, `write_fmt`, `write`, `flush`)
4. **8% of crates have no stdlib:** Proc-macros, FFI stubs - can be excluded from verification targets

## Implementation Notes

### MIR Parsing

We parse the human-readable MIR text format using regex patterns:

```rust
// Direct function calls
let stdlib_call_re = Regex::new(r"(std|core|alloc)::[a-zA-Z0-9_:]+::[a-z_][a-zA-Z0-9_]*")?;

// Trait implementations  
let trait_impl_re = Regex::new(r"<[^>]+ as (std|core|alloc)::[^>]+>::([a-z_][a-zA-Z0-9_]*)")?;

// Type annotations
let type_annotation_re = Regex::new(r"(std|core|alloc)::[a-zA-Z0-9_:]+::[A-Z][a-zA-Z0-9]*")?;
```

### Why Not `stable-mir-json`?

We considered using `stable-mir-json` for structured MIR output, but:

1. **Text MIR is sufficient** for counting stdlib usage
2. **stable-mir-json requires nightly** Rust and custom compilation
3. **Performance is adequate** - 30 seconds for 1036 projects

For ML/semantic analysis requiring typed MIR, `stable-mir-json` would be preferred.

### Thread Safety

- Uses `rayon` for parallel MIR file processing
- Per-thread aggregation into local `BTreeMap`s
- Final merge of results after parallel phase
- Greedy cover computation is single-threaded (fast enough)

## Related Documentation

- [Using Rust's MIR](UsingRustsMir.md) - Details on MIR format and parsing
- [Proof Parsers](ProofParsers.md) - Parsers for other proof systems

## Files Generated

| File | Description |
|------|-------------|
| `analyses/analyze_modules_mir.log` | Full analysis log |
| `analyses/rusticate-mirify.log` | MIRification progress |
| `analyses/rusticate-mirify.errs` | Build errors |
| `target/debug/deps/*.mir` | MIR files (per project) |

