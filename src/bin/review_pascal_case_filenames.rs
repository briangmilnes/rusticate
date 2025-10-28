// Copyright (C) Brian G. Milnes 2025

//! Review: PascalCase filename convention
//! 
//! Replaces: scripts/rust/review_camelcase.py
//! RustRules.md Lines 303-306: File names should be in PascalCase (start with capital, no underscores)
//! Binary: rusticate-review-pascal-case-filenames

use anyhow::Result;
use rusticate::{StandardArgs, format_number, find_rust_files};
use std::path::{Path, PathBuf};
use std::time::Instant;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_pascal_case_filenames.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct Violation {
    file: PathBuf,
    message: String,
}

fn is_pascalcase(filename: &str) -> bool {
    let name = if filename.ends_with(".rs") {
        &filename[..filename.len() - 3]
    } else {
        filename
    };
    
    if name.is_empty() {
        return false;
    }
    
    // PascalCase requirements:
    // 1. Must start with uppercase letter (distinguishes from camelCase)
    // 2. Must NOT contain underscores (that's snake_case)
    // 3. Numbers are allowed (e.g., Chap42 is fine)
    // 
    // Examples:
    //   PascalCase:  MyFile, ArraySeq, BinaryHeapPQ
    //   camelCase:   myFile (wrong - starts lowercase)
    //   snake_case:  my_file (wrong - has underscore)
    
    let first_char = name.chars().next().unwrap();
    if !first_char.is_uppercase() {
        return false;
    }
    
    // Check for underscores (indicates snake_case, not PascalCase)
    if name.contains('_') {
        return false;
    }
    
    true
}

fn check_file(file_path: &Path) -> Option<Violation> {
    let filename = file_path.file_name()?.to_str()?;
    
    // Skip special files
    if matches!(filename, "lib.rs" | "main.rs" | "mod.rs") {
        return None;
    }
    
    if !is_pascalcase(filename) {
        let name = if filename.ends_with(".rs") {
            &filename[..filename.len() - 3]
        } else {
            filename
        };
        
        let message = if name.contains('_') {
            format!("File '{filename}' uses snake_case (underscore), should be PascalCase")
        } else if name.chars().next().is_some_and(|c| c.is_lowercase()) {
            format!("File '{filename}' starts with lowercase (camelCase), should be PascalCase")
        } else {
            format!("File '{filename}' does not follow PascalCase convention")
        };
        
        return Some(Violation {
            file: file_path.to_path_buf(),
            message,
        });
    }
    
    None
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
    let mut violations = Vec::new();
    
    for file in &files {
        if let Some(violation) = check_file(file) {
            violations.push(violation);
        }
    }
    
    // Report violations
    if violations.is_empty() {
        log!("✓ All files follow PascalCase naming convention");
    } else {
        log!("✗ Found {} violation(s):", format_number(violations.len()));
        log!("");
        for v in &violations {
            // Use relative path from base_dir (Emacs will use compilation directory)
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                log!("{}:1: {}", rel_path.display(), v.message);
            }
        }
    }
    
    // Summary line
    log!("");
    log!("Summary: {} files checked, {} files with violations",
             format_number(files.len()), format_number(violations.len()));
    
    let elapsed = start.elapsed().as_millis();
    log!("Completed in {}ms", elapsed);
    
    // Exit code: 0 if no violations, 1 if violations found
    if violations.is_empty() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

