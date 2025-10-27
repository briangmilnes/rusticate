//! Review: Trait Definition Order
//!
//! Ensures trait definitions appear BEFORE impl blocks.
//!
//! Correct order:
//! 1. Data structure (struct/enum)
//! 2. Trait definition <- SHOULD BE HERE
//! 3. Inherent impl (impl Type { ... })
//! 4. Custom trait implementations
//! 5. Standard trait implementations
//!
//! Binary: rusticate-review-trait-definition-order

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasName}, SyntaxKind, SourceFile, Edition};
use rusticate::{StandardArgs, find_rust_files};
use std::path::Path;
use std::time::Instant;
use std::fs;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_trait_definition_order.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct Violation {
    struct_name: String,
    struct_line: usize,
    trait_name: String,
    trait_line: usize,
    first_impl_line: usize,
}

fn check_file(file_path: &Path) -> Vec<Violation> {
    // Skip specific files/dirs
    if file_path.file_name().map(|n| n == "Types.rs").unwrap_or(false) {
        return Vec::new();
    }
    if file_path.to_str().map(|s| s.contains("Chap47")).unwrap_or(false) {
        return Vec::new();
    }

    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();

    let mut violations = Vec::new();
    
    // Track state: struct name, first impl line, seen impl
    let mut current_struct: Option<(String, usize)> = None;
    let mut seen_impl_after_struct = false;
    let mut first_impl_line: Option<usize> = None;

    // Process top-level items in order
    for node in root.children() {
        let kind = node.kind();
        let line_num = content[..node.text_range().start().into()]
            .chars()
            .filter(|&c| c == '\n')
            .count() + 1;

        match kind {
            SyntaxKind::STRUCT | SyntaxKind::ENUM => {
                // Reset state for new struct/enum
                if let Some(adt) = ast::Struct::cast(node.clone())
                    .map(|s| s.name().map(|n| n.to_string()))
                    .flatten()
                    .or_else(|| {
                        ast::Enum::cast(node.clone())
                            .and_then(|e| e.name().map(|n| n.to_string()))
                    })
                {
                    current_struct = Some((adt, line_num));
                    seen_impl_after_struct = false;
                    first_impl_line = None;
                }
            }
            SyntaxKind::IMPL => {
                // Mark that we've seen an impl after current struct
                if current_struct.is_some() && !seen_impl_after_struct {
                    seen_impl_after_struct = true;
                    first_impl_line = Some(line_num);
                }
            }
            SyntaxKind::TRAIT => {
                // Check if trait appears after impl
                if let (Some((struct_name, struct_line)), Some(impl_line)) =
                    (&current_struct, first_impl_line)
                {
                    if seen_impl_after_struct {
                        if let Some(trait_node) = ast::Trait::cast(node.clone()) {
                            if let Some(trait_name) = trait_node.name() {
                                violations.push(Violation {
                                    struct_name: struct_name.clone(),
                                    struct_line: *struct_line,
                                    trait_name: trait_name.to_string(),
                                    trait_line: line_num,
                                    first_impl_line: impl_line,
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    violations
}

fn main() -> Result<()> {
    let start_time = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();

    let files = find_rust_files(&args.paths);

    let mut all_violations = Vec::new();

    for file_path in &files {
        let violations = check_file(file_path);
        for violation in violations {
            let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
            all_violations.push((rel_path.to_path_buf(), violation));
        }
    }

    if all_violations.is_empty() {
        log!("✓ All trait definitions are in correct order");
        let elapsed = start_time.elapsed();
        eprintln!("\nCompleted in {}ms", elapsed.as_millis());
        return Ok(());
    }

    log!("✗ Trait Definition Order Violations:\n");
    log!("Trait definitions should appear BEFORE impl blocks (after struct/enum).\n");

    for (file_path, v) in &all_violations {
        log!("  {}:{}", file_path.display(), v.struct_line);
        log!("    Struct: {}", v.struct_name);
        log!("    Line {}: First impl block", v.first_impl_line);
        log!("    Line {}: trait {} definition", v.trait_line, v.trait_name);
        log!("    → Trait {} should move before line {}", v.trait_name, v.first_impl_line);
        log!("");
    }

    log!("Total violations: {}", all_violations.len());
    log!("\nCorrect order:");
    log!("  1. Data structure (struct/enum)");
    log!("  2. Trait definition <- SHOULD BE HERE");
    log!("  3. Inherent impl (impl Type {{ ... }})");
    log!("  4. Custom trait implementations");
    log!("  5. Standard trait implementations");

    let elapsed = start_time.elapsed();
    eprintln!("\nCompleted in {}ms", elapsed.as_millis());

    Ok(())
}

