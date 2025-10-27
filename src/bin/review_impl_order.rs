// Copyright (C) Brian G. Milnes 2025

//! Review: Implementation order - standard traits should be at the bottom
//! 
//! Replaces: scripts/rust/src/review_impl_order.py
//! Rule: Standard trait impls (Eq, Debug, Display, etc.) must come AFTER custom trait impls
//! Git commit: 584a672b6a34782766863c5f76a461d3297a741a
//! Binary: rusticate-review-impl-order
//!
//! Correct order:
//! 1. Data structure (struct/enum)
//! 2. Trait definition
//! 3. Inherent impl (impl Type { ... })
//! 4. Custom trait implementations
//! 5. Standard trait implementations <- AT THE BOTTOM

use anyhow::Result;
use rusticate::{StandardArgs, format_number, parse_source, find_nodes, find_rust_files};
use ra_ap_syntax::{SyntaxKind, ast::{self, AstNode}};
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
            .open("analyses/review_impl_order.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn is_standard_trait(name: &str) -> bool {
    matches!(name,
        "Eq" | "PartialEq" | "Ord" | "PartialOrd" |
        "Debug" | "Display" |
        "Clone" | "Copy" |
        "Hash" |
        "Default" |
        "From" | "Into" | "TryFrom" | "TryInto" |
        "AsRef" | "AsMut" |
        "Deref" | "DerefMut" |
        "Drop" |
        "Iterator" | "IntoIterator" |
        "Index" | "IndexMut" |
        "Add" | "Sub" | "Mul" | "Div" | "Rem" | "Neg" |
        "BitAnd" | "BitOr" | "BitXor" | "Shl" | "Shr" |
        "Not" |
        "Send" | "Sync" |
        "Fn" | "FnMut" | "FnOnce" |
        "Error"
    )
}

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    standard_line: usize,
    standard_trait: String,
    custom_line: usize,
    custom_trait: String,
}

fn check_file(file_path: &Path, source: &str) -> Result<Vec<Violation>> {
    let source_file = parse_source(source)?;
    let root = source_file.syntax();
    
    // Find all IMPL nodes
    let impl_nodes = find_nodes(root, SyntaxKind::IMPL);
    
    // Track impl order
    let mut impls = Vec::new();
    
    for impl_node in impl_nodes {
        if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
            // Check if this is a trait impl (has trait_() method)
            if let Some(trait_type) = impl_ast.trait_() {
                // Get trait name
                let trait_text = trait_type.syntax().text().to_string();
                let trait_name = trait_text
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .find(|s| !s.is_empty())
                    .unwrap_or(&trait_text)
                    .to_string();
                
                let line_num = rusticate::line_number(impl_ast.syntax(), source);
                let is_standard = is_standard_trait(&trait_name);
                
                impls.push((line_num, trait_name, is_standard));
            }
        }
    }
    
    // Check for violations: standard trait impl before custom trait impl
    let mut violations = Vec::new();
    let mut first_standard: Option<(usize, String)> = None;
    
    for (line_num, trait_name, is_standard) in impls {
        if is_standard {
            if first_standard.is_none() {
                first_standard = Some((line_num, trait_name));
            }
        } else {
            // Custom trait impl
            if let Some((std_line, ref std_trait)) = first_standard {
                // Violation: standard impl came before this custom impl
                violations.push(Violation {
                    file: file_path.to_path_buf(),
                    standard_line: std_line,
                    standard_trait: std_trait.clone(),
                    custom_line: line_num,
                    custom_trait: trait_name,
                });
            }
        }
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
    
    // This rule only applies to src/ (not tests/ or benches/)
    let src_dirs: Vec<_> = search_dirs.iter()
        .filter(|p| p.is_dir() && (p.ends_with("src") || p.components().any(|c| c.as_os_str() == "src")))
        .cloned()
        .collect();
    
    if src_dirs.is_empty() {
        log!("✓ No src/ directories to check");
        let elapsed = start.elapsed().as_millis();
        log!("Completed in {}ms", elapsed);
        return Ok(());
    }
    
    let files = find_rust_files(&src_dirs);
    let mut all_violations = Vec::new();
    
    for file in &files {
        match fs::read_to_string(file) {
            Ok(source) => {
                match check_file(file, &source) {
                    Ok(violations) => all_violations.extend(violations),
                    Err(e) => {
                        eprintln!("Warning: Failed to parse {}: {}", file.display(), e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to read {}: {}", file.display(), e);
            }
        }
    }
    
    // Report violations
    if all_violations.is_empty() {
        log!("✓ All implementations are in correct order");
    } else {
        log!("✗ Implementation Order Violations:");
        log!("");
        log!("Standard trait impls (Eq, Debug, Display, etc.) should be AT THE BOTTOM (after custom trait impls).");
        log!("");
        
        for v in &all_violations {
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                log!("{}:{}: Standard trait '{}' before custom trait '{}'", 
                         rel_path.display(), v.standard_line, v.standard_trait, v.custom_trait);
                log!("  Line {}: {} impl (standard trait)", v.standard_line, v.standard_trait);
                log!("  Line {}: {} impl (custom trait)", v.custom_line, v.custom_trait);
                log!("  → Standard trait impls should move to the bottom");
            }
        }
        
        log!("");
        log!("Correct order:");
        log!("  1. Data structure (struct/enum)");
        log!("  2. Trait definition");
        log!("  3. Inherent impl (impl Type {{ ... }})");
        log!("  4. Custom trait implementations");
        log!("  5. Standard trait implementations <- AT THE BOTTOM");
    }
    
    // Summary
    log!("");
    log!("Summary: {} files checked, {} violations", 
             format_number(files.len()), format_number(all_violations.len()));
    
    let elapsed = start.elapsed().as_millis();
    log!("Completed in {}ms", elapsed);
    
    if all_violations.is_empty() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

