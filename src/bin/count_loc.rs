// Copyright (C) Brian G. Milnes 2025

//! Count lines of code in Rust project
//! 
//! Replaces: scripts/analyze/count_loc.sh
//! Provides LOC metrics for the project

use anyhow::Result;
use rusticate::{StandardArgs, format_number, find_rust_files};
use std::fs;
use std::path::{Path, PathBuf};
use std::io::{self, Write};
use std::time::Instant;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/count_loc.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn count_lines_in_file(path: &Path) -> Result<usize> {
    let content = fs::read_to_string(path)?;
    Ok(content.lines().count())
}

fn find_script_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ext == "py" || ext == "sh" {
                        files.push(path);
                    }
                }
            } else if path.is_dir() {
                files.extend(find_script_files(&path));
            }
        }
    }
    files
}

// Helper to print, ignoring broken pipe errors
fn print_line(s: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    writeln!(stdout, "{}", s)?;
    Ok(())
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let search_dirs = args.get_search_dirs();
    
    // Categorize search directories
    let mut src_dirs = Vec::new();
    let mut tests_dirs = Vec::new();
    let mut benches_dirs = Vec::new();
    let mut other_dirs = Vec::new();
    let mut files = Vec::new();
    
    for path in search_dirs {
        if path.is_file() {
            files.push(path);
        } else if path.is_dir() {
            // Check if this is a src, tests, or benches directory
            if path.ends_with("src") || path.components().any(|c| c.as_os_str() == "src") {
                src_dirs.push(path);
            } else if path.ends_with("tests") || path.components().any(|c| c.as_os_str() == "tests") {
                tests_dirs.push(path);
            } else if path.ends_with("benches") || path.components().any(|c| c.as_os_str() == "benches") {
                benches_dirs.push(path);
            } else {
                other_dirs.push(path);
            }
        }
    }
    
    let mut src_total = 0;
    let mut tests_total = 0;
    let mut benches_total = 0;
    let mut other_total = 0;
    
    // Count SRC
    if !src_dirs.is_empty() {
        let _ = print_line("SRC LOC");
        let src_files = find_rust_files(&src_dirs);
        for file in &src_files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else {
                    if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                        return Ok(());
                    }
                }
                src_total += lines;
            }
        }
        if print_line(&format!("{:>8} total", format_number(src_total))).is_err() {
            return Ok(());
        }
        let _ = print_line("");
    }
    
    // Count Tests
    if !tests_dirs.is_empty() {
        if print_line("Tests LOC").is_err() { return Ok(()); }
        let tests_files = find_rust_files(&tests_dirs);
        for file in &tests_files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else {
                    if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                        return Ok(());
                    }
                }
                tests_total += lines;
            }
        }
        if print_line(&format!("{:>8} total", format_number(tests_total))).is_err() { return Ok(()); }
        let _ = print_line("");
    }
    
    // Count Benches
    if !benches_dirs.is_empty() {
        if print_line("Benches LOC").is_err() { return Ok(()); }
        let benches_files = find_rust_files(&benches_dirs);
        for file in &benches_files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else {
                    if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                        return Ok(());
                    }
                }
                benches_total += lines;
            }
        }
        if print_line(&format!("{:>8} total", format_number(benches_total))).is_err() { return Ok(()); }
        let _ = print_line("");
    }
    
    // Count scripts (if scripts/ directory exists in other_dirs)
    let mut scripts_total = 0;
    let scripts_dirs: Vec<_> = other_dirs.iter()
        .filter(|p| p.ends_with("scripts") || p.components().any(|c| c.as_os_str() == "scripts"))
        .cloned()
        .collect();
    
    if !scripts_dirs.is_empty() {
        if print_line("Scripts LOC").is_err() { return Ok(()); }
        let script_files = scripts_dirs.iter()
            .flat_map(|d| find_script_files(d))
            .collect::<Vec<_>>();
        
        for file in &script_files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else {
                    if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                        return Ok(());
                    }
                }
                scripts_total += lines;
            }
        }
        if print_line(&format!("{:>8} total", format_number(scripts_total))).is_err() { return Ok(()); }
        let _ = print_line("");
    }
    
    // Count other directories (non-src, non-tests, non-benches, non-scripts)
    let true_other_dirs: Vec<_> = other_dirs.iter()
        .filter(|p| !p.ends_with("scripts") && !p.components().any(|c| c.as_os_str() == "scripts"))
        .cloned()
        .collect();
    
    if !true_other_dirs.is_empty() {
        let other_files = find_rust_files(&true_other_dirs);
        for file in &other_files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else {
                    if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                        return Ok(());
                    }
                }
                other_total += lines;
            }
        }
    }
    
    // Count individual files
    if !files.is_empty() {
        for file in &files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else {
                    if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                        return Ok(());
                    }
                }
                other_total += lines;
            }
        }
    }
    
    // Total
    let total_loc = src_total + tests_total + benches_total + scripts_total + other_total;
    
    // Summary line - only show categories that were searched
    if print_line("").is_err() { return Ok(()); }
    let mut summary_parts = Vec::new();
    if !src_dirs.is_empty() {
        summary_parts.push(format!("src {} LOC", format_number(src_total)));
    }
    if !tests_dirs.is_empty() {
        summary_parts.push(format!("tests {} LOC", format_number(tests_total)));
    }
    if !benches_dirs.is_empty() {
        summary_parts.push(format!("benches {} LOC", format_number(benches_total)));
    }
    if scripts_total > 0 {
        summary_parts.push(format!("scripts {} LOC", format_number(scripts_total)));
    }
    if other_total > 0 {
        summary_parts.push(format!("other {} LOC", format_number(other_total)));
    }
    summary_parts.push(format!("total {} LOC", format_number(total_loc)));
    
    if print_line(&format!("Summary: {}", summary_parts.join(", "))).is_err() { 
        return Ok(()); 
    }
    
    let elapsed = start.elapsed().as_millis();
    let _ = print_line(&format!("Completed in {}ms", elapsed));
    
    Ok(())
}

