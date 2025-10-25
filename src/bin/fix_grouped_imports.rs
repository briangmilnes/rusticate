//! Fix: Grouped Imports
//!
//! Converts grouped imports to single-line imports:
//!
//! Before:
//!   use crate::{path1::*, path2::*, path3};
//!
//! After:
//!   use crate::path1::*;
//!   use crate::path2::*;
//!   use crate::path3;
//!
//! Uses AST parsing - NO STRING HACKING.
//!
//! Binary: fix-grouped-imports

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition, SyntaxNode, TextRange};
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

fn extract_use_trees(use_tree_list: &ast::UseTreeList) -> Vec<String> {
    let mut trees = Vec::new();
    
    for tree in use_tree_list.use_trees() {
        trees.push(tree.to_string());
    }
    
    trees
}

fn expand_grouped_import(use_stmt: &ast::Use) -> Option<(TextRange, Vec<String>)> {
    // Check if this has a USE_TREE_LIST (grouped import)
    let mut has_list = false;
    for descendant in use_stmt.syntax().descendants() {
        if descendant.kind() == SyntaxKind::USE_TREE_LIST {
            has_list = true;
            break;
        }
    }
    
    if !has_list {
        return None;
    }
    
    // Extract the base path (everything before the {})
    let use_text = use_stmt.syntax().to_string();
    
    // Find the base path
    let base_path = if let Some(pos) = use_text.find('{') {
        use_text[..pos].trim_start_matches("use ").trim()
    } else {
        return None;
    };
    
    // Extract individual imports from the list
    let mut individual_imports = Vec::new();
    
    for descendant in use_stmt.syntax().descendants() {
        if descendant.kind() == SyntaxKind::USE_TREE_LIST {
            if let Some(list) = ast::UseTreeList::cast(descendant) {
                for tree in extract_use_trees(&list) {
                    let tree = tree.trim();
                    // Build the full import statement
                    let full_import = if base_path.is_empty() {
                        format!("use {};", tree)
                    } else if base_path.ends_with("::") {
                        format!("use {}{};", base_path, tree)
                    } else {
                        format!("use {}::{};", base_path, tree)
                    };
                    individual_imports.push(full_import);
                }
            }
            break;
        }
    }
    
    if individual_imports.is_empty() {
        return None;
    }
    
    Some((use_stmt.syntax().text_range(), individual_imports))
}

fn fix_file(file_path: &Path, dry_run: bool) -> Result<usize> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut replacements: Vec<(TextRange, String)> = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_stmt) = ast::Use::cast(node) {
                if let Some((range, expanded)) = expand_grouped_import(&use_stmt) {
                    // Join the expanded imports with newlines
                    let replacement = expanded.join("\n");
                    replacements.push((range, replacement));
                }
            }
        }
    }
    
    if replacements.is_empty() {
        return Ok(0);
    }
    
    // Sort in reverse order to apply from end to start
    replacements.sort_by(|a, b| b.0.start().cmp(&a.0.start()));
    
    let mut result = content.clone();
    for (range, replacement) in &replacements {
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        result.replace_range(start..end, replacement);
    }
    
    if !dry_run {
        fs::write(file_path, result)?;
    }
    
    Ok(replacements.len())
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/fix_grouped_imports.log").ok();
    
    let start_time = Instant::now();
    
    // Check for --dry-run flag
    let dry_run = std::env::args().any(|arg| arg == "--dry-run" || arg == "-n");
    
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let all_files = find_rust_files(&args.paths);
    
    if dry_run {
        log!("DRY RUN MODE - No files will be modified");
    }
    log!("Entering directory '{}'", base_dir.display());
    println!();
    
    let mut total_fixed = 0;
    let mut files_fixed = Vec::new();
    
    for file_path in &all_files {
        match fix_file(file_path, dry_run) {
            Ok(count) if count > 0 => {
                let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
                if dry_run {
                    log!("{}:1: Would expand {} grouped imports", rel_path.display(), count);
                } else {
                    log!("{}:1: Expanded {} grouped imports", rel_path.display(), count);
                }
                files_fixed.push(rel_path.display().to_string());
                total_fixed += count;
            }
            Err(e) => {
                eprintln!("Error processing {}: {}", file_path.display(), e);
            }
            _ => {}
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    if dry_run {
        log!("  Grouped imports that would be expanded: {}", total_fixed);
        log!("  Files that would be modified: {}", files_fixed.len());
    } else {
        log!("  Grouped imports expanded: {}", total_fixed);
        log!("  Files modified: {}", files_fixed.len());
    }
    
    if !files_fixed.is_empty() {
        println!();
        if dry_run {
            log!("Files that would be modified:");
        } else {
            log!("Files modified:");
        }
        for file in &files_fixed {
            log!("  {}", file);
        }
    }
    
    log!("{}", "=".repeat(80));
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

