// Copyright (C) Brian G. Milnes 2025

//! Integration tests for rusticate-review-struct-file-naming

mod common;
use common::{TestContext, parse_number};

use anyhow::Result;
use serial_test::serial;
use std::process::Command;

#[test]
#[serial]
fn test_review_struct_file_naming_on_apas() -> Result<()> {
    let _ctx = TestContext::ensure_apas_at_script_commit("rust/src/review_struct_file_naming.py")?;
    
    let binary_path = std::env::current_dir()?.join("target/release/rusticate-review-struct-file-naming");
    
    let output = Command::new(binary_path)
        .args(["-d", "src"])
        .current_dir("APAS-AI-copy/apas-ai")
        .output()?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // Print output for debugging
    if !stderr.is_empty() {
        eprintln!("STDERR:\n{stderr}");
    }
    
    // Parse the summary line to get counts
    // Format: "Summary: X files checked, Y violations"
    let mut files_checked = 0;
    let mut violations = 0;
    
    for line in stdout.lines() {
        if line.starts_with("Summary:") {
            // Extract numbers from "Summary: X files checked, Y violations"
            let parts: Vec<&str> = line.split(',').collect();
            
            if let Some(files_part) = parts.first() {
                // "Summary: X files checked"
                if let Some(num_str) = files_part.split_whitespace().nth(1) {
                    files_checked = parse_number(num_str)?;
                }
            }
            
            if let Some(violations_part) = parts.get(1) {
                // "Y violations"
                if let Some(num_str) = violations_part.split_whitespace().next() {
                    violations = parse_number(num_str)?;
                }
            }
        }
    }
    
    // Validate results
    assert!(files_checked > 0, "Should check at least some files");
    
    // The exit code should be 0 if no violations, 1 if violations found
    if violations > 0 {
        assert!(!output.status.success(), "Should exit with error when violations found");
    } else {
        assert!(output.status.success(), "Should exit successfully when no violations");
    }
    
    Ok(())
}

