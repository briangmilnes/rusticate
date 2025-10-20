// Copyright (C) Brian G. Milnes 2025

//! Integration test for rusticate-review-snake-case-filenames
//! 
//! Tests the snake_case filename checker (standard Rust convention)
//! No git commit specified - this tool doesn't correspond to a Python script

use anyhow::Result;
use serial_test::serial;
use std::process::Command;

mod common;
use common::parse_number;

#[test]
#[serial]
fn test_review_snake_case_on_apas() -> Result<()> {
    // APAS uses PascalCase, so should find many violations when checking for snake_case
    let binary_path = std::env::current_dir()?.join("target/release/rusticate-review-snake-case-filenames");
    
    let output = Command::new(binary_path)
        .args(&["-c"])
        .current_dir("APAS-AI-copy/apas-ai")
        .output()?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    
    // Should fail (violations found)
    assert_eq!(output.status.code(), Some(1));
    
    // Should have summary line
    assert!(stdout.contains("Summary:"));
    
    // Should have timing
    assert!(stdout.contains("Completed in"));
    assert!(stdout.contains("ms"));
    
    // Parse summary line: "Summary: N files checked, M violations"
    if let Some(summary_line) = stdout.lines().find(|l| l.starts_with("Summary:")) {
        // Extract the violations count
        if let Some(violations_part) = summary_line.split(',').nth(1) {
            let violations_str = violations_part
                .split_whitespace()
                .next()
                .expect("Should have violations count");
            let violations = parse_number(violations_str)?;
            
            // APAS uses PascalCase, so should have many violations
            assert!(violations > 500, "Expected many violations in APAS, got {}", violations);
        }
    }
    
    Ok(())
}

