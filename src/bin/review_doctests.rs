// Copyright (C) Brian G. Milnes 2025

//! Review: Doctest checking using AST
//! 
//! Finds doctests that fail to compile by parsing them with ra_ap_syntax
//! 
//! Binary: rusticate-review-doctests
//!
//! Uses AST parsing to extract and validate doctest code blocks

use anyhow::Result;
use ra_ap_syntax::{SourceFile, Edition};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use rusticate::{StandardArgs, format_number, find_rust_files};


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_doctests.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct DoctestFailure {
    file: PathBuf,
    line: usize,
    error_message: String,
    code_snippet: String,
}

fn extract_doctests(source: &str) -> Vec<(usize, String)> {
    let mut doctests = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut in_doctest = false;
    let mut doctest_start = 0;
    let mut current_doctest = String::new();
    
    for (line_idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        
        // Check for doc comment markers
        if trimmed.starts_with("//!") || trimmed.starts_with("///") {
            let content = trimmed.trim_start_matches("//!").trim_start_matches("///").trim();
            
            // Start of doctest
            if content.starts_with("```rust") || content == "```" && !in_doctest {
                in_doctest = true;
                doctest_start = line_idx + 1; // 1-indexed
                current_doctest.clear();
            }
            // End of doctest
            else if content.starts_with("```") && in_doctest {
                in_doctest = false;
                if !current_doctest.trim().is_empty() {
                    doctests.push((doctest_start, current_doctest.clone()));
                }
            }
            // Content of doctest
            else if in_doctest {
                current_doctest.push_str(content);
                current_doctest.push('\n');
            }
        }
    }
    
    doctests
}

fn check_doctest(code: &str) -> Option<String> {
    let parsed = SourceFile::parse(code, Edition::Edition2021);
    
    if !parsed.errors().is_empty() {
        let error = &parsed.errors()[0];
        Some(format!("{error}"))
    } else {
        None
    }
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    // Print compilation directory for Emacs compile-mode
    log!("Entering directory '{}'", base_dir.display());
    log!("");
    
    let search_dirs = args.get_search_dirs();
    
    let files = find_rust_files(&search_dirs);
    let mut all_failures = Vec::new();
    
    for file in &files {
        if let Ok(content) = fs::read_to_string(file) {
            let doctests = extract_doctests(&content);
            
            for (line_num, code) in doctests {
                if let Some(error) = check_doctest(&code) {
                    all_failures.push(DoctestFailure {
                        file: file.clone(),
                        line: line_num,
                        error_message: error,
                        code_snippet: code.lines().next().unwrap_or("").to_string(),
                    });
                }
            }
        }
    }
    
    if all_failures.is_empty() {
        log!("✓ All doctests parse correctly");
    } else {
        log!("✗ Found {} doctest failure(s):", format_number(all_failures.len()));
        log!("");
        
        for failure in &all_failures {
            if let Ok(rel_path) = failure.file.strip_prefix(&base_dir) {
                log!("{}:{}: doctest failed to parse", rel_path.display(), failure.line);
                log!("  Error: {}", failure.error_message);
                if !failure.code_snippet.is_empty() {
                    log!("  Code: {}", failure.code_snippet.trim());
                }
            }
        }
    }
    
    // Summary
    log!("");
    log!("Summary: {} doctest(s) failed", format_number(all_failures.len()));
    
    let elapsed = start.elapsed().as_millis();
    log!("Completed in {}ms", elapsed);
    
    // Exit code: 0 if no failures, 1 if failures found
    if all_failures.is_empty() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
