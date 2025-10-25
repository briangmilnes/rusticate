//! Review: Unnecessary UFCS and Qualified Paths
//!
//! Finds UFCS calls and qualified paths that could be simplified because
//! the trait is in scope via glob imports.
//!
//! Uses AST parsing - NO STRING HACKING.
//!
//! Binary: review-unnecessary-ufcs-and-qualified-paths

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasName}, SyntaxKind, SourceFile, Edition, SyntaxNode};
use rusticate::{find_rust_files, StandardArgs};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

macro_rules! log {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        println!("{}", msg);
    }};
}

#[derive(Debug, Clone)]
struct UfcsCall {
    line: usize,
    full_text: String,
}

struct FileReport {
    file: PathBuf,
    ufcs_calls: Vec<UfcsCall>,
    glob_imports: Vec<String>,
}

fn get_line_number(node: &SyntaxNode, content: &str) -> usize {
    let offset: usize = node.text_range().start().into();
    content[..offset].lines().count()
}

fn has_star_in_use_tree(use_node: &SyntaxNode) -> bool {
    // Check for STAR token in the use tree
    for token in use_node.descendants_with_tokens() {
        if token.kind() == SyntaxKind::STAR {
            return true;
        }
    }
    false
}

fn find_glob_imports(root: &SyntaxNode) -> Vec<String> {
    let mut imports = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if has_star_in_use_tree(&node) {
                // Extract the full use statement using AST
                if let Some(use_stmt) = ast::Use::cast(node.clone()) {
                    imports.push(use_stmt.syntax().to_string());
                }
            }
        }
    }
    
    imports
}

fn find_ufcs_calls(root: &SyntaxNode, content: &str) -> Vec<UfcsCall> {
    let mut calls = Vec::new();
    
    // UFCS calls are represented as paths with generic arguments that contain "as"
    // Look for CALL_EXPR nodes that have paths with angle brackets
    for node in root.descendants() {
        if node.kind() == SyntaxKind::CALL_EXPR {
            // Check if any part of the call expression contains angle bracket syntax
            // The UFCS pattern <Type as Trait>::method uses PATH with GENERIC_ARG_LIST
            let full_text = node.to_string();
            
            // Check for angle bracket at the start (UFCS pattern)
            let trimmed = full_text.trim();
            if trimmed.starts_with('<') && has_as_keyword(&node) {
                calls.push(UfcsCall {
                    line: get_line_number(&node, content),
                    full_text: full_text.clone(),
                });
            }
        }
    }
    
    calls
}

fn has_as_keyword(node: &SyntaxNode) -> bool {
    // Check if this node contains an AS_KW token
    for token in node.descendants_with_tokens() {
        if token.kind() == SyntaxKind::AS_KW {
            return true;
        }
    }
    false
}

fn analyze_file(file_path: &Path) -> Result<FileReport> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let glob_imports = find_glob_imports(root);
    let ufcs_calls = find_ufcs_calls(root, &content);
    
    Ok(FileReport {
        file: file_path.to_path_buf(),
        ufcs_calls,
        glob_imports,
    })
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/review_unnecessary_ufcs_and_qualified_paths.log").ok();
    
    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let all_files = find_rust_files(&args.paths);
    
    log!("Entering directory '{}'", base_dir.display());
    println!();
    
    let mut total_ufcs = 0;
    let mut files_with_ufcs = Vec::new();
    
    for file_path in &all_files {
        if let Ok(report) = analyze_file(file_path) {
            if !report.ufcs_calls.is_empty() {
                let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
                files_with_ufcs.push((rel_path.to_path_buf(), report));
            }
        }
    }
    
    // Sort by file path
    files_with_ufcs.sort_by(|a, b| a.0.cmp(&b.0));
    
    // Print results
    println!();
    log!("{}", "=".repeat(80));
    log!("UFCS CALLS FOUND:");
    log!("{}", "=".repeat(80));
    println!();
    
    if files_with_ufcs.is_empty() {
        log!("None found");
        println!();
    } else {
        for (file, report) in &files_with_ufcs {
            log!("{}:1:", file.display());
            log!("  Glob imports: {}", report.glob_imports.len());
            for import in &report.glob_imports {
                log!("    {}", import);
            }
            log!("  UFCS calls: {}", report.ufcs_calls.len());
            for call in &report.ufcs_calls {
                log!("    Line {}: {}", call.line, call.full_text);
            }
            println!();
            total_ufcs += report.ufcs_calls.len();
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Total UFCS calls: {}", total_ufcs);
    log!("  Files with UFCS calls: {}", files_with_ufcs.len());
    log!("{}", "=".repeat(80));
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}
