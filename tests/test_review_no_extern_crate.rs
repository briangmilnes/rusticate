// Copyright (C) Brian G. Milnes 2025

//! Integration tests for rusticate-review-no-extern-crate

use anyhow::Result;
use serial_test::serial;
use std::process::Command;
mod common;
use common::{TestContext, parse_number};

#[test]
#[serial]
fn test_review_no_extern_crate_on_apas() -> Result<()> {
    // Ensure APAS is at the correct commit (from Python script comment)
    let _ctx = TestContext::ensure_apas_at_script_commit("rust/review_no_extern_crate.py")?;
    
    let binary_path = std::env::current_dir()?.join("target/release/rusticate-review-no-extern-crate");
    
    // Run the binary with -c (codebase: src, tests, benches)
    let output = Command::new(binary_path)
        .args(&["-c"])
        .current_dir("APAS-AI-copy/apas-ai")
        .output()?;
    
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    
    // Print for debugging
    if !stderr.is_empty() {
        eprintln!("STDERR: {}", stderr);
    }
    
    // Validate exit code (0 = no violations)
    assert_eq!(output.status.code(), Some(0), 
        "Expected exit code 0 (no violations), got {:?}\nOutput: {}", 
        output.status.code(), stdout);
    
    // Validate success message
    assert!(stdout.contains("✓ No 'extern crate' usage found"),
        "Expected success message not found in output:\n{}", stdout);
    
    // Parse and validate numeric output from Summary line
    // Expected format: "Summary: 719 files checked, 0 files with violations, 0 total violations"
    let summary_line = stdout.lines()
        .find(|line| line.starts_with("Summary:"))
        .expect("Summary line not found");
    
    // Extract numbers
    let parts: Vec<&str> = summary_line.split(',').collect();
    assert_eq!(parts.len(), 3, "Expected 3 parts in summary line");
    
    // Parse "719 files checked"
    let files_checked = parts[0].split_whitespace()
        .nth(1)
        .and_then(|s| parse_number(s).ok())
        .expect("Failed to parse files checked");
    
    // Parse "0 files with violations"
    let files_with_violations = parts[1].trim().split_whitespace()
        .next()
        .and_then(|s| parse_number(s).ok())
        .expect("Failed to parse files with violations");
    
    // Parse "0 total violations"
    let total_violations = parts[2].trim().split_whitespace()
        .next()
        .and_then(|s| parse_number(s).ok())
        .expect("Failed to parse total violations");
    
    // Validate numbers
    assert!(files_checked > 700, "Expected >700 files checked, got {}", files_checked);
    assert_eq!(files_with_violations, 0, "Expected 0 files with violations");
    assert_eq!(total_violations, 0, "Expected 0 total violations");
    
    // Validate timing line
    assert!(stdout.contains("Completed in"), "Missing timing line");
    assert!(stdout.contains("ms"), "Missing milliseconds unit");
    
    println!("✓ Test passed: {} files checked, no violations found", files_checked);
    Ok(())
}

