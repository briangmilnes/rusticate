# Rusticate Development Rules

## Mandatory: AST-First Development

### Rule 1: All Code Analysis Must Use AST Traversal
- **MUST**: Use `ra_ap_syntax::SyntaxNode` and `ra_ap_syntax::ast` types for analyzing Rust code
- **MUST**: Use `node.descendants()` or `node.children()` for traversing code structure
- **MUST**: Use `SyntaxKind` enum to identify node types (e.g., `CALL_EXPR`, `FN`, `TRAIT`)
- **FORBIDDEN**: Using `.find()`, `.contains()`, regex, or string pattern matching on Rust source code for structural analysis

**Bad Example:**
```rust
if source.contains("fn ") {  // String hacking!
    // find function...
}
```

**Good Example:**
```rust
for node in root.descendants() {
    if node.kind() == SyntaxKind::FN {
        if let Some(func) = ast::Fn::cast(node) {
            // Process function...
        }
    }
}
```

### Rule 2: All Code Transformations Must Use AST Offsets
- **MUST**: Use `node.text_range().start()` and `node.text_range().end()` to get byte offsets
- **MUST**: Build replacements as `Vec<(start_offset, end_offset, new_text)>` 
- **MUST**: Sort replacements by reverse offset before applying to preserve positions
- **FORBIDDEN**: Using string operations like `.replace()`, `.split()`, manual parenthesis matching, or character-by-character parsing

**Bad Example:**
```rust
// String hacking with manual depth counting!
let mut depth = 0;
for (i, ch) in source.chars().enumerate() {
    if ch == '(' { depth += 1; }
    if ch == ')' { depth -= 1; }
}
```

**Good Example:**
```rust
if let Some(call_expr) = ast::CallExpr::cast(node) {
    if let Some(arg_list) = call_expr.arg_list() {
        let args: Vec<_> = arg_list.args().collect();
        let start: usize = node.text_range().start().into();
        let end: usize = node.text_range().end().into();
        replacements.push((start, end, new_text));
    }
}
```

### Rule 3: Acceptable String Operations
String operations are **ONLY** acceptable for:
- Formatting output messages and error reports
- Building file paths (`PathBuf`, `Path::join()`)
- CLI argument parsing (before source code is involved)
- Extracting final text from AST nodes (`.to_string()` on AST nodes)
- Comparing identifiers extracted from AST nodes

String operations are **NEVER** acceptable for:
- Detecting Rust syntax structures in source code
- Finding function calls, trait definitions, impl blocks, etc.
- Transforming code by text manipulation
- Parsing parameter lists, type signatures, etc.

### Rule 4: Red Flags - Patterns That Indicate String Hacking

If you see these patterns operating on Rust source code, it's **WRONG**:
- `source.find("fn ")` - Use `SyntaxKind::FN`
- `source.contains("impl ")` - Use `SyntaxKind::IMPL`  
- `source.split("::")` - Use `ast::Path` and `path.segments()`
- `source.rfind(|c: char| ...)` - Use AST traversal
- Manual depth counting with `'('` and `')'` - Use `ast::CallExpr` and `.arg_list()`
- `regex!("fn \\w+\\(")` - Use `ast::Fn` and `.param_list()`
- `line.trim_start_matches('{')` - Use AST node ranges

### Rule 5: Verification of Transformations
All code transformations **MUST**:
- Parse the result with `SourceFile::parse(source, Edition::Edition2021)`
- Check `parsed.errors().is_empty()` 
- Verify the transformation by compiling the result (`cargo build`)
- Run tests to ensure semantics are preserved

**Required pattern:**
```rust
let new_source = apply_transformation(&old_source)?;

// Verify it parses
let parsed = SourceFile::parse(&new_source, Edition::Edition2021);
if !parsed.errors().is_empty() {
    return Err(anyhow::anyhow!("Transformation produced invalid syntax"));
}

// Write and compile
fs::write(file_path, &new_source)?;
```

### Rule 6: Lossless Preservation
When transforming code, **MUST** preserve:
- All comments (doc comments, inline comments)
- All whitespace and indentation (within reason)
- All formatting (unless explicitly fixing it)
- All trivia (preserved automatically by using AST byte offsets)

This is why we use `ra_ap_syntax` (lossless) instead of `syn` (lossy).

## Import Style Rules

### Rule 7: Explicit Imports Required
- **MUST**: Use explicit imports, never wildcards for external crates
- **FORBIDDEN**: `use external_crate::*;` or `use external_crate::module::*;`
- **EXCEPTION**: `use crate::Types::Types::*;` within APAS codebase (internal convention)
- **WHY**: External crates can export types like `String`, `Option`, `Result` that conflict with std types

**Bad Example:**
```rust
use ra_ap_syntax::ast::*;  // Imports ast::String, conflicts with std::string::String!
```

**Good Example:**
```rust
use ra_ap_syntax::ast::{self, AstNode, HasVisibility, HasName, HasArgList};
// Or use qualified paths: ast::Fn, ast::Trait, etc.
```

**Rationale**: Libraries like `ra_ap_syntax` export `ast::String` (for AST nodes), which shadows `std::string::String`. Wildcard imports make code fragile and cause surprising compilation errors when libraries add new exports.

## Code Organization Rules

### Rule 8: Module Encapsulation
- All code in each file **MUST** be wrapped in `pub mod { ... }` block
- Except: `main.rs`, `lib.rs`, test files, and macro-only modules
- See `RustRules.md` for full APAS conventions

### Rule 9: No Warnings Allowed
- **ZERO** compiler warnings permitted
- Use `#[allow(...)]` only with explicit justification in comments
- Unused variables must be prefixed with `_` if intentionally unused

### Rule 10: Copyright Headers
All Rust files **MUST** start with:
```rust
// Copyright (C) Brian G. Milnes 2025
```

### Rule 11: Tool Naming and Structure
- Binary names: `rusticate-<category>-<operation>` (e.g., `rusticate-review-module-encapsulation`)
- Use kebab-case for binary names
- One binary per operation
- Shared code in `src/lib.rs` and modules under `src/`

## Testing Rules

### Rule 12: Comprehensive Testing
- Every review tool **MUST** have a test file in `tests/`
- Tests **MUST** run against `APAS-AI-copy` at specific git commits
- Tests **MUST** verify numeric output (count of violations, etc.)
- Use `#[serial_test::serial]` for tests that modify git state

### Rule 13: Test-Driven Transformation
When implementing fix tools:
1. Write the analysis logic first (AST traversal)
2. Test on a single file
3. Compile the result
4. Run tests on the result  
5. Compile benches on the result
6. Only then scale to multiple files

## Output Rules

### Rule 14: Emacs Compile-Mode Compatibility
All review tool output **MUST** follow rustc format:
```
Entering directory 'path/to/dir'

path/to/file.rs:line:column: message
    code context (optional)
```

See `docs/CALLING_CONVENTION.md` for full spec.

### Rule 15: Standard Arguments
All tools **MUST** use `rusticate::StandardArgs` for CLI parsing:
- `-c, --codebase`: Entire codebase (src/ tests/ benches/)
- `-d, --dir DIR`: Specific directory  
- `-f, --file FILE`: Single file
- `-m, --module NAME`: Module (src/Name.rs, tests/test_Name.rs, benches/bench_Name.rs)
- `--help`: Usage information

### Rule 16: Timing Standard
All tools **MUST** output timing as the last line:
```
Completed in Xms
```

## Documentation Rules

### Rule 17: Document Transformations
Complex transformations **MUST** be documented in `docs/`:
- Describe the AST manipulation steps
- Show before/after examples
- Explain limitations and edge cases

### Rule 18: Logging Conventions
Follow `docs/LOGGING_CONVENTIONS.md`:
- Use `format_number()` for counts (comma separation)
- Use physics-style units: "1,234 LOC", "42 violations"
- Include summary lines with totals

## Meta-Rule: When in Doubt, Use AST

**If you're unsure whether to use string operations or AST:**
- If it involves Rust syntax structure → **USE AST**
- If it's analyzing code → **USE AST**
- If it's transforming code → **USE AST** 
- If it's just formatting output → String operations OK

**The test**: Could this logic break if I add comments or whitespace to the code?
- If YES → You're string hacking, use AST instead
- If NO → Probably safe, but double-check


