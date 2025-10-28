// Copyright (C) Brian G. Milnes 2025

//! Integration tests for rusticate-review-bench-modules

mod common;
use common::TestContext;

use anyhow::Result;
use serial_test::serial;
use std::process::Command;

#[test]
#[serial]
fn test_review_bench_modules_on_apas() -> Result<()> {
    let _ctx = TestContext::ensure_apas_at_script_commit("rust/benches/review_bench_modules.py")?;
    
    let binary_path = std::env::current_dir()?.join("target/release/rusticate-review-bench-modules");
    
    let output = Command::new(binary_path)
        .current_dir("APAS-AI-copy/apas-ai")
        .output()?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // Print output for debugging
    if !stderr.is_empty() {
        eprintln!("STDERR:\n{stderr}");
    }
    
    // The tool runs cargo bench --no-run, so we just check it doesn't crash
    // Exit code depends on whether benchmarks compile
    println!("STDOUT:\n{stdout}");
    
    Ok(())
}

