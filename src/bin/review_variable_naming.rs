// Copyright (C) Brian G. Milnes 2025

//! Review: Variable naming discipline
//! 
//! Replaces: scripts/rust/src/review_variable_naming.py
//! Rule: RustRules.md Lines 22-26
//! - No "temp" variables: Never use temp_vec, temp_data, temp_result, etc.
//! - No rock band/song names: Never use led_zeppelin, pink_floyd, stairway_to_heaven, etc.
//! Git commit: 584a672b6a34782766863c5f76a461d3297a741a
//! Binary: rusticate-review-variable-naming

use anyhow::Result;
use rusticate::{StandardArgs, format_number, find_rust_files};
use std::fs;
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
            .open("analyses/review_variable_naming.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    issue: String,
    line_content: String,
}

const ROCK_BANDS: &[&str] = &[
    "led_zeppelin", "pink_floyd", "the_beatles", "rolling_stones",
    "queen", "ac_dc", "metallica", "nirvana", "radiohead",
    "stairway_to_heaven", "bohemian_rhapsody", "hotel_california",
];

fn check_file(file_path: &Path) -> Result<Vec<Violation>> {
    let content = fs::read_to_string(file_path)?;
    let mut violations = Vec::new();
    
    for (line_idx, line) in content.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();
        
        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }
        
        // Check for temp_ pattern
        if line.contains("temp_") {
            // Find all occurrences
            for word in line.split(|c: char| !c.is_alphanumeric() && c != '_') {
                if word.starts_with("temp_") {
                    violations.push(Violation {
                        file: file_path.to_path_buf(),
                        line_num,
                        issue: format!("temp variable: {word}"),
                        line_content: trimmed.to_string(),
                    });
                }
            }
        }
        
        // Check for rock band names
        let line_lower = line.to_lowercase();
        for band in ROCK_BANDS {
            if line_lower.contains(band) {
                violations.push(Violation {
                    file: file_path.to_path_buf(),
                    line_num,
                    issue: format!("rock band name: {band}"),
                    line_content: trimmed.to_string(),
                });
                break; // Only report one per line
            }
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
    
    // This rule only applies to src/ (not tests/ or benches/)
    let src_dirs: Vec<_> = search_dirs.iter()
        .filter(|p| p.is_dir() && (p.ends_with("src") || p.components().any(|c| c.as_os_str() == "src")))
        .cloned()
        .collect();
    
    if src_dirs.is_empty() {
        log!("✓ No src/ directories to check");
        let elapsed = start.elapsed().as_millis();
        log!("Completed in {}ms", elapsed);
        return Ok(());
    }
    
    let files = find_rust_files(&src_dirs);
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
        log!("✓ No prohibited variable names found");
    } else {
        log!("✗ Found prohibited variable names (RustRules.md Lines 22-26):");
        log!("");
        
        for v in &all_violations {
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                log!("{}:{}: {}", 
                         rel_path.display(), v.line_num, v.issue);
                log!("  {}", v.line_content);
            }
        }
        
        log!("");
        log!("Fix: Use descriptive names like 'entries', 'result_vec', 'filtered_data'.");
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

