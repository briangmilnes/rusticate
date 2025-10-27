// Copyright (C) Brian G. Milnes 2025

//! Fix: Duplicate Methods
//! 
//! Removes duplicate standalone pub fn when there's a trait + impl for the same method.
//! This is the "convenience function" anti-pattern where you have:
//!   1. trait Foo { fn bar(...); }
//!   2. impl Foo for T { fn bar(...) { ... } }
//!   3. pub fn bar(...) { ... }  <-- REMOVE THIS
//! 
//! Binary: rusticate-fix-duplicate-methods

use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use ra_ap_syntax::{ast::{self, AstNode, HasName, HasVisibility}, SyntaxKind, SourceFile, Edition};
use rusticate::{StandardArgs, find_nodes};
use rusticate::args::args::find_rust_files;
use rusticate::duplicate_methods::find_duplicate_methods;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/fix_duplicate_methods.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn find_module_block(root: &ra_ap_syntax::SyntaxNode) -> Option<ra_ap_syntax::SyntaxNode> {
    for node in root.descendants() {
        if node.kind() == SyntaxKind::MODULE {
            if let Some(module) = ast::Module::cast(node.clone()) {
                if module.visibility().is_some() {
                    return Some(node);
                }
            }
        }
    }
    None
}

fn fix_file(file_path: &PathBuf, dry_run: bool) -> Result<usize> {
    let issues = find_duplicate_methods(file_path)?;
    
    if issues.is_empty() {
        return Ok(0);
    }
    
    let source = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&source, Edition::Edition2021);
    
    if !parsed.errors().is_empty() {
        return Ok(0);
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let module_node = match find_module_block(root) {
        Some(n) => n,
        None => return Ok(0),
    };
    
    let mut removals = Vec::new();
    
    // For each duplicate issue, find the pub fn to remove
    for issue in &issues {
        let has_trait = issue.locations.iter().any(|l| l.location_type == "trait");
        let has_impl = issue.locations.iter().any(|l| l.location_type == "impl");
        let pub_fn_locs: Vec<_> = issue.locations.iter()
            .filter(|l| l.location_type == "pub fn")
            .collect();
        
        // If we have trait + impl + pub fn, remove the pub fn
        if has_trait && has_impl && !pub_fn_locs.is_empty() {
            // Find the actual pub fn node to remove
            let fn_nodes = find_nodes(&module_node, SyntaxKind::FN);
            for fn_node in fn_nodes {
                // Check if this is a module-level pub fn
                let mut is_module_level = true;
                let mut parent = fn_node.parent();
                while let Some(p) = parent {
                    if p.kind() == SyntaxKind::IMPL || p.kind() == SyntaxKind::TRAIT {
                        is_module_level = false;
                        break;
                    }
                    if p == module_node {
                        break;
                    }
                    parent = p.parent();
                }
                
                if is_module_level {
                    if let Some(fn_ast) = ast::Fn::cast(fn_node.clone()) {
                        if fn_ast.visibility().is_some() {
                            if let Some(name_node) = fn_ast.name() {
                                if name_node.text() == issue.name {
                                    removals.push(fn_node.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    if removals.is_empty() {
        return Ok(0);
    }
    
    // Build new source by removing the pub fns
    let mut new_source = source.clone();
    
    // Sort removals by position (reverse order to avoid offset issues)
    let mut removal_ranges: Vec<_> = removals.iter().map(|node| {
        let start: usize = node.text_range().start().into();
        let end: usize = node.text_range().end().into();
        
        // Include doc comments before the function
        let mut actual_start = start;
        
        // Find the start of the line
        let line_start = new_source[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
        
        // Count lines before the function that are doc comments or empty
        let mut check_pos = line_start;
        while check_pos > 0 {
            let prev_line_end = new_source[..check_pos.saturating_sub(1)].rfind('\n').unwrap_or(0);
            let prev_line = &new_source[prev_line_end..check_pos].trim();
            
            if prev_line.starts_with("///") || prev_line.starts_with("//!") || 
               prev_line.starts_with("/**") || prev_line.is_empty() {
                actual_start = prev_line_end;
                check_pos = prev_line_end;
            } else {
                break;
            }
        }
        
        // Include trailing whitespace/newlines after the function
        let mut actual_end = end;
        while actual_end < new_source.len() && 
              (new_source.as_bytes()[actual_end] == b'\n' || 
               new_source.as_bytes()[actual_end] == b'\r') {
            actual_end += 1;
        }
        
        (actual_start, actual_end)
    }).collect();
    
    removal_ranges.sort_by(|a, b| b.0.cmp(&a.0));  // Reverse order
    
    for (start, end) in removal_ranges {
        new_source.replace_range(start..end, "");
    }
    
    let fixed_count = removals.len();
    
    if !dry_run {
        fs::write(file_path, new_source)?;
        log!("Fixed {} duplicate pub fn(s) in {}", fixed_count, file_path.display());
    } else {
        log!("[DRY RUN] Would fix {} duplicate pub fn(s) in {}", fixed_count, file_path.display());
    }
    
    Ok(fixed_count)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    
    // Check for --dry-run flag
    let dry_run = std::env::args().any(|arg| arg == "--dry-run");
    
    // Get directories to search
    let search_dirs = args.get_search_dirs();
    let files = find_rust_files(&search_dirs);

    log!("{}", "=".repeat(80));
    log!("FIX DUPLICATE METHODS");
    if dry_run {
        log!("(DRY RUN MODE)");
    }
    log!("{}", "=".repeat(80));
    log!("");

    let mut total_fixed = 0;
    let mut files_modified = 0;

    for file_path in &files {
        if let Ok(count) = fix_file(file_path, dry_run) {
            if count > 0 {
                total_fixed += count;
                files_modified += 1;
            }
        }
    }

    log!("");
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Files modified: {}", files_modified);
    log!("  Total fixes: {}", total_fixed);
    log!("");
    log!("Completed in {}ms", start.elapsed().as_millis());

    Ok(())
}
