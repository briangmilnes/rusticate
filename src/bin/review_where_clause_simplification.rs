// Copyright (C) Brian G. Milnes 2025

//! Review: Where clause simplification
//! 
//! Replaces: scripts/rust/src/review_where_clause_simplification.py
//! Rule: RustRules.md Lines 322-329
//! "Replace fn method<F>(...) where F: Fn(...); with fn method<F: Fn(...)>(...);
//! for simple bounds. Minimize where clauses across codebase by inlining bounds."
//! Git commit: 584a672b6a34782766863c5f76a461d3297a741a
//! Binary: rusticate-review-where-clause-simplification

use anyhow::Result;
use rusticate::{StandardArgs, format_number, parse_source, find_nodes, find_rust_files};
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
            .open("analyses/review_where_clause_simplification.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    fn_name: String,
    where_text: String,
}

fn is_simple_bound(bound_text: &str) -> bool {
    let trimmed = bound_text.trim();
    
    // Simple: single trait name, no + operator
    if trimmed.contains('+') {
        return false;
    }
    
    // Fn traits with complex signatures (multiple commas) might be too complex
    if trimmed.contains("Fn") && trimmed.contains('(') {
        let comma_count = trimmed.matches(',').count();
        if comma_count > 1 {
            return false;
        }
    }
    
    true
}

fn check_file(file_path: &Path, source: &str) -> Result<Vec<Violation>> {
    let source_file = parse_source(source)?;
    let root = source_file.syntax();
    
    // Find all WHERE_CLAUSE nodes
    let where_clauses = find_nodes(root, SyntaxKind::WHERE_CLAUSE);
    let mut violations = Vec::new();
    
    for where_node in where_clauses {
        // Get the where clause text
        let where_text = where_node.text().to_string();
        
        // Find parent function
        let fn_parent = where_node.ancestors()
            .find(|n| n.kind() == SyntaxKind::FN);
        
        if let Some(fn_node) = fn_parent {
            // Get function name from children
            let fn_name = fn_node.children_with_tokens()
                .filter_map(|child| child.into_token())
                .find(|token| token.kind() == SyntaxKind::IDENT)
                .map(|token| token.text().to_string())
                .unwrap_or_else(|| "<anonymous>".to_string());
            
            // Simple heuristic: check if the where clause looks simplifiable
            // We look for patterns like "where T: SingleTrait" without +
            let where_content = where_text
                .strip_prefix("where")
                .unwrap_or(&where_text)
                .trim();
            
            // Split by comma to get individual bounds
            let bounds: Vec<&str> = where_content
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            
            // Check if all bounds are simple
            let all_simple = bounds.iter().all(|b| is_simple_bound(b));
            
            // Report if bounds are simple (could be inlined)
            if all_simple && !bounds.is_empty() {
                let line_num = rusticate::line_number(&fn_node, source);
                violations.push(Violation {
                    file: file_path.to_path_buf(),
                    line_num,
                    fn_name,
                    where_text: where_text.replace('\n', " "),
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
        log!("✓ No simple where clauses found that should be inlined");
    } else {
        log!("✗ Found simplifiable where clauses (RustRules.md Lines 322-329):");
        log!("");
        
        for v in &all_violations {
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                log!("{}:{}: fn {} has simplifiable where clause", 
                         rel_path.display(), v.line_num, v.fn_name);
                log!("  {}", v.where_text);
                log!("  → Could be inlined into generic parameters");
            }
        }
        
        log!("");
        log!("Suggestion: Inline simple single-bound where clauses into generic parameters.");
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

