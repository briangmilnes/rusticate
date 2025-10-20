// Copyright (C) Brian G. Milnes 2025

use serial_test::serial;
use std::process::Command;

#[test]
#[serial]
fn test_review_stub_delegation_on_apas() {
    // Get binary path
    let binary = env!("CARGO_BIN_EXE_rusticate-review-stub-delegation");
    
    // Run on APAS-AI-copy
    let apas_path = "APAS-AI-copy/apas-ai";
    
    let output = Command::new(binary)
        .arg("-c")
        .current_dir(apas_path)
        .output()
        .expect("Failed to run rusticate-review-stub-delegation");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    println!("STDOUT:\n{}", stdout);
    println!("STDERR:\n{}", stderr);
    
    // Should find violations (exit code 1)
    assert_eq!(output.status.code(), Some(1), "Expected violations in APAS-AI");
    
    // Should have "Entering directory" line
    assert!(stdout.contains("Entering directory"), "Missing 'Entering directory' line");
    
    // Should find multiple violations
    assert!(stdout.contains("✗ Found"), "Missing summary line");
    
    // Should see specific violation patterns
    assert!(stdout.contains(": stub delegation between inherent impl and trait impl"),
        "Missing violation format");
    
    // Should see the BSTSetAVLMtEph example from user's question
    assert!(stdout.contains("BSTSetAVLMtEph.rs"), "Missing BSTSetAVLMtEph.rs violation");
    assert!(stdout.contains("overlapping methods"), "Missing overlapping methods info");
}

