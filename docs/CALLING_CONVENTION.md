# Rusticate Calling Convention

## Binary Naming Convention

**Pattern**: `rusticate-<category>-<operation>`

All executables follow this hierarchical naming:
- Prefix: `rusticate-` (namespace)
- Category: `review`, `fix`, `find`, `count`
- Operation: Specific check or fix (kebab-case)

### Examples
```
rusticate-review                          # Master review runner
rusticate-fix                             # Master fix runner
rusticate-parse                           # AST parser

rusticate-review-module-encapsulation     # Check module encapsulation (MANDATORY)
rusticate-review-no-extern-crate          # Check for forbidden extern crate
rusticate-review-import-order             # Check import ordering

rusticate-fix-import-order                # Fix import ordering
rusticate-fix-where-clause                # Simplify where clauses
```

### Source File Mapping
- **Source**: `src/bin/review_module_encapsulation.rs` (snake_case, Rust convention)
- **Binary**: `rusticate-review-module-encapsulation` (kebab-case, CLI convention)
- **Cargo.toml**: Explicit `[[bin]]` sections map source to binary names

## Command Line Interface

### Standard Arguments

**All tools follow the same simple pattern:**

```bash
rusticate-<tool> [OPTIONS]
```

**Options:**
- `-c, --codebase`: Analyze src/, tests/, benches/ (default)
- `-d, --dir DIR...`: Analyze specific directories
- `-f, --file FILE`: Analyze a single file
- `-m, --module NAME`: Find module in src/ and its tests/benches
- `-h, --help`: Show usage information
- `--dry-run`: (Fix tools only) Show what would change without modifying

### Path Resolution

Tools intelligently handle different target types:

#### 1. **Full Codebase** (default - scans src/, tests/, benches/)
```bash
cd ~/projects/my-project
rusticate-review-import-order
# Scans: src/, tests/, benches/

# Or explicitly:
rusticate-review-import-order -c
rusticate-review-import-order --codebase
```

#### 2. **Specific Directories**
```bash
# Single directory
rusticate-review-import-order -d src

# Multiple directories
rusticate-review-import-order -d src tests benches

# Absolute paths
rusticate-review-import-order -d /path/to/project/src
```

#### 3. **Single File**
```bash
rusticate-review-import-order -f src/lib.rs
rusticate-review-import-order --file src/parser.rs
```

#### 4. **Module Name** (finds module source + tests + benches)
```bash
# Finds ArraySeqStEph.rs in src/
# Then finds test_ArraySeqStEph.rs in tests/ (if it exists)
# Then finds bench_ArraySeqStEph.rs in benches/ (if it exists)
rusticate-review-import-order -m ArraySeqStEph
rusticate-review-import-order --module ArraySeqStEph
```

### Usage Patterns

#### Review Tools (Read-Only Analysis)

```bash
# Review entire project
rusticate-review-module-encapsulation ~/projects/my-project

# Review just src/
rusticate-review-module-encapsulation ~/projects/my-project/src

# Review single file
rusticate-review-module-encapsulation src/lib.rs

# Review current project
cd ~/projects/my-project && rusticate-review-module-encapsulation
```

#### Fix Tools (Modifies Files)

```bash
# Dry run (preview changes)
rusticate-fix-import-order ~/projects/my-project --dry-run

# Apply fixes
rusticate-fix-import-order ~/projects/my-project

# Fix single file
rusticate-fix-import-order src/lib.rs

# Fix just tests
rusticate-fix-import-order ~/projects/my-project/tests
```

#### Count Tools (Metrics)

```bash
# Count LOC in entire project
rusticate-count-loc ~/projects/my-project

# Count in specific directory
rusticate-count-loc ~/projects/my-project/src

# Count in single file
rusticate-count-loc src/lib.rs
```

## Exit Codes

Following Unix conventions:

- **0**: Success (no violations found, or fixes applied successfully)
- **1**: Violations found (review tools) or fix failed
- **2**: Invalid arguments or usage
- **3**: File not found or read error

For review tools:
- Exit 0 if all checks pass
- Exit 1 if any violations found
- Output violations to stdout in Emacs compile-mode compatible format

For fix tools:
- Exit 0 if fixes applied successfully
- Exit 1 if unable to apply fixes or compilation fails after fixes

## Error Location Format (MANDATORY)

**All error/violation locations MUST use the standard compiler format:**

```
filename:line: message
```

Or with column (when available):
```
filename:line:column: message
```

**Format:**
```
Entering directory 'path/to/target'

src/Chap18/ArraySeqStEph.rs:23: fn outside pub mod
  fn new(length: N, init_value: T) -> Self;
src/parser.rs:45:12: expected semicolon
```

**Requirements:**

1. First line: `Entering directory 'path'` - tells Emacs the base directory
2. Blank line after directory
3. Each violation: `filename:line: message` on first line
4. Optional: Code context indented on next line
5. Relative paths from the "Entering directory" path
6. Line numbers are 1-indexed

**Rationale:**

This format is recognized by:
- Emacs compile-mode (`M-x compile`, `C-x \``)
- Vim quickfix (`:make`, `:cn`)
- Most IDE error parsers
- Standard Unix tooling (grep -n, etc.)

## Timing Standard (MANDATORY)

**All tools MUST report execution time as the last line of output:**

- **Format**: `Completed in {time}ms` (milliseconds, no other units)
- **Position**: Always the final line of output
- **Precision**: Integer milliseconds (no decimal places)
- **Consistency**: Same format for all tools (no variation)

**Examples:**
```
# Counting tool
SRC LOC
   808 total
...
Total LOC
186845 total
Completed in 245ms

# Review tool
src/lib.rs:42:5: error: Code found outside pub mod block
✗ Found 1 violation
Completed in 1523ms

# Fix tool
Fixed 15 files
Completed in 3891ms
```

**Implementation:**
```rust
use std::time::Instant;

fn main() -> Result<()> {
    let start = Instant::now();
    
    // ... tool logic ...
    
    let elapsed = start.elapsed().as_millis();
    println!("Completed in {}ms", elapsed);
    Ok(())
}
```

## Output Formats

### Text Format (Default)

Human-readable, Emacs compile-mode compatible:

```
src/lib.rs:42:5: error: Code found outside pub mod block
src/lib.rs:108:1: warning: extern crate is forbidden
```

Format: `<file>:<line>:<col>: <severity>: <message>`

### JSON Format

Machine-readable for tooling integration:

```json
{
  "violations": [
    {
      "file": "src/lib.rs",
      "line": 42,
      "column": 5,
      "severity": "error",
      "code": "module-encapsulation",
      "message": "Code found outside pub mod block"
    }
  ],
  "summary": {
    "files_checked": 15,
    "violations": 2,
    "errors": 1,
    "warnings": 1
  }
}
```

## Python Script Migration

Each Python script maps to a Rust binary:

| Python Script | Rust Binary | Status |
|--------------|-------------|---------|
| `scripts/review.py` | `rusticate-review` | ✓ Structure ready |
| `scripts/rust/review_no_extern_crate.py` | `rusticate-review-no-extern-crate` | ✓ Structure ready |
| `scripts/rust/src/review_module_encapsulation.py` | `rusticate-review-module-encapsulation` | ✓ Structure ready |
| `scripts/rust/fix_import_order.py` | `rusticate-fix-import-order` | ✓ Structure ready |
| ... (84 more scripts) | ... | ⏳ To be implemented |

## Installation

### From Source

```bash
cd /home/milnes/projects/rusticate
cargo build --release
```

Binaries will be in `target/release/rusticate-*`

### Using Cargo Install

```bash
cargo install --path .
```

Installs all binaries to `~/.cargo/bin/`

### Selective Build

Build specific binaries:

```bash
# Build single binary
cargo build --release --bin rusticate-review

# Build multiple
cargo build --release --bin rusticate-review --bin rusticate-fix
```

## Integration with Existing Workflow

### Emacs Integration

All text output is compatible with Emacs compile mode:

```elisp
(defun rusticate-review ()
  "Run rusticate review on current file."
  (interactive)
  (compile (format "rusticate-review --path %s" buffer-file-name)))
```

### Script Replacement

To replace Python scripts in CI/CD:

```bash
# Old Python way
python3 scripts/rust/src/review_module_encapsulation.py

# New Rust way
rusticate-review-module-encapsulation --path src/
```

### Make/Justfile Integration

```makefile
review:
    rusticate-review --path src/

fix:
    rusticate-fix --path src/ --dry-run

fix-apply:
    rusticate-fix --path src/ --in-place
```

## Arguments Style Philosophy

- **Explicit over implicit**: Use `--path` not positional args
- **Consistent naming**: All tools use same arg names (`--path`, `--format`, etc.)
- **Safe by default**: Fixes require `--in-place`, reviewers never modify
- **Unix-friendly**: Exit codes, stdout/stderr, pipeable JSON
- **Editor-friendly**: Text format works in Emacs, VS Code, vim error lists

## Future Extensions

### Planned Arguments

```bash
# Parallel processing
--jobs <N>              # Process N files in parallel

# Rule configuration
--config <FILE>         # Load rules from config file
--rules <RULES>         # Enable specific rules only
--disable <RULES>       # Disable specific rules

# Git integration
--staged                # Only check staged files
--changed               # Only check uncommitted changes
```

### Planned Binaries

Priority order based on MANDATORY rules:

1. `rusticate-review-single-trait-impl` (MANDATORY)
2. `rusticate-review-zero-warnings` (MANDATORY)
3. `rusticate-review-integration-test-structure` (MANDATORY)
4. `rusticate-fix-where-clause-simplification`
5. `rusticate-fix-ufcs-elimination`
... (80+ more)

## Testing the Convention

```bash
# List all binaries
ls target/release/rusticate-*

# Test help for each
for bin in target/release/rusticate-*; do
    echo "=== $bin ==="
    $bin --help
done

# Test on example file
echo 'fn main() { println!("test"); }' > /tmp/test.rs
rusticate-parse --path /tmp/test.rs
```

## Summary

✓ **Namespace**: All binaries prefixed with `rusticate-`
✓ **Hierarchical**: Category + operation in name
✓ **Consistent**: All use same argument conventions
✓ **Unix-friendly**: Standard exit codes, formats
✓ **Editor-friendly**: Emacs compile-mode compatible
✓ **Scalable**: Can add 80+ binaries following same pattern

