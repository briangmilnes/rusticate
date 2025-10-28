// Copyright (C) Brian G. Milnes 2025

use serial_test::serial;
use std::process::Command;

#[test]
#[serial]
fn test_review_public_only_inherent_impls_on_apas() {
    // Get binary path
    let binary = env!("CARGO_BIN_EXE_rusticate-review-public-only-inherent-impls");
    
    // Run on APAS-AI-copy
    let apas_path = "APAS-AI-copy/apas-ai";
    
    let output = Command::new(binary)
        .arg("-c")
        .current_dir(apas_path)
        .output()
        .expect("Failed to run rusticate-review-public-only-inherent-impls");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    println!("STDOUT:\n{stdout}");
    println!("STDERR:\n{stderr}");
    
    // Should find violations (exit code 1)
    assert_eq!(output.status.code(), Some(1), "Expected violations in APAS-AI");
    
    // Should have "Entering directory" line
    assert!(stdout.contains("Entering directory"), "Missing 'Entering directory' line");
    
    // Should find exactly 17 violations in latest main
    assert!(stdout.contains("✗ Found 17 inherent impl(s) with only public methods"), 
        "Expected exactly 17 violations");
    
    // Should see specific violation patterns
    assert!(stdout.contains(": inherent impl with only public methods (move to trait impl)"),
        "Missing violation format");
    
    // Should see some specific files (from latest main branch)
    assert!(stdout.contains("AVLTreeSeqStEph.rs"), "Missing AVLTreeSeqStEph.rs violation");
}

