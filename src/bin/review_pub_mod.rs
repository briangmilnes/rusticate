// Copyright (C) Brian G. Milnes 2025

//! Review: Check for pub mod declarations
//! 
//! Checks that each .rs file (except binaries, lib.rs, main.rs) has a 
//! 'pub mod X {}' declaration at the module level.
//! This is a general Rust module organization pattern.
//! 
//! Binary: rusticate-review-pub-mod
//!
//! Uses AST parsing to find MODULE nodes with pub visibility

use anyhow::Result;
use rusticate::{StandardArgs, format_number, parse_source, find_rust_files, line_number};
use ra_ap_syntax::{SyntaxKind, ast::{self, AstNode, HasVisibility, HasName}, Edition};
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
            .open("analyses/review_pub_mod.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
}

fn has_pub_mod(content: &str) -> Option<usize> {
    // Parse source and find MODULE node with pub visibility
    let parsed = ra_ap_syntax::SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    for node in root.children() {
        if node.kind() == SyntaxKind::MODULE {
            if let Some(module) = ast::Module::cast(node.clone()) {
                if let Some(vis) = module.visibility() {
                    if vis.to_string() == "pub" {
                        return Some(line_number(&node, content));
                    }
                }
            }
        }
    }
    None
}

fn should_skip_file(file_path: &Path) -> bool {
    let file_name = file_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    
    // Skip lib.rs, main.rs, mod.rs (entry points/module declarations)
    if file_name == "lib.rs" || file_name == "main.rs" || file_name == "mod.rs" {
        return true;
    }
    
    // Skip test files (test_*.rs or in tests/)
    if file_name.starts_with("test_") {
        return true;
    }
    
    // Check if file is in tests/, benches/, or src/bin/ directories
    if let Some(parent) = file_path.parent() {
        if let Some(parent_name) = parent.file_name().and_then(|n| n.to_str()) {
            if parent_name == "bin" || parent_name == "tests" || parent_name == "benches" {
                return true;
            }
        }
    }
    
    false
}

fn check_file(file_path: &Path) -> Result<Option<Violation>> {
    if should_skip_file(file_path) {
        return Ok(None);
    }
    
    let content = fs::read_to_string(file_path)?;
    let _source_file = parse_source(&content)?; // Validate parsing
    
    if has_pub_mod(&content).is_none() {
        Ok(Some(Violation {
            file: file_path.to_path_buf(),
            line_num: 1, // Report at line 1 since there's no pub mod
        }))
    } else {
        Ok(None)
    }
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
            Ok(Some(violation)) => all_violations.push(violation),
            Ok(None) => {}, // No violation or skipped
            Err(e) => {
                // Skip files that fail to parse
                eprintln!("Warning: Failed to parse {}: {}", file.display(), e);
            }
        }
    }
    
    // Report violations
    if all_violations.is_empty() {
        log!("✓ All non-binary modules have pub mod declarations");
    } else {
        log!("✗ Found {} file(s) missing pub mod declarations:", format_number(all_violations.len()));
        log!("");
        for v in &all_violations {
            // Use relative path from base_dir (Emacs will use compilation directory)
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                log!("{}:{}: missing pub mod declaration", rel_path.display(), v.line_num);
            } else {
                log!("{}:{}: missing pub mod declaration", v.file.display(), v.line_num);
            }
        }
        log!("");
        log!("Summary: {} files checked, {} missing pub mod", 
            format_number(files.len()), 
            format_number(all_violations.len()));
        std::process::exit(1);
    }
    
    log!("Completed in {}ms", start.elapsed().as_millis());
    Ok(())
}

