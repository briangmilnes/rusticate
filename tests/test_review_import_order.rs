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
    let _ctx = TestContext::ensure_apas_at_script_commit("rust/review_import_order.py")?;
    
    let binary_path = std::env::current_dir()?.join("target/release/rusticate-review-import-order");
    
    // Run the binary with -d src
    let output = Command::new(binary_path)
        .args(["-d", "src"])
        .current_dir("APAS-AI-copy/apas-ai")
        .output()?;
    
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;

    // Print for debugging
    if !stderr.is_empty() {
        eprintln!("STDERR: {stderr}");
    }
    
    println!("OUTPUT:\n{stdout}");
    
    // The script should find violations (Types::Types::* is often last instead of first)
    // Exit code 1 = violations found
    assert_eq!(output.status.code(), Some(1), 
        "Expected exit code 1 (violations found), got {:?}\nOutput: {}", 
        output.status.code(), stdout);
    
    // Validate violation message format
    assert!(stdout.contains("✗ Found") && stdout.contains("violation(s)"),
        "Expected violation message not found in output:\n{stdout}");
    
    // Should detect Types::Types::* ordering issues
    assert!(stdout.contains("Types::Types"),
        "Expected Types::Types ordering violations");
    
    // Parse and validate numeric output from Summary line
    // Expected format: "Summary: X files checked, Y files with violations, Z total violations"
    let summary_line = stdout.lines()
        .find(|line| line.starts_with("Summary:"))
        .expect("Summary line not found");
    
    // Extract numbers by finding specific keywords
    // Format: "Summary: 719 files checked, 681 files with violations, 1,062 total violations"
    let extract_number_before = |text: &str, keyword: &str| -> Option<usize> {
        text.find(keyword)
            .and_then(|idx| {
                let before = &text[..idx];
                before.split_whitespace().last()
            })
            .and_then(|num_str| parse_number(num_str).ok())
    };
    
    let files_checked = extract_number_before(summary_line, " files checked")
        .expect("Failed to parse files checked");
    
    let files_with_violations = extract_number_before(summary_line, " files with violations")
        .expect("Failed to parse files with violations");
    
    let total_violations = extract_number_before(summary_line, " total violations")
        .expect("Failed to parse total violations");
    
    // Validate numbers - should find significant violations
    assert!(files_checked > 200, "Expected >200 files checked, got {files_checked}");
    assert!(files_with_violations > 50, "Expected >50 files with violations, got {files_with_violations}");
    assert!(total_violations > 100, "Expected >100 total violations, got {total_violations}");
    
    // Validate timing line
    assert!(stdout.contains("Completed in"), "Missing timing line");
    assert!(stdout.contains("ms"), "Missing milliseconds unit");
    
    println!("✓ Test passed: {files_checked} files checked, {files_with_violations} files with violations, {total_violations} total violations");
    Ok(())
}

#[test]
#[serial]
fn test_review_import_order_on_rusticate() -> Result<()> {
    // TODO: Fix import order in Rusticate itself
    // Currently Rusticate has import order violations that need to be fixed
    let binary_path = std::env::current_dir()?.join("target/release/rusticate-review-import-order");
    
    let output = Command::new(binary_path)
        .arg("-c")  // Check codebase (src, tests, benches)
        .output()?;
    
    let stdout = String::from_utf8(output.stdout)?;
    
    // Currently expects violations (exit code 1)
    // Once fixed, change this to assert exit code 0
    assert_eq!(output.status.code(), Some(1), 
        "Rusticate currently has import violations (TODO: fix):\n{stdout}");
    
    // Validate output format
    assert!(stdout.contains("✗ Found") && stdout.contains("violation(s)"),
        "Expected violation message in output");
    
    assert!(stdout.contains("Summary:") && stdout.contains("Completed in"),
        "Expected summary and timing lines");
    
    println!("Rusticate import violations (TODO: fix these):\n{stdout}");
    Ok(())
}

