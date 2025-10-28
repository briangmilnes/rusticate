// Copyright (C) Brian G. Milnes 2025

//! Integration tests for rusticate-review-module-encapsulation

use anyhow::Result;
use serial_test::serial;
use std::process::Command;
mod common;
use common::{TestContext, parse_number};

#[test]
#[serial]
fn test_review_module_encapsulation_on_apas() -> Result<()> {
    // Ensure APAS is at the correct commit (from Python script comment)
    let _ctx = TestContext::ensure_apas_at_script_commit("rust/src/review_module_encapsulation.py")?;
    
    let binary_path = std::env::current_dir()?.join("target/release/rusticate-review-module-encapsulation");
    
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
    
    // Validate exit code (1 = violations found at this commit)
    // Note: The 3 Simple/Clean/Minimal files are known violations
    assert_eq!(output.status.code(), Some(1), 
        "Expected exit code 1 (violations found), got {:?}\nOutput: {}", 
        output.status.code(), stdout);
    
    // Validate violation message
    assert!(stdout.contains("✗ Found") && stdout.contains("violation(s):"),
        "Expected violation message not found in output:\n{stdout}");
    
    // Parse and validate numeric output from Summary line
    // Expected format: "Summary: 265 files checked, 3 files with violations, 82 total violations"
    let summary_line = stdout.lines()
        .find(|line| line.starts_with("Summary:"))
        .expect("Summary line not found");
    
    // Extract numbers
    let parts: Vec<&str> = summary_line.split(',').collect();
    assert_eq!(parts.len(), 3, "Expected 3 parts in summary line");
    
    // Parse "265 files checked"
    let files_checked = parts[0].split_whitespace()
        .nth(1)
        .and_then(|s| parse_number(s).ok())
        .expect("Failed to parse files checked");
    
    // Parse "3 files with violations"
    let files_with_violations = parts[1].split_whitespace()
        .next()
        .and_then(|s| parse_number(s).ok())
        .expect("Failed to parse files with violations");
    
    // Parse "82 total violations"
    let total_violations = parts[2].split_whitespace()
        .next()
        .and_then(|s| parse_number(s).ok())
        .expect("Failed to parse total violations");
    
    // Validate numbers (checking that violations exist)
    assert!(files_checked > 250, "Expected >250 files checked, got {files_checked}");
    assert!(files_with_violations >= 3, "Expected at least 3 files with violations, got {files_with_violations}");
    assert!(total_violations >= 80, "Expected at least 80 total violations, got {total_violations}");
    
    // Validate that the violations are in the expected files
    assert!(stdout.contains("ArraySeqStEphSimple.rs"), "Expected Simple file in violations");
    assert!(stdout.contains("ArraySeqStEphClean.rs"), "Expected Clean file in violations");
    assert!(stdout.contains("ArraySeqStEphMinimal.rs"), "Expected Minimal file in violations");
    
    // Validate timing line
    assert!(stdout.contains("Completed in"), "Missing timing line");
    assert!(stdout.contains("ms"), "Missing milliseconds unit");
    
    println!("✓ Test passed: {files_checked} files checked, {total_violations} violations found in 3 experimental files");
    Ok(())
}

