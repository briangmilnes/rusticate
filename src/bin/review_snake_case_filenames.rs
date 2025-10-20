// Copyright (C) Brian G. Milnes 2025

//! Review: snake_case filename convention (standard Rust)
//! 
//! Enforces standard Rust filename convention: snake_case
//! RustRules.md: Standard Rust style is snake_case for filenames
//! Binary: rusticate-review-snake-case-filenames

use anyhow::Result;
use rusticate::{StandardArgs, format_number, find_rust_files};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    message: String,
}

fn is_snake_case(filename: &str) -> bool {
    let name = if filename.ends_with(".rs") {
        &filename[..filename.len() - 3]
    } else {
        filename
    };
    
    if name.is_empty() {
        return false;
    }
    
    // snake_case requirements:
    // 1. Must start with lowercase letter (distinguishes from PascalCase)
    // 2. Can contain lowercase letters, digits, and underscores
    // 3. No uppercase letters allowed (that would be PascalCase or camelCase)
    // 
    // Examples:
    //   snake_case:  my_file, binary_search, hash_map  ✓
    //   PascalCase:  MyFile (wrong - starts uppercase)
    //   camelCase:   myFile (wrong - has uppercase)
    //   SCREAMING:   MY_FILE (wrong - all uppercase)
    
    let first_char = name.chars().next().unwrap();
    if !first_char.is_lowercase() && !first_char.is_ascii_digit() {
        return false;
    }
    
    // Check that no uppercase letters exist anywhere
    for c in name.chars() {
        if c.is_uppercase() {
            return false;
        }
        // Only allow lowercase, digits, and underscores
        if !c.is_lowercase() && !c.is_ascii_digit() && c != '_' {
            return false;
        }
    }
    
    true
}

fn check_file(file_path: &Path) -> Option<Violation> {
    let filename = file_path.file_name()?.to_str()?;
    
    // Skip special files
    if matches!(filename, "lib.rs" | "main.rs" | "mod.rs") {
        return None;
    }
    
    if !is_snake_case(filename) {
        let name = if filename.ends_with(".rs") {
            &filename[..filename.len() - 3]
        } else {
            filename
        };
        
        let message = if name.chars().next().map_or(false, |c| c.is_uppercase()) {
            format!("File '{}' starts with uppercase (PascalCase), should be snake_case", filename)
        } else if name.chars().any(|c| c.is_uppercase()) {
            format!("File '{}' contains uppercase letters, should be snake_case", filename)
        } else {
            format!("File '{}' does not follow snake_case convention", filename)
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
    println!("Entering directory '{}'", base_dir.display());
    println!();
    
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
        println!("✓ All files follow snake_case naming convention");
    } else {
        println!("✗ Found {} violation(s):", format_number(violations.len()));
        println!();
        for v in &violations {
            // Use relative path from base_dir (Emacs will use compilation directory)
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                println!("{}:1: {}", rel_path.display(), v.message);
            }
        }
    }
    
    // Summary line
    println!();
    println!("Summary: {} files checked, {} files with violations",
             format_number(files.len()), format_number(violations.len()));
    
    let elapsed = start.elapsed().as_millis();
    println!("Completed in {}ms", elapsed);
    
    // Exit code: 0 if no violations, 1 if violations found
    if violations.is_empty() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

