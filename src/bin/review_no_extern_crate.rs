// Copyright (C) Brian G. Milnes 2025

//! Review: No 'extern crate' usage
//! 
//! Replaces: scripts/rust/review_no_extern_crate.py
//! RustRules.md Line 86: "Never use extern crate. Do not add re-exports."
//! Binary: rusticate-review-no-extern-crate
//!
//! Uses AST parsing to find EXTERN_CRATE items

use anyhow::Result;
use rusticate::{StandardArgs, format_number, parse_source, find_nodes, line_number, find_rust_files};
use ra_ap_syntax::{SyntaxKind, ast::AstNode};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_no_extern_crate.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    line_content: String,
}

fn check_file(file_path: &Path) -> Result<Vec<Violation>> {
    let content = fs::read_to_string(file_path)?;
    let source_file = parse_source(&content)?;
    let root = source_file.syntax();
    
    // Find all EXTERN_CRATE items using AST
    let extern_crates = find_nodes(root, SyntaxKind::EXTERN_CRATE);
    
    let mut violations = Vec::new();
    
    for node in extern_crates {
        let line_num = line_number(&node, &content);
        let line_content = content.lines().nth(line_num - 1)
            .unwrap_or("")
            .trim()
            .to_string();
        
        violations.push(Violation {
            file: file_path.to_path_buf(),
            line_num,
            line_content,
        });
    }
    
    Ok(violations)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    // Print compilation directory for Emacs compile-mode
    log!("Entering directory '{}'", base_dir.display());
    log!("");
    
    let search_dirs = args.get_search_dirs();
    
    let files = find_rust_files(&search_dirs);
    let mut all_violations = Vec::new();
    
    for file in &files {
        match check_file(file) {
            Ok(violations) => all_violations.extend(violations),
            Err(e) => {
                // Skip files that fail to parse
                eprintln!("Warning: Failed to parse {}: {}", file.display(), e);
            }
        }
    }
    
    // Report violations
    if all_violations.is_empty() {
        log!("✓ No 'extern crate' usage found");
    } else {
        log!("✗ Found {} violation(s):", format_number(all_violations.len()));
        log!("");
        for v in &all_violations {
            // Use relative path from base_dir (Emacs will use compilation directory)
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                log!("{}:{}: extern crate usage", rel_path.display(), v.line_num);
                log!("  {}", v.line_content.trim());
            }
        }
    }
    
    // Summary line
    let unique_files: std::collections::HashSet<_> = all_violations.iter().map(|v| &v.file).collect();
    log!("");
    log!("Summary: {} files checked, {} files with violations, {} total violations",
             format_number(files.len()), format_number(unique_files.len()), format_number(all_violations.len()));
    
    let elapsed = start.elapsed().as_millis();
    log!("Completed in {}ms", elapsed);
    
    // Exit code: 0 if no violations, 1 if violations found
    if all_violations.is_empty() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
