// Copyright (C) Brian G. Milnes 2025

//! Count for loops in Rust code
//!
//! Counts:
//! - Total for loops
//! - Range-based loops (for x in 0..n, 0..collection.len(), etc.)
//! - Iterator-based loops (for x in collection)
//!
//! Usage:
//!   rusticate-count-for-loops -d src/
//!
//! Binary: rusticate-count-for-loops

use anyhow::Result;
use ra_ap_syntax::{ast, AstNode, SyntaxKind};
use rusticate::{StandardArgs, find_rust_files};
use std::path::Path;

#[derive(Default, Debug)]
struct FileStats {
    total_loops: usize,
    range_loops: usize,
    iterator_loops: usize,
}

fn analyze_file(path: &Path) -> Result<FileStats> {
    let content = std::fs::read_to_string(path)?;
    let parsed = ra_ap_syntax::SourceFile::parse(&content, ra_ap_syntax::Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut stats = FileStats::default();
    
    // Find all for loops
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FOR_EXPR {
            if let Some(for_expr) = ast::ForExpr::cast(node) {
                stats.total_loops += 1;
                
                // Check if it's a range-based or iterator-based loop
                if let Some(iterable) = for_expr.iterable() {
                    let iterable_text = iterable.to_string();
                    
                    // Check for range patterns: 0..x, 0..collection.len(), start..end
                    if is_range_loop(&iterable_text) {
                        stats.range_loops += 1;
                    } else {
                        stats.iterator_loops += 1;
                    }
                }
            }
        }
    }
    
    Ok(stats)
}

fn is_range_loop(iterable_text: &str) -> bool {
    // Check for range operators
    iterable_text.contains("..") || iterable_text.contains("..=")
}

fn main() -> Result<()> {
    let args = StandardArgs::parse()?;
    let paths = args.get_search_dirs();
    let all_files = find_rust_files(&paths);
    
    println!("FOR LOOP COUNTS\n");
    
    let mut grand_total = 0;
    let mut grand_range = 0;
    let mut grand_iterator = 0;
    
    let mut file_results = Vec::new();
    
    for file in &all_files {
        match analyze_file(file) {
            Ok(stats) => {
                if stats.total_loops > 0 {
                    grand_total += stats.total_loops;
                    grand_range += stats.range_loops;
                    grand_iterator += stats.iterator_loops;
                    
                    file_results.push((file.clone(), stats));
                }
            }
            Err(e) => {
                eprintln!("Error analyzing {}: {}", file.display(), e);
            }
        }
    }
    
    // Sort by total loops (descending)
    file_results.sort_by(|a, b| b.1.total_loops.cmp(&a.1.total_loops));
    
    // Print top files with most loops
    println!("Files with most loops:\n");
    for (file, stats) in file_results.iter().take(20) {
        println!("  {:4} loops ({:3} range, {:3} iterator) - {}", 
            stats.total_loops, 
            stats.range_loops, 
            stats.iterator_loops,
            file.display()
        );
    }
    
    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!("SUMMARY");
    println!("═══════════════════════════════════════════════════════════════");
    println!();
    println!("Total files analyzed: {}", all_files.len());
    println!("Files with loops: {}", file_results.len());
    println!();
    println!("Total for loops: {}", grand_total);
    println!("  Range-based loops (0..n, 0..len()): {}", grand_range);
    println!("  Iterator-based loops (for x in collection): {}", grand_iterator);
    println!();
    
    if grand_total > 0 {
        let range_pct = (grand_range * 100) / grand_total;
        let iter_pct = (grand_iterator * 100) / grand_total;
        println!("Distribution:");
        println!("  Range loops: {}%", range_pct);
        println!("  Iterator loops: {}%", iter_pct);
    }
    
    Ok(())
}

