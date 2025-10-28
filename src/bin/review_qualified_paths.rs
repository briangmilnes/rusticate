//! Review: Qualified Path Organization
//!
//! Finds fully-qualified paths in code bodies that should be imported.
//!
//! Violations: Using std::collections::hash_set::Iter or similar long paths
//! instead of importing at the top and using the short name.
//!
//! Examples:
//!   BAD:  fn iter(&self) -> std::collections::hash_set::Iter<'_, T>
//!   GOOD: use std::collections::hash_set::Iter;
//!         fn iter(&self) -> Iter<'_, T>
//!
//! Binary: rusticate-review-qualified-paths

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition};
use rusticate::{StandardArgs, find_rust_files};
use std::path::Path;
use std::time::Instant;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_qualified_paths.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn check_file(file_path: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();

    let mut violations = Vec::new();

    // Traverse all PATH nodes in the AST
    for node in root.descendants() {
        if node.kind() == SyntaxKind::PATH {
            if let Some(path) = ast::Path::cast(node.clone()) {
                let path_str = path.to_string();
                
                // Check if it's a qualified path we care about:
                // - Starts with std:: or core::
                // - Has at least 2 :: separators (e.g., std::fmt::Display)
                if !path_str.starts_with("std::") && !path_str.starts_with("core::") {
                    continue;
                }
                
                // Count :: separators
                let separator_count = path_str.matches("::").count();
                if separator_count < 2 {
                    continue;
                }
                
                // Skip std::fmt::Result (acceptable to keep qualified)
                if path_str == "std::fmt::Result" {
                    continue;
                }
                
                // Check if this is inside a USE statement (skip those)
                let mut parent = node.parent();
                let mut in_use = false;
                while let Some(p) = parent {
                    if p.kind() == SyntaxKind::USE {
                        in_use = true;
                        break;
                    }
                    parent = p.parent();
                }
                if in_use {
                    continue;
                }
                
                // Check if this is a function/method call (path followed by arguments)
                // Look at the parent node to see if it's a CALL_EXPR or PATH_EXPR followed by a call
                if let Some(p) = node.parent() {
                    let pk = p.kind();
                    // Skip if it's part of a call expression
                    if pk == SyntaxKind::CALL_EXPR {
                        continue;
                    }
                    // Skip if the parent is PATH_EXPR and grandparent is CALL_EXPR
                    if pk == SyntaxKind::PATH_EXPR {
                        if let Some(gp) = p.parent() {
                            if gp.kind() == SyntaxKind::CALL_EXPR {
                                continue;
                            }
                        }
                    }
                }
                
                // Skip if this is part of a turbofish ::<T>
                if path_str.contains("::<") {
                    continue;
                }
                
                // Get line number
                let line_num = content[..node.text_range().start().into()]
                    .chars()
                    .filter(|&c| c == '\n')
                    .count() + 1;
                
                // Get the line content for context
                let lines: Vec<&str> = content.lines().collect();
                let line_content = if line_num > 0 && line_num <= lines.len() {
                    lines[line_num - 1].trim()
                } else {
                    ""
                };
                
                violations.push(format!(
                    "  {}:{} - '{}' should be imported\n    {}",
                    file_path.display(),
                    line_num,
                    path_str,
                    line_content
                ));
            }
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
        for mut violation in violations {
            // Make paths relative
            if let Ok(rel_path) = file_path.strip_prefix(&base_dir) {
                violation = violation.replace(&file_path.display().to_string(), &rel_path.display().to_string());
            }
            all_violations.push(violation);
        }
    }

    if all_violations.is_empty() {
        log!("✓ Qualified Path Organization: No violations found (RustRules.md)");
        let elapsed = start_time.elapsed();
        eprintln!("\nCompleted in {}ms", elapsed.as_millis());
        return Ok(());
    }

    log!("✗ Qualified Path Organization violations found:\n");
    for violation in &all_violations {
        log!("{}", violation);
    }
    log!("\nTotal violations: {}", all_violations.len());
    log!("\nUse 'use' statements at the top to import types, then use short names.");

    let elapsed = start_time.elapsed();
    eprintln!("\nCompleted in {}ms", elapsed.as_millis());

    Ok(())
}

