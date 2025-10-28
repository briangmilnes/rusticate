// Copyright (C) Brian G. Milnes 2025

//! Integration tests for rusticate-review-minimize-ufcs-call-sites

use anyhow::Result;
use serial_test::serial;
use std::process::Command;

mod common;
use common::TestContext;

#[test]
#[serial]
fn test_review_minimize_ufcs_call_sites_on_apas() -> Result<()> {
    let _ctx = TestContext::ensure_apas_at_script_commit("rust/review_minimize_ufcs_call_sites.py")?;
    
    let binary_path = std::env::current_dir()?.join("target/release/rusticate-review-minimize-ufcs-call-sites");
    
    let output = Command::new(binary_path)
        .args(["-c"])
        .current_dir("APAS-AI-copy/apas-ai")
        .output()?;
    
    let stdout = String::from_utf8(output.stdout)?;
    
    // Exit code 1 = violations found (APAS-AI has some UFCS at call sites)
    assert_eq!(output.status.code(), Some(1), 
        "Expected exit code 1 (violations found), got {:?}\nOutput: {}", 
        output.status.code(), stdout);
    
    // Should have summary line
    assert!(stdout.contains("Summary:"));
    
    // Should have timing
    assert!(stdout.contains("Completed in"));
    assert!(stdout.contains("ms"));
    
    Ok(())
}
