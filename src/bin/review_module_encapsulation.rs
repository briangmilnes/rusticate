// Copyright (C) Brian G. Milnes 2025

//! Review: Mandatory module encapsulation
//! 
//! Replaces: scripts/rust/src/review_module_encapsulation.py
//! RustRules.md Lines 117-123: ALL CODE MUST BE WITHIN pub mod M{...}
//! Binary: rusticate-review-module-encapsulation
//!
//! Uses AST parsing to check if items are inside MODULE nodes

use anyhow::Result;
use rusticate::{StandardArgs, format_number, parse_source, find_nodes_where, find_rust_files};
use ra_ap_syntax::{SyntaxKind, SyntaxNode, ast::AstNode};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    keyword: String,
    line_content: String,
}

/// Check if a node is inside a MODULE
fn is_inside_module(node: &SyntaxNode) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::MODULE {
            return true;
        }
        current = parent.parent();
    }
    false
}

/// Check if an item should be inside a module (not lib.rs/main.rs exceptions)
fn check_file(file_path: &Path) -> Result<Vec<Violation>> {
    let filename = file_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    
    // Skip lib.rs, main.rs, and files in src/bin/ (binary entry points)
    if matches!(filename, "lib.rs" | "main.rs") {
        return Ok(Vec::new());
    }
    
    // Skip files in src/bin/ directory (binary entry points)
    if file_path.components().any(|c| c.as_os_str() == "bin") {
        return Ok(Vec::new());
    }
    
    let content = fs::read_to_string(file_path)?;
    let source_file = parse_source(&content)?;
    let root = source_file.syntax();
    
    let mut violations = Vec::new();
    
    // Find all item nodes that should be inside modules
    let item_kinds = [
        SyntaxKind::FN,
        SyntaxKind::STRUCT,
        SyntaxKind::ENUM,
        SyntaxKind::TYPE_ALIAS,
        SyntaxKind::TRAIT,
        SyntaxKind::IMPL,
        SyntaxKind::CONST,
        SyntaxKind::STATIC,
    ];
    
    for kind in &item_kinds {
        let items = find_nodes_where(root, |node| {
            node.kind() == *kind && !is_inside_module(node)
        });
        
        for item_node in items {
            // Skip macro_rules! which are allowed at file level
            let text = item_node.text().to_string();
            if text.contains("macro_rules!") {
                continue;
            }
            
            let line_num = rusticate::line_number(&item_node, &content);
            let line_content: String = content
                .lines()
                .nth(line_num - 1)
                .unwrap_or("")
                .trim()
                .chars()
                .take(80)
                .collect();
            
            let keyword = match kind {
                SyntaxKind::FN => "fn",
                SyntaxKind::STRUCT => "struct",
                SyntaxKind::ENUM => "enum",
                SyntaxKind::TYPE_ALIAS => "type",
                SyntaxKind::TRAIT => "trait",
                SyntaxKind::IMPL => "impl",
                SyntaxKind::CONST => "const",
                SyntaxKind::STATIC => "static",
                _ => "item",
            };
            
            violations.push(Violation {
                file: file_path.to_path_buf(),
                line_num,
                keyword: keyword.to_string(),
                line_content,
            });
        }
    }
    
    Ok(violations)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    // Print compilation directory for Emacs compile-mode
    println!("Entering directory '{}'", base_dir.display());
    println!();
    
    let search_dirs = args.get_search_dirs();
    
    // Module encapsulation rule only applies to src/ (not tests/ or benches/)
    let src_dirs: Vec<_> = search_dirs.iter()
        .filter(|p| p.is_dir() && (p.ends_with("src") || p.components().any(|c| c.as_os_str() == "src")))
        .cloned()
        .collect();
    
    if src_dirs.is_empty() {
        println!("✓ No src/ directories to check");
        let elapsed = start.elapsed().as_millis();
        println!("Completed in {}ms", elapsed);
        return Ok(());
    }
    
    let files = find_rust_files(&src_dirs);
    let mut all_violations = Vec::new();
    
    for file in &files {
        match check_file(file) {
            Ok(violations) => all_violations.extend(violations),
            Err(e) => {
                eprintln!("Warning: Failed to parse {}: {}", file.display(), e);
            }
        }
    }
    
    // Report violations
    if all_violations.is_empty() {
        println!("✓ All code is properly encapsulated in pub mod blocks");
    } else {
        println!("✗ Found {} violation(s):", format_number(all_violations.len()));
        println!();
        for v in &all_violations {
            // Use relative path from base_dir (Emacs will use compilation directory)
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                println!("{}:{}: {} outside pub mod", rel_path.display(), v.line_num, v.keyword);
                println!("  {}", v.line_content.trim());
            }
        }
    }
    
    // Summary line
    let unique_files: std::collections::HashSet<_> = all_violations.iter().map(|v| &v.file).collect();
    println!();
    println!("Summary: {} files checked, {} files with violations, {} total violations",
             format_number(files.len()), format_number(unique_files.len()), format_number(all_violations.len()));
    
    let elapsed = start.elapsed().as_millis();
    println!("Completed in {}ms", elapsed);
    
    // Exit code: 0 if no violations, 1 if violations found
    if all_violations.is_empty() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
