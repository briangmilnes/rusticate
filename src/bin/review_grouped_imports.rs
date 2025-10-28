//! Review: Grouped Imports
//!
//! Finds grouped imports like:
//!   use crate::{path1::*, path2::*, path3};
//!
//! And reports them so they can be converted to single-line imports:
//!   use crate::path1::*;
//!   use crate::path2::*;
//!   use crate::path3;
//!
//! Binary: review-grouped-imports

use anyhow::Result;
use ra_ap_syntax::{ast::AstNode, SyntaxKind, SourceFile, Edition, SyntaxNode};
use rusticate::{find_rust_files, StandardArgs};
use std::fs;
use std::path::Path;
use std::time::Instant;

macro_rules! log {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        println!("{}", msg);
    }};
}

#[derive(Debug)]
struct GroupedImport {
    line: usize,
    full_text: String,
}

fn get_line_number(node: &SyntaxNode, content: &str) -> usize {
    let offset: usize = node.text_range().start().into();
    content[..offset].lines().count()
}

fn has_use_tree_list(use_node: &SyntaxNode) -> bool {
    // Check if this use statement has a USE_TREE_LIST (grouped imports)
    for descendant in use_node.descendants() {
        if descendant.kind() == SyntaxKind::USE_TREE_LIST {
            return true;
        }
    }
    false
}

fn find_grouped_imports(root: &SyntaxNode, content: &str) -> Vec<GroupedImport> {
    let mut grouped = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE
            && has_use_tree_list(&node) {
                grouped.push(GroupedImport {
                    line: get_line_number(&node, content),
                    full_text: node.to_string(),
                });
            }
    }
    
    grouped
}

fn analyze_file(file_path: &Path) -> Result<Vec<GroupedImport>> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    Ok(find_grouped_imports(root, &content))
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/review_grouped_imports.log").ok();
    
    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let all_files = find_rust_files(&args.paths);
    
    log!("Entering directory '{}'", base_dir.display());
    println!();
    
    let mut total_grouped = 0;
    let mut files_with_grouped = Vec::new();
    
    for file_path in &all_files {
        if let Ok(grouped) = analyze_file(file_path) {
            if !grouped.is_empty() {
                let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
                files_with_grouped.push((rel_path.to_path_buf(), grouped));
            }
        }
    }
    
    // Sort by file path
    files_with_grouped.sort_by(|a, b| a.0.cmp(&b.0));
    
    // Print results
    println!();
    log!("{}", "=".repeat(80));
    log!("GROUPED IMPORTS:");
    log!("{}", "=".repeat(80));
    println!();
    
    if files_with_grouped.is_empty() {
        log!("None found");
        println!();
    } else {
        for (file, grouped) in &files_with_grouped {
            log!("{}:1:", file.display());
            log!("  {} grouped imports", grouped.len());
            
            for import in grouped {
                log!("    Line {}: {}", import.line, import.full_text);
            }
            
            println!();
            total_grouped += grouped.len();
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Total grouped imports: {}", total_grouped);
    log!("  Files with grouped imports: {}", files_with_grouped.len());
    log!("{}", "=".repeat(80));
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

