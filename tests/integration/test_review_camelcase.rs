// Copyright (C) Brian G. Milnes 2025

//! Integration tests for rusticate-review-camelcase

use anyhow::Result;
use serial_test::serial;
use std::process::Command;
use super::common::{TestContext, parse_number};

#[test]
#[serial]
fn test_review_camelcase_on_apas() -> Result<()> {
    // Ensure APAS is at the correct commit (from Python script comment)
    let ctx = TestContext::ensure_apas_at_script_commit("rust/review_camelcase.py")?;
    
    // Run the binary
    let output = Command::new("./target/release/rusticate-review-camelcase")
        .arg(&ctx.apas_path)
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
    assert!(stdout.contains("✓ All files follow CamelCase naming convention"),
        "Expected success message not found in output:\n{}", stdout);
    
    // Parse and validate numeric output from Summary line
    // Expected format: "Summary: 719 files checked, 0 files with violations"
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
    
    // Parse "0 files with violations"
    let files_with_violations = parts[1].trim().split_whitespace()
        .next()
        .and_then(|s| parse_number(s).ok())
        .expect("Failed to parse files with violations");
    
    // Validate numbers
    assert!(files_checked > 700, "Expected >700 files checked, got {}", files_checked);
    assert_eq!(files_with_violations, 0, "Expected 0 files with violations");
    
    // Validate timing line
    assert!(stdout.contains("Completed in"), "Missing timing line");
    assert!(stdout.contains("ms"), "Missing milliseconds unit");
    
    println!("✓ Test passed: {} files checked, all follow CamelCase", files_checked);
    Ok(())
}

