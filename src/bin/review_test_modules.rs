// Copyright (C) Brian G. Milnes 2025

//! Review: All test modules compile successfully
//! 
//! Replaces: scripts/rust/tests/review_test_modules.py
//! Rule: All test files should be discoverable and compile
//! Git commit: 584a672b6a34782766863c5f76a461d3297a741a
//! Binary: rusticate-review-test-modules

use anyhow::Result;
use std::process::Command;
use std::time::Instant;

fn main() -> Result<()> {
    let start = Instant::now();
    
    // Run cargo test to check if tests compile
    let output = Command::new("cargo")
        .args(&["test", "--tests", "--no-run", "--quiet"])
        .output()?;
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    if !output.status.success() {
        println!("❌ Test compilation check failed:");
        
        // Show error lines
        for line in stderr.lines() {
            let line_lower = line.to_lowercase();
            if line_lower.contains("error") || line_lower.contains("warning") {
                println!("   {}", line);
            }
        }
        
        let elapsed = start.elapsed().as_millis();
        println!("Completed in {}ms", elapsed);
        
        std::process::exit(1);
    }
    
    println!("✓ All test modules compile successfully");
    
    let elapsed = start.elapsed().as_millis();
    println!("Completed in {}ms", elapsed);
    
    Ok(())
}

