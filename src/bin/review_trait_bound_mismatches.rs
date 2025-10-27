// Copyright (C) Brian G. Milnes 2025

//! Review: Trait bound mismatches between inherent and trait impls
//! 
//! Replaces: scripts/rust/src/review_trait_bound_mismatches.py
//! Rule: Detects cases where inherent impl has weaker bounds than trait impl
//! Git commit: 584a672b6a34782766863c5f76a461d3297a741a
//! Binary: rusticate-review-trait-bound-mismatches
//!
//! This is a simplified version that flags files with both inherent and trait impls
//! for manual review of potential bound mismatches.

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
            .open("analyses/review_trait_bound_mismatches.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    message: String,
}

fn check_file(file_path: &Path, source: &str) -> Result<Vec<Violation>> {
    let source_file = parse_source(source)?;
    let root = source_file.syntax();
    
    // Find traits
    let trait_nodes = find_nodes(root, SyntaxKind::TRAIT);
    let impl_nodes = find_nodes(root, SyntaxKind::IMPL);
    
    if trait_nodes.is_empty() || impl_nodes.is_empty() {
        return Ok(Vec::new());
    }
    
    // Track which files have both inherent and trait impls
    let mut has_inherent = false;
    let mut has_trait_impl = false;
    
    for impl_node in &impl_nodes {
        if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
            if impl_ast.trait_().is_some() {
                has_trait_impl = true;
            } else {
                has_inherent = true;
            }
        }
    }
    
    // Simple heuristic: if file has both trait def, inherent impl, and trait impl,
    // flag for manual review
    if !trait_nodes.is_empty() && has_inherent && has_trait_impl {
        let line_num = rusticate::line_number(&trait_nodes[0], source);
        return Ok(vec![Violation {
            file: file_path.to_path_buf(),
            line_num,
            message: "File has trait + inherent impl + trait impl; check for bound mismatches".to_string(),
        }]);
    }
    
    Ok(Vec::new())
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
        log!("✓ No potential trait bound mismatches found");
    } else {
        log!("✗ Potential trait bound mismatches found:");
        log!("");
        log!("Files with trait + inherent impl + trait impl should be reviewed for bound consistency.");
        log!("");
        
        for v in &all_violations {
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                log!("{}:{}: {}", 
                         rel_path.display(), v.line_num, v.message);
            }
        }
    }
    
    // Summary
    log!("");
    log!("Summary: {} files checked, {} potential issues", 
             format_number(files.len()), format_number(all_violations.len()));
    
    let elapsed = start.elapsed().as_millis();
    log!("Completed in {}ms", elapsed);
    
    if all_violations.is_empty() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

