// Copyright (C) Brian G. Milnes 2025

//! Count lines of code in Rust project
//! 
//! Replaces: scripts/analyze/count_loc.sh
//! Provides LOC metrics for the project

use anyhow::Result;
use rusticate::{StandardArgs, format_number, find_rust_files, parse_source};
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SyntaxNode};
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

#[derive(Debug, Default, Clone, Copy)]
struct VerusLocCounts {
    spec: usize,
    proof: usize,
    exec: usize,
    total: usize,
}

fn count_lines_in_file(path: &Path) -> Result<usize> {
    let content = fs::read_to_string(path)?;
    Ok(content.lines().count())
}

fn count_verus_lines_in_file(path: &Path) -> Result<VerusLocCounts> {
    let content = fs::read_to_string(path)?;
    
    let mut counts = VerusLocCounts::default();
    counts.total = content.lines().count();
    
    // Verus code is inside verus! {} macro, which the AST parser won't expand
    // So we need to do text-based analysis
    
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    
    while i < lines.len() {
        let line = lines[i].trim();
        
        // Skip empty lines and comments
        if line.is_empty() || line.starts_with("//") || line.starts_with("/*") {
            i += 1;
            continue;
        }
        
        // Check for function declarations
        if line.contains(" fn ") || line.starts_with("fn ") || line.starts_with("pub fn") {
            // Determine function type based on modifiers
            let is_spec = line.contains("spec fn") || line.contains("spec(");
            let is_proof = line.contains("proof fn");
            
            // Count lines in this function by finding the matching closing brace
            let func_start = i;
            let func_lines = count_function_lines(&lines, i);
            
            if is_spec {
                counts.spec += func_lines;
            } else if is_proof {
                counts.proof += func_lines;
            } else {
                counts.exec += func_lines;
                
                // Check if this exec function has proof blocks inside
                let proof_lines = count_proof_blocks_in_range(&lines, i, i + func_lines);
                if proof_lines > 0 {
                    counts.proof += proof_lines;
                    counts.exec -= proof_lines; // Don't double-count
                }
            }
            
            i += func_lines;
        } else {
            i += 1;
        }
    }
    
    Ok(counts)
}

fn count_function_lines(lines: &[&str], start: usize) -> usize {
    // Find the opening brace and count until matching closing brace
    let mut brace_count = 0;
    let mut found_open = false;
    
    for (offset, line) in lines[start..].iter().enumerate() {
        for ch in line.chars() {
            if ch == '{' {
                brace_count += 1;
                found_open = true;
            } else if ch == '}' {
                brace_count -= 1;
                if found_open && brace_count == 0 {
                    return offset + 1;
                }
            }
        }
    }
    
    // If we didn't find a matching brace, count until end
    1
}

fn count_proof_blocks_in_range(lines: &[&str], start: usize, end: usize) -> usize {
    let mut proof_lines = 0;
    let mut i = start;
    
    while i < end && i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("proof {") || line == "proof" && i + 1 < lines.len() && lines[i + 1].trim().starts_with('{') {
            // Count lines in this proof block
            let block_lines = count_function_lines(lines, i);
            proof_lines += block_lines;
            i += block_lines;
        } else {
            i += 1;
        }
    }
    
    proof_lines
}

fn count_proof_blocks_in_node(node: &SyntaxNode) -> usize {
    let mut proof_lines = 0;
    
    // Look for "proof { ... }" blocks by finding BLOCK_EXPR preceded by "proof" identifier
    for child in node.descendants() {
        if child.kind() == SyntaxKind::BLOCK_EXPR {
            let text = child.text().to_string();
            // Check if this looks like a proof block
            if let Some(prev) = child.prev_sibling_or_token() {
                if prev.to_string().trim() == "proof" {
                    proof_lines += text.lines().count();
                }
            }
        }
    }
    
    proof_lines
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

fn print_line(s: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    writeln!(stdout, "{s}")?;
    Ok(())
}

fn count_verus_project(_args: &StandardArgs, base_dir: &Path, search_dirs: &[PathBuf], start: std::time::Instant) -> Result<()> {
    let rust_files = find_rust_files(search_dirs);
    
    let mut total_spec = 0;
    let mut total_proof = 0;
    let mut total_exec = 0;
    let mut total_lines = 0;
    
    println!("Verus LOC (Spec/Proof/Exec)");
    println!();
    
    for file in &rust_files {
        if let Ok(counts) = count_verus_lines_in_file(file) {
            if let Ok(rel_path) = file.strip_prefix(base_dir) {
                println!("{:>8}/{:>8}/{:>8} {}", 
                    format_number(counts.spec),
                    format_number(counts.proof), 
                    format_number(counts.exec),
                    rel_path.display()
                );
            } else {
                println!("{:>8}/{:>8}/{:>8} {}", 
                    format_number(counts.spec),
                    format_number(counts.proof),
                    format_number(counts.exec),
                    file.display()
                );
            }
            total_spec += counts.spec;
            total_proof += counts.proof;
            total_exec += counts.exec;
            total_lines += counts.total;
        }
    }
    
    println!();
    println!("{:>8}/{:>8}/{:>8} total", 
        format_number(total_spec),
        format_number(total_proof),
        format_number(total_exec)
    );
    println!("{:>8} total lines", format_number(total_lines));
    println!();
    println!("{} files analyzed in {}ms", rust_files.len(), start.elapsed().as_millis());
    
    Ok(())
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let search_dirs = args.get_search_dirs();
    let is_verus = args.language == "Verus";
    
    // If Verus mode, use different counting
    if is_verus {
        return count_verus_project(&args, &base_dir, &search_dirs, start);
    }
    
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
    let mut src_file_count = 0;
    let mut tests_file_count = 0;
    let mut benches_file_count = 0;
    let mut scripts_file_count = 0;
    let mut other_file_count = 0;
    
    // Count SRC
    if !src_dirs.is_empty() {
        let _ = print_line("SRC LOC");
        let src_files = find_rust_files(&src_dirs);
        src_file_count = src_files.len();
        for file in &src_files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                    return Ok(());
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
        tests_file_count = tests_files.len();
        for file in &tests_files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                    return Ok(());
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
        benches_file_count = benches_files.len();
        for file in &benches_files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                    return Ok(());
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
        scripts_file_count = script_files.len();
        
        for file in &script_files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                    return Ok(());
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
        other_file_count += other_files.len();
        for file in &other_files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                    return Ok(());
                }
                other_total += lines;
            }
        }
    }
    
    // Count individual files
    if !files.is_empty() {
        other_file_count += files.len();
        for file in &files {
            if let Ok(lines) = count_lines_in_file(file) {
                if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                    if print_line(&format!("{:>8} {}", format_number(lines), rel_path.display())).is_err() {
                        return Ok(());
                    }
                } else if print_line(&format!("{:>8} {}", format_number(lines), file.display())).is_err() {
                    return Ok(());
                }
                other_total += lines;
            }
        }
    }
    
    // Total
    let total_loc = src_total + tests_total + benches_total + scripts_total + other_total;
    let total_files = src_file_count + tests_file_count + benches_file_count + scripts_file_count + other_file_count;
    
    // Summary line - only show categories that were searched
    if print_line("").is_err() { return Ok(()); }
    let mut summary_parts = Vec::new();
    if !src_dirs.is_empty() {
        summary_parts.push(format!("src {} files {} LOC", format_number(src_file_count), format_number(src_total)));
    }
    if !tests_dirs.is_empty() {
        summary_parts.push(format!("tests {} files {} LOC", format_number(tests_file_count), format_number(tests_total)));
    }
    if !benches_dirs.is_empty() {
        summary_parts.push(format!("benches {} files {} LOC", format_number(benches_file_count), format_number(benches_total)));
    }
    if scripts_total > 0 {
        summary_parts.push(format!("scripts {} files {} LOC", format_number(scripts_file_count), format_number(scripts_total)));
    }
    if other_total > 0 {
        summary_parts.push(format!("other {} files {} LOC", format_number(other_file_count), format_number(other_total)));
    }
    summary_parts.push(format!("total {} files {} LOC", format_number(total_files), format_number(total_loc)));
    
    if print_line(&format!("Summary: {}", summary_parts.join(", "))).is_err() { 
        return Ok(()); 
    }
    
    let elapsed = start.elapsed().as_millis();
    let _ = print_line(&format!("Completed in {elapsed}ms"));
    
    Ok(())
}

