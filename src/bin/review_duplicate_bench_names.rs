// Copyright (C) Brian G. Milnes 2025

//! Review: No duplicate benchmark names in Cargo.toml
//! 
//! Replaces: scripts/rust/benches/review_duplicate_bench_names.py
//! Rule: Each [[bench]] entry must have a unique name
//! Git commit: 584a672b6a34782766863c5f76a461d3297a741a
//! Binary: rusticate-review-duplicate-bench-names

use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

fn main() -> Result<()> {
    let start = Instant::now();
    
    // Read Cargo.toml
    let cargo_content = fs::read_to_string("Cargo.toml")?;
    
    // Find all [[bench]] entries
    let mut bench_names: HashMap<String, Vec<String>> = HashMap::new();
    let lines: Vec<&str> = cargo_content.lines().collect();
    
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() == "[[bench]]" {
            // Look for name and path in next few lines
            let mut name = None;
            let mut path = None;
            
            for j in (i+1)..std::cmp::min(i+10, lines.len()) {
                let line = lines[j].trim();
                
                if line.starts_with("name") {
                    if let Some(value) = line.split('=').nth(1) {
                        name = Some(value.trim().trim_matches('"').to_string());
                    }
                } else if line.starts_with("path") {
                    if let Some(value) = line.split('=').nth(1) {
                        path = Some(value.trim().trim_matches('"').to_string());
                    }
                }
                
                // Stop at next section
                if line.starts_with('[') && line != "[[bench]]" {
                    break;
                }
            }
            
            if let (Some(n), Some(p)) = (name, path) {
                bench_names.entry(n).or_default().push(p);
            }
        }
        i += 1;
    }
    
    // Count total benches first
    let total_benches: usize = bench_names.values().map(|v| v.len()).sum();
    
    // Find duplicates
    let mut duplicates: Vec<(String, Vec<String>)> = bench_names
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .collect();
    
    duplicates.sort_by(|a, b| a.0.cmp(&b.0));
    
    if !duplicates.is_empty() {
        println!("✗ Found duplicate benchmark names in Cargo.toml:");
        println!();
        
        let mut total_violations = 0;
        for (name, paths) in &duplicates {
            println!("  name = \"{}\" appears {} times:", name, paths.len());
            for path in paths {
                println!("    - {}", path);
            }
            println!();
            total_violations += paths.len() - 1;
        }
        
        println!("Total violations: {}", total_violations);
        println!("\nFix: Each benchmark must have a unique name.");
        println!("Suggestion: Add chapter suffix like 'BenchFooChap37' and 'BenchFooChap38'");
        
        let elapsed = start.elapsed().as_millis();
        println!("Completed in {}ms", elapsed);
        
        std::process::exit(1);
    }
    println!("✓ All {} benchmark names are unique in Cargo.toml", total_benches);
    
    let elapsed = start.elapsed().as_millis();
    println!("Completed in {}ms", elapsed);
    
    Ok(())
}

