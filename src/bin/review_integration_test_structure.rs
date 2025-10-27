// Copyright (C) Brian G. Milnes 2025

//! Review: Integration test structure (MANDATORY)
//! 
//! Replaces: scripts/rust/tests/review_integration_test_structure.py
//! Rule: RustRules.md Lines 292-298
//! "Integration tests must have test functions at the root level of the file.
//! NEVER use #[cfg(test)] modules in integration test files - this prevents test discovery."
//! Git commit: 584a672b6a34782766863c5f76a461d3297a741a
//! Binary: rusticate-review-integration-test-structure
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use rusticate::{StandardArgs, format_number, find_rust_files};


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_integration_test_structure.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    line_content: String,
}

fn check_file(file_path: &Path) -> Result<Vec<Violation>> {
    let content = fs::read_to_string(file_path)?;
    let mut violations = Vec::new();
    
    let mut in_multiline_comment = false;
    
    for (line_idx, line) in content.lines().enumerate() {
        let line_num = line_idx + 1;
        
        // Handle multi-line comments
        if line.contains("/*") {
            in_multiline_comment = true;
        }
        if line.contains("*/") {
            in_multiline_comment = false;
            continue;
        }
        if in_multiline_comment {
            continue;
        }
        
        let trimmed = line.trim();
        
        // Skip single-line comments
        if trimmed.starts_with("//") {
            continue;
        }
        
        // Check for #[cfg(test)]
        if line.contains("#[cfg(test)]") {
            violations.push(Violation {
                file: file_path.to_path_buf(),
                line_num,
                line_content: trimmed.to_string(),
            });
        }
    }
    
    Ok(violations)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    // Print compilation directory for Emacs compile-mode
    log!("Entering directory '{}'", base_dir.display());
    log!("");
    
    let search_dirs = args.get_search_dirs();
    
    // This rule only applies to tests/ directory
    let tests_dirs: Vec<_> = search_dirs.iter()
        .filter(|p| p.is_dir() && (p.ends_with("tests") || p.components().any(|c| c.as_os_str() == "tests")))
        .cloned()
        .collect();
    
    if tests_dirs.is_empty() {
        log!("✓ No tests/ directory found");
        let elapsed = start.elapsed().as_millis();
        log!("Completed in {}ms", elapsed);
        return Ok(());
    }
    
    let files = find_rust_files(&tests_dirs);
    let mut all_violations = Vec::new();
    
    for file in &files {
        match check_file(file) {
            Ok(violations) => all_violations.extend(violations),
            Err(e) => {
                eprintln!("Warning: Failed to read {}: {}", file.display(), e);
            }
        }
    }
    
    // Report violations
    if all_violations.is_empty() {
        log!("✓ No #[cfg(test)] modules in integration tests");
    } else {
        log!("✗ Found #[cfg(test)] in integration tests (RustRules.md Lines 292-298):");
        log!("");
        
        for v in &all_violations {
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                log!("{}:{}: #[cfg(test)] in integration test", 
                         rel_path.display(), v.line_num);
                log!("  {}", v.line_content);
            }
        }
        
        log!("");
        log!("Fix: Remove #[cfg(test)] modules from integration tests.");
        log!("Integration tests should have #[test] functions at root level.");
    }
    
    // Summary
    log!("");
    log!("Summary: {} files checked, {} violations", 
             format_number(files.len()), format_number(all_violations.len()));
    
    let elapsed = start.elapsed().as_millis();
    log!("Completed in {}ms", elapsed);
    
    if all_violations.is_empty() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

