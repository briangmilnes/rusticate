// Copyright (C) Brian G. Milnes 2025

//! Test review dispatcher - verify all review tools work via dispatcher

use std::process::Command;
use std::path::PathBuf;

fn get_binary_path(binary_name: &str) -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // Remove test binary name
    path.pop(); // Remove 'deps'
    path.push(binary_name);
    path
}

fn get_all_review_tools() -> Vec<&'static str> {
    // Only tools that are actually built (in Cargo.toml)
    vec![
        "bench-modules",
        "comment-placement",
        "doctests",
        "duplicate-bench-names",
        "duplicate-methods",
        "impl-order",
        "impl-trait-bounds",
        "import-order",
        "inherent-and-trait-impl",
        "inherent-plus-trait-impl",
        "integration-test-structure",
        "internal-method-impls",
        "logging",
        "minimize-ufcs-call-sites",
        "module-encapsulation",
        "no-extern-crate",
        "non-wildcard-uses",
        "no-trait-method-duplication",
        "pascal-case-filenames",
        "public-only-inherent-impls",
        "qualified-paths",
        "redundant-inherent-impls",
        "single-trait-impl",
        "snake-case-filenames",
        "st-mt-consistency",
        "string-hacking",
        "struct-file-naming",
        "stt-compliance",
        "stub-delegation",
        "test-modules",
        "trait-bound-mismatches",
        "trait-definition-order",
        "trait-method-conflicts",
        "trait-self-usage",
        "typeclasses",
        "variable-naming",
        "where-clause-simplification",
    ]
}

#[test]
fn test_dispatcher_no_args() {
    let binary = get_binary_path("rusticate-review");
    let output = Command::new(&binary)
        .output()
        .expect("Failed to run rusticate-review");
    
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rusticate-review: Run review tools by name or all at once"));
    assert!(stderr.contains("Available tools:"));
}

#[test]
fn test_dispatcher_help() {
    let binary = get_binary_path("rusticate-review");
    let output = Command::new(&binary)
        .arg("--help")
        .output()
        .expect("Failed to run rusticate-review --help");
    
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rusticate-review: Run review tools by name or all at once"));
}

#[test]
fn test_dispatcher_invalid_tool() {
    let binary = get_binary_path("rusticate-review");
    let output = Command::new(&binary)
        .arg("INVALID_TOOL_NAME")
        .output()
        .expect("Failed to run rusticate-review");
    
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Error: Unknown review tool 'INVALID_TOOL_NAME'"));
}

#[test]
fn test_all_review_tools_exist() {
    // Test that each review tool binary exists
    for tool in get_all_review_tools() {
        let binary_name = format!("rusticate-review-{}", tool);
        let binary = get_binary_path(&binary_name);
        assert!(
            binary.exists(),
            "Review tool binary does not exist: {}",
            binary_name
        );
    }
}

#[test]
fn test_dispatcher_runs_each_tool() {
    // Test that dispatcher can invoke each review tool
    let dispatcher = get_binary_path("rusticate-review");
    
    for tool in get_all_review_tools() {
        let output = Command::new(&dispatcher)
            .arg(tool)
            .arg("--help")
            .output()
            .unwrap_or_else(|_| panic!("Failed to run rusticate-review {}", tool));
        
        // Should either succeed or fail gracefully (some tools might require args)
        // Main thing is the dispatcher found and executed the tool
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        assert!(
            stdout.contains("Usage:") || stderr.contains("Usage:") || output.status.success(),
            "Tool {} did not run correctly through dispatcher",
            tool
        );
    }
}

#[test]
fn test_each_review_tool_directly() {
    // Test that each review tool can be run directly with --help
    for tool in get_all_review_tools() {
        let binary_name = format!("rusticate-review-{}", tool);
        let binary = get_binary_path(&binary_name);
        
        if !binary.exists() {
            panic!("Binary does not exist: {}", binary_name);
        }
        
        let output = Command::new(&binary)
            .arg("--help")
            .output()
            .unwrap_or_else(|_| panic!("Failed to run {}", binary_name));
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        assert!(
            output.status.success() || stdout.contains("Usage:") || stderr.contains("Usage:"),
            "Tool {} did not respond to --help correctly",
            binary_name
        );
    }
}

#[test]
fn test_specific_tools_sample() {
    // Test a few specific tools with actual arguments
    let dispatcher = get_binary_path("rusticate-review");
    
    // Test string-hacking on a simple file
    let test_file = "tests/test_review_dispatcher.rs";
    let output = Command::new(&dispatcher)
        .arg("string-hacking")
        .arg("-f")
        .arg(test_file)
        .output()
        .expect("Failed to run string-hacking via dispatcher");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("files checked") || stdout.contains("violations"));
    
    // Test logging (runs on src/bin by default)
    let output = Command::new(&dispatcher)
        .arg("logging")
        .output()
        .expect("Failed to run logging via dispatcher");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("files checked") || stdout.contains("logging"));
}

#[test]
#[ignore] // Ignore by default as it runs all tools (slow)
fn test_dispatcher_all_command() {
    let dispatcher = get_binary_path("rusticate-review");
    let output = Command::new(&dispatcher)
        .arg("all")
        .arg("-f")
        .arg("tests/test_review_dispatcher.rs")
        .output()
        .expect("Failed to run rusticate-review all");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Running all review tools"));
    assert!(stdout.contains("Summary"));
    assert!(stdout.contains("Ran") && stdout.contains("review tools"));
}

