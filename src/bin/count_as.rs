// Copyright (C) Brian G. Milnes 2025

//! Count 'as' type cast expressions in Rust code
//! 
//! Replaces: scripts/analyze/count_as.sh
//! Uses AST parsing to find AS_EXPR nodes (type casts)
//! Binary: rusticate-count-as

use anyhow::Result;
use rusticate::{StandardArgs, format_number, parse_source, find_nodes, find_rust_files};
use ra_ap_syntax::{SyntaxKind, ast::AstNode};
use std::fs;
use std::path::Path;
use std::time::Instant;
use std::collections::BTreeMap;

fn count_as_in_file(file_path: &Path) -> Result<usize> {
    let content = fs::read_to_string(file_path)?;
    let source_file = parse_source(&content)?;
    let root = source_file.syntax();
    
    // Find all AS_EXPR (cast expressions) nodes
    let as_exprs = find_nodes(root, SyntaxKind::CAST_EXPR);
    
    Ok(as_exprs.len())
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let search_dirs = args.get_search_dirs();
    
    // Categorize search directories
    let mut src_dirs = Vec::new();
    let mut tests_dirs = Vec::new();
    let mut benches_dirs = Vec::new();
    let mut other_dirs = Vec::new();
    
    for path in search_dirs {
        if path.is_file() {
            other_dirs.push(path);
        } else if path.is_dir() {
            if path.ends_with("src") || path.components().any(|c| c.as_os_str() == "src") {
                src_dirs.push(path);
            } else if path.ends_with("tests") || path.components().any(|c| c.as_os_str() == "tests") {
                tests_dirs.push(path);
            } else if path.ends_with("benches") || path.components().any(|c| c.as_os_str() == "benches") {
                benches_dirs.push(path);
            } else {
                other_dirs.push(path);
            }
        }
    }
    
    let mut section_counts = BTreeMap::new();
    let mut section_totals: BTreeMap<&str, usize> = BTreeMap::new();
    
    // Process each category
    let categories = [
        ("src", &src_dirs),
        ("tests", &tests_dirs),
        ("benches", &benches_dirs),
    ];
    
    for (name, dirs) in &categories {
        if dirs.is_empty() {
            continue;
        }
        
        let files = find_rust_files(dirs);
        let mut file_counts = Vec::new();
        
        for file in files {
            match count_as_in_file(&file) {
                Ok(count) => {
                    if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                        file_counts.push((rel_path.display().to_string(), count));
                    } else {
                        file_counts.push((file.display().to_string(), count));
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to parse {}: {}", file.display(), e);
                }
            }
        }
        
        if !file_counts.is_empty() {
            let total: usize = file_counts.iter().map(|(_, c)| c).sum();
            section_totals.insert(name, total);
            section_counts.insert(*name, file_counts);
        }
    }
    
    // Process other files/dirs
    if !other_dirs.is_empty() {
        let files: Vec<_> = other_dirs.iter()
            .filter(|p| p.is_file())
            .cloned()
            .collect();
        let dirs: Vec<_> = other_dirs.iter()
            .filter(|p| p.is_dir())
            .cloned()
            .collect();
        
        let mut all_files = files;
        all_files.extend(find_rust_files(&dirs));
        
        for file in all_files {
            match count_as_in_file(&file) {
                Ok(count) => {
                    if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                        println!("{}: {} 'as' expressions", rel_path.display(), format_number(count));
                    } else {
                        println!("{}: {} 'as' expressions", file.display(), format_number(count));
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to parse {}: {}", file.display(), e);
                }
            }
        }
    }
    
    // Print detailed counts by section
    for (section, files) in &section_counts {
        println!("{}:", section);
        for (file, count) in files {
            println!("  {}: {} 'as' expressions", file, format_number(*count));
        }
        println!();
    }
    
    // Summary line - only show categories that were searched
    let mut summary_parts = Vec::new();
    if !src_dirs.is_empty() {
        let count = section_totals.get("src").copied().unwrap_or(0);
        summary_parts.push(format!("src {}", format_number(count)));
    }
    if !tests_dirs.is_empty() {
        let count = section_totals.get("tests").copied().unwrap_or(0);
        summary_parts.push(format!("tests {}", format_number(count)));
    }
    if !benches_dirs.is_empty() {
        let count = section_totals.get("benches").copied().unwrap_or(0);
        summary_parts.push(format!("benches {}", format_number(count)));
    }
    
    let total: usize = section_totals.values().sum();
    summary_parts.push(format!("total {}", format_number(total)));
    
    println!("Summary: {} 'as' expressions", summary_parts.join(", "));
    
    let elapsed = start.elapsed().as_millis();
    println!("Completed in {}ms", elapsed);
    
    Ok(())
}
