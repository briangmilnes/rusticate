// Copyright (C) Brian G. Milnes 2025

//! Integration tests for rusticate-review-import-order
//! 
//! Validates import ordering: std → external → internal with blank lines
//! Also checks Types::Types::* comes first within internal imports
//! Replaces: scripts/rust/review_import_order.py

use anyhow::Result;
use serial_test::serial;
use std::process::Command;

mod common;
use common::{TestContext, parse_number};

#[test]
#[serial]
fn test_review_import_order_on_apas() -> Result<()> {
    let ctx = TestContext::ensure_apas_at_script_commit("rust/review_import_order.py")?;
    
    // Run the binary
    let output = Command::new("./target/release/rusticate-review-import-order")
        .arg(&ctx.apas_path)
        .output()?;
    
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;

    // Print for debugging
    if !stderr.is_empty() {
        eprintln!("STDERR: {}", stderr);
    }
    
    println!("OUTPUT:\n{}", stdout);
    
    // The script should find violations (Types::Types::* is often last instead of first)
    // Exit code 1 = violations found
    assert_eq!(output.status.code(), Some(1), 
        "Expected exit code 1 (violations found), got {:?}\nOutput: {}", 
        output.status.code(), stdout);
    
    // Validate violation message format
    assert!(stdout.contains("✗ Found") && stdout.contains("violation(s)"),
        "Expected violation message not found in output:\n{}", stdout);
    
    // Should detect Types::Types::* ordering issues
    assert!(stdout.contains("Types::Types"),
        "Expected Types::Types ordering violations");
    
    // Parse and validate numeric output from Summary line
    // Expected format: "Summary: X files checked, Y files with violations, Z total violations"
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
    
    // Parse "Y files with violations"
    let files_with_violations = parts[1].trim().split_whitespace()
        .next()
        .and_then(|s| parse_number(s).ok())
        .expect("Failed to parse files with violations");
    
    // Parse "Z total violations"
    let total_violations = parts[2].trim().split_whitespace()
        .next()
        .and_then(|s| parse_number(s).ok())
        .expect("Failed to parse total violations");
    
    // Validate numbers - should find significant violations
    assert!(files_checked > 700, "Expected >700 files checked, got {}", files_checked);
    assert!(files_with_violations > 100, "Expected >100 files with violations, got {}", files_with_violations);
    assert!(total_violations > 1000, "Expected >1000 total violations, got {}", total_violations);
    
    // Validate timing line
    assert!(stdout.contains("Completed in"), "Missing timing line");
    assert!(stdout.contains("ms"), "Missing milliseconds unit");
    
    println!("✓ Test passed: {} files checked, {} files with violations, {} total violations", 
             files_checked, files_with_violations, total_violations);
    Ok(())
}

#[test]
#[serial]
fn test_review_import_order_on_rusticate() -> Result<()> {
    // Rusticate itself should have correct import order
    let output = Command::new("./target/release/rusticate-review-import-order")
        .arg(".")
        .output()?;
    
    let stdout = String::from_utf8(output.stdout)?;
    
    // Should pass (no violations)
    assert_eq!(output.status.code(), Some(0), 
        "Rusticate should have correct import order, got violations:\n{}", stdout);
    
    assert!(stdout.contains("✓ Import order correct"),
        "Expected success message not found");
    
    Ok(())
}

