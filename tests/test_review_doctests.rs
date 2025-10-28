// Copyright (C) Brian G. Milnes 2025

//! Integration test for rusticate-review-doctests
//! 
//! Tests the doctest syntax checker
//! No git commit specified - this is a new tool

use anyhow::Result;
use serial_test::serial;
use std::process::Command;

mod common;

#[test]
#[serial]
fn test_review_doctests_on_apas() -> Result<()> {
    // No git commit for this tool - it's a new Rusticate-only tool
    
    let binary_path = std::env::current_dir()?.join("target/release/rusticate-review-doctests");
    
    let output = Command::new(binary_path)
        .args(["-d", "src"])
        .current_dir("APAS-AI-copy/apas-ai")
        .output()?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    if !stderr.is_empty() {
        eprintln!("STDERR:\n{stderr}");
    }
    
    // Should produce output
    assert!(!stdout.is_empty(), "Expected non-empty output");
    
    // Should have timing
    assert!(stdout.contains("Completed in") && stdout.contains("ms"));
    
    Ok(())
}

