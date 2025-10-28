// Copyright (C) Brian G. Milnes 2025

//! Integration tests for rusticate-review-pascal-case-filenames
//! 
//! Validates that all Rust filenames use PascalCase (start with capital, no underscores)
//! Replaces: scripts/rust/review_camelcase.py

use anyhow::Result;
use serial_test::serial;
use std::process::Command;
mod common;
use common::{TestContext, parse_number};

#[test]
#[serial]
fn test_review_pascal_case_filenames_on_apas() -> Result<()> {
    // Python script was named "camelcase" but actually enforces PascalCase
    let _ctx = TestContext::ensure_apas_at_script_commit("rust/review_camelcase.py")?;
    
    let binary_path = std::env::current_dir()?.join("target/release/rusticate-review-pascal-case-filenames");
    
    // Run the binary with -c (codebase: src, tests, benches)
    let output = Command::new(binary_path)
        .args(["-c"])
        .current_dir("APAS-AI-copy/apas-ai")
        .output()?;
    
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    
    // Print for debugging
    if !stderr.is_empty() {
        eprintln!("STDERR: {stderr}");
    }
    
    // Validate exit code (1 = violations found)
    // APAS has 53 files with underscores (snake_case) that violate PascalCase rule
    assert_eq!(output.status.code(), Some(1), 
        "Expected exit code 1 (violations found), got {:?}\nOutput: {}", 
        output.status.code(), stdout);
    
    // Validate violation message
    assert!(stdout.contains("✗ Found") && stdout.contains("violation(s):"),
        "Expected violation message not found in output:\n{stdout}");
    
    // Validate that underscores are being detected
    assert!(stdout.contains("uses snake_case (underscore)"),
        "Expected underscore detection message not found");
    
    // Parse and validate numeric output from Summary line
    // Expected format: "Summary: 719 files checked, 53 files with violations"
    let summary_line = stdout.lines()
        .find(|line| line.starts_with("Summary:"))
        .expect("Summary line not found");
    
    // Extract numbers
    let parts: Vec<&str> = summary_line.split(',').collect();
    assert_eq!(parts.len(), 2, "Expected 2 parts in summary line");
    
    // Parse "719 files checked"
    let files_checked = parts[0].split_whitespace()
        .nth(1)
        .and_then(|s| parse_number(s).ok())
        .expect("Failed to parse files checked");
    
    // Parse "53 files with violations"
    let files_with_violations = parts[1].split_whitespace()
        .next()
        .and_then(|s| parse_number(s).ok())
        .expect("Failed to parse files with violations");
    
    // Validate numbers (53 files have underscores: Example41_3, Algorithm21_1, etc.)
    assert!(files_checked > 700, "Expected >700 files checked, got {files_checked}");
    assert_eq!(files_with_violations, 53, "Expected exactly 53 files with violations (underscore names)");
    
    // Validate timing line
    assert!(stdout.contains("Completed in"), "Missing timing line");
    assert!(stdout.contains("ms"), "Missing milliseconds unit");
    
    println!("✓ Test passed: {files_checked} files checked, {files_with_violations} violations found (files with underscores)");
    Ok(())
}

