# Rusticate Logging Conventions

## Core Principles

1. **All output goes to stdout** - Never use stderr for normal operation
2. **Emacs compile-mode compatible** - All error/warning locations must be clickable
3. **Structured and parseable** - Other tools can analyze our output
4. **Human-readable first** - Readable in terminal and Emacs
5. **No interactive prompts** - Tools run unattended

## Output Format

### Location Format (MANDATORY)
All issues, findings, or locations must follow this format:
```
filename:line: message
filename:line:column: message  (when column is known)
```

Example:
```
src/Chap05/SetStEph.rs:104: impl<T: Eq + Hash> SetStEph<T> { (for external type) - BUG
```

### Directory Context (MANDATORY for multi-file tools)
First line of output when processing multiple files:
```
Entering directory '/absolute/path/to/search/root'
```

This tells Emacs the base directory for relative paths.

### Relative vs Absolute Paths
- Use **relative paths** from the search root in all output
- Makes output portable and easier to read
- Emacs uses "Entering directory" to resolve them

### Hierarchical Output
Use tabs (`\t`) to show hierarchy:
```
Module:line: description
Module:line:	item description
Module:line:		sub-item description
Module:line:			detail
```

Example:
```
SetStEph.rs:104:	impl<T: Eq + Hash> SetStEph<T> { (for external type) - BUG
SetStEph.rs:104:		pub methods: size, mem, union
SetStEph.rs:104:		pub functions: empty, singleton
```

## Severity Labels (MANDATORY for review tools)

Every finding must be labeled:
- **OK** - Meets all conventions, no issues
- **WARNING** - Non-critical deviation (informational only)
- **BUG** - Violation of coding rules, must be fixed

Example:
```
MappingStEph.rs:4: pub mod MappingStEph { - OK
MappingStEph.rs:54:	impl<A: Eq + Hash, B: Eq + Hash> MappingStEph<A, B> { (for external type) - BUG
MappingStEph.rs:22:	trait MappingStEphTrait (internal) - WARNING
```

## Numeric Formatting (MANDATORY)

Use comma separators for all counts >= 1,000:
```rust
use rusticate::format_number;

println!("Total files: {}", format_number(1234));  // "1,234"
println!("Total lines: {}", format_number(500));   // "500"
```

## Summary Section (MANDATORY)

Every tool must end with:
1. A separator line: `"=".repeat(80)`
2. "SUMMARY:" header
3. Key metrics with labels and counts
4. Timing: `"Completed in {}ms"`

Example:
```
================================================================================
SUMMARY:
  Total modules analyzed: 142
  Total bugs: 1,234
  Total warnings: 56
Completed in 1,234ms
```

## Exit Codes (MANDATORY)

- **0** - Success, no issues found (or tool doesn't check for issues)
- **1** - Issues found (BUGs or WARNINGs)
- **Non-zero** - Fatal error (file not found, parse error, etc.)

For review tools:
```rust
if has_issues {
    std::process::exit(1);
}
Ok(())  // exits with 0
```

## Timing (MANDATORY)

All tools must report execution time:
```rust
use std::time::Instant;

let start = Instant::now();
// ... do work ...
println!("Completed in {}ms", start.elapsed().as_millis());
```

## Error Handling

### Graceful Degradation
```rust
// Handle broken pipe gracefully
if let Err(e) = writeln!(stdout, "...") {
    if e.kind() == std::io::ErrorKind::BrokenPipe {
        std::process::exit(0);
    }
    return Err(e.into());
}
```

### Parse Errors
When a file can't be parsed:
```rust
println!("{}:1: Parse error: {}", file.display(), error);
// Continue processing other files
```

Don't abort entire run on single file failure.

## Examples

### Review Tool Output
```
Entering directory '/home/user/project/src'

Chap05/SetStEph.rs:4: pub mod SetStEph { - OK
Chap05/SetStEph.rs:16:	struct SetStEph (external) - OK
Chap05/SetStEph.rs:21:	trait SetStEphTrait (external) - OK
Chap05/SetStEph.rs:104:	impl<T: Eq + Hash> SetStEph<T> { (for external type) - BUG
Chap05/SetStEph.rs:104:		pub methods: size, mem, union
Chap05/SetStEph.rs:104:	duplicate method: size [pub inherent SetStEph, internal trait impl SetStEph] - BUG

================================================================================
SUMMARY:
  Total files analyzed: 142
  Total bugs: 87
  Total warnings: 12
Completed in 1,234ms
```

### Count Tool Output
```
Entering directory '/home/user/project'

src/parser.rs: 245 LOC
src/analyzer.rs: 189 LOC
src/fixer.rs: 123 LOC
...

================================================================================
SUMMARY:
  src: 12,345 LOC
  tests: 8,901 LOC
  benches: 2,345 LOC
  total: 23,591 LOC
Completed in 56ms
```

### Fix Tool Output
```
Entering directory '/home/user/project/src'

SetStEph.rs:173: Fixed import order
    - Moved 3 imports to correct section
    - Added blank line between sections

MappingStEph.rs:65: Fixed import order
    - Reordered Types::Types::* imports

================================================================================
SUMMARY:
  Files modified: 2
  Total fixes: 2
Completed in 123ms
```

## Analyze Tool Output

Analysis/Pareto tools parse review tool output:
```
================================================================================
PARETO ANALYSIS: BUGS
================================================================================
   142 ( 45.2%, cumulative  45.2%): duplicate method
    87 ( 27.7%, cumulative  72.9%): inherent impl with pub methods
    52 ( 16.6%, cumulative  89.5%): method with unused self parameter
    21 (  6.7%, cumulative  96.2%): no external trait
    12 (  3.8%, cumulative 100.0%): missing module
--------------------------------------------------------------------------------
TOTAL BUGS: 314

Completed in 2,345ms
```

## Anti-Patterns (DO NOT)

❌ Don't use stderr for normal output
❌ Don't print progress bars or spinners (breaks parsing)
❌ Don't use ANSI color codes (breaks Emacs)
❌ Don't prompt for user input
❌ Don't use debug print macros (`dbg!`, `println!` for debugging)
❌ Don't print without location for findings
❌ Don't forget timing in output
❌ Don't use ambiguous paths (always relative or absolute, with "Entering directory")

## Testing Logging

Verify your tool's output:
1. Run in terminal - should be readable
2. Run in `M-x compile` in Emacs - locations should be clickable (blue)
3. Run through grep/sed/awk - should be parseable
4. Pipe to `head` - should handle broken pipe gracefully
5. Check exit code - `echo $?` after run

## Copyright

Copyright (C) Brian G. Milnes 2025

