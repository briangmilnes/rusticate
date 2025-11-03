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
    let source_file = parse_source(&content)?;
    let root = source_file.syntax();
    
    let mut counts = VerusLocCounts::default();
    counts.total = content.lines().count();
    
    // Find verus! macro calls and analyze their token tree
    for node in root.descendants() {
        if node.kind() == SyntaxKind::MACRO_CALL {
            if let Some(macro_call) = ast::MacroCall::cast(node.clone()) {
                // Check if this is a verus! macro
                if let Some(path) = macro_call.path() {
                    if path.to_string() == "verus" {
                        // Extract the token tree and walk it directly
                        if let Some(token_tree) = macro_call.token_tree() {
                            let tree_start: usize = token_tree.syntax().text_range().start().into();
                            analyze_verus_token_tree(token_tree.syntax(), &content, tree_start, &mut counts);
                        }
                    }
                }
            }
        }
    }
    
    Ok(counts)
}

fn analyze_verus_token_tree(tree: &SyntaxNode, content: &str, _tree_start: usize, counts: &mut VerusLocCounts) {
    // Walk the token tree looking for FN tokens
    // Collect preceding IDENT tokens to determine function type
    
    let mut i = 0;
    let tokens: Vec<_> = tree.descendants_with_tokens()
        .filter_map(|n| n.into_token())
        .collect();
    
    while i < tokens.len() {
        let token = &tokens[i];
        
        // Look for "fn" keyword
        if token.kind() == SyntaxKind::FN_KW {
            // Look backwards for modifiers: spec, proof, global, layout, exec, open, closed, tracked
            let mut is_spec = false;
            let mut is_proof = false;
            let mut is_global = false;
            let mut is_layout = false;
            
            // Check the 10 tokens before "fn" for Verus modifiers
            let start_idx = if i >= 10 { i - 10 } else { 0 };
            for j in start_idx..i {
                if tokens[j].kind() == SyntaxKind::IDENT {
                    let text = tokens[j].text();
                    match text {
                        "spec" => is_spec = true,
                        "proof" => is_proof = true,
                        "global" => is_global = true,
                        "layout" => is_layout = true,
                        _ => {}
                    }
                }
            }
            
            // Find the function body by looking for the matching braces
            let func_lines = count_function_lines_from_tokens(&tokens, i, content);
            
            // Categorize the function
            if is_spec || is_global || is_layout {
                // spec, global, layout are all specification-level
                counts.spec += func_lines;
            } else if is_proof {
                counts.proof += func_lines;
            } else {
                // Regular exec function
                counts.exec += func_lines;
                
                // Look for proof blocks inside exec functions
                let proof_lines = count_proof_blocks_from_tokens(&tokens, i, content);
                if proof_lines > 0 {
                    counts.proof += proof_lines;
                    counts.exec -= proof_lines;
                }
            }
        }
        
        i += 1;
    }
}

fn count_function_lines_from_tokens(tokens: &[ra_ap_syntax::SyntaxToken], fn_idx: usize, content: &str) -> usize {
    // Find the opening brace after the fn token
    let mut i = fn_idx + 1;
    while i < tokens.len() && tokens[i].kind() != SyntaxKind::L_CURLY {
        i += 1;
    }
    
    if i >= tokens.len() {
        return 1; // No body found
    }
    
    let start_offset: usize = tokens[fn_idx].text_range().start().into();
    
    // Find matching closing brace
    let mut brace_count = 0;
    while i < tokens.len() {
        match tokens[i].kind() {
            SyntaxKind::L_CURLY => brace_count += 1,
            SyntaxKind::R_CURLY => {
                brace_count -= 1;
                if brace_count == 0 {
                    let end_offset: usize = tokens[i].text_range().end().into();
                    let start_line = content[..start_offset].lines().count();
                    let end_line = content[..end_offset].lines().count();
                    return end_line - start_line + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    
    1 // Fallback
}

fn count_proof_blocks_from_tokens(tokens: &[ra_ap_syntax::SyntaxToken], fn_idx: usize, content: &str) -> usize {
    // Look for "proof" IDENT followed by "{"
    let mut total_lines = 0;
    let mut i = fn_idx;
    
    // Find the end of this function first
    let mut end_idx = i + 1;
    while end_idx < tokens.len() && tokens[end_idx].kind() != SyntaxKind::L_CURLY {
        end_idx += 1;
    }
    
    let mut brace_count = 0;
    while end_idx < tokens.len() {
        match tokens[end_idx].kind() {
            SyntaxKind::L_CURLY => brace_count += 1,
            SyntaxKind::R_CURLY => {
                brace_count -= 1;
                if brace_count == 0 {
                    break;
                }
            }
            _ => {}
        }
        end_idx += 1;
    }
    
    // Now search within this range for "proof {" patterns
    i = fn_idx;
    while i < end_idx {
        if tokens[i].kind() == SyntaxKind::IDENT && tokens[i].text() == "proof" {
            // Look for opening brace
            let mut j = i + 1;
            while j < end_idx && tokens[j].kind() == SyntaxKind::WHITESPACE {
                j += 1;
            }
            
            if j < end_idx && tokens[j].kind() == SyntaxKind::L_CURLY {
                // Found a proof block - count its lines
                let block_start: usize = tokens[i].text_range().start().into();
                let mut block_brace_count = 0;
                let mut k = j;
                
                while k < end_idx {
                    match tokens[k].kind() {
                        SyntaxKind::L_CURLY => block_brace_count += 1,
                        SyntaxKind::R_CURLY => {
                            block_brace_count -= 1;
                            if block_brace_count == 0 {
                                let block_end: usize = tokens[k].text_range().end().into();
                                let start_line = content[..block_start].lines().count();
                                let end_line = content[..block_end].lines().count();
                                total_lines += end_line - start_line + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
                
                i = k;
            }
        }
        i += 1;
    }
    
    total_lines
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

fn count_repositories(repo_dir: &PathBuf, language: &str, start: Instant) -> Result<()> {
    let projects = StandardArgs::find_cargo_projects(repo_dir);
    
    if projects.is_empty() {
        println!("No Cargo projects found in {}", repo_dir.display());
        return Ok(());
    }
    
    let is_verus = language == "Verus";
    
    // Store per-project results
    let mut all_results = Vec::new();
    
    for (idx, project) in projects.iter().enumerate() {
        let project_name = project.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        
        println!("=== Project {}/{}: {} ({}) ===", 
            idx + 1, 
            projects.len(), 
            project_name,
            project.display()
        );
        println!();
        
        // Get search dirs for this project
        let src_dir = project.join("src");
        let tests_dir = project.join("tests");
        let benches_dir = project.join("benches");
        
        let mut search_dirs = Vec::new();
        if src_dir.exists() { search_dirs.push(src_dir); }
        if tests_dir.exists() { search_dirs.push(tests_dir); }
        if benches_dir.exists() { search_dirs.push(benches_dir); }
        
        if search_dirs.is_empty() {
            println!("  (No src/tests/benches directories found)");
            println!();
            continue;
        }
        
        if is_verus {
            // Count Verus LOC for this project
            let rust_files = find_rust_files(&search_dirs);
            let mut spec = 0;
            let mut proof = 0;
            let mut exec = 0;
            let mut total = 0;
            
            for file in &rust_files {
                if let Ok(counts) = count_verus_lines_in_file(file) {
                    spec += counts.spec;
                    proof += counts.proof;
                    exec += counts.exec;
                    total += counts.total;
                }
            }
            
            println!("  Verus LOC: {:>8} spec / {:>8} proof / {:>8} exec",
                format_number(spec),
                format_number(proof),
                format_number(exec)
            );
            println!("  Total lines: {:>8}", format_number(total));
            println!("  Files: {}", rust_files.len());
            println!();
            
            all_results.push((project_name.to_string(), spec, proof, exec, total, rust_files.len()));
        } else {
            // Count regular Rust LOC for this project
            let rust_files = find_rust_files(&search_dirs);
            let mut loc = 0;
            
            for file in &rust_files {
                if let Ok(lines) = count_lines_in_file(file) {
                    loc += lines;
                }
            }
            
            println!("  LOC: {:>8}", format_number(loc));
            println!("  Files: {}", rust_files.len());
            println!();
            
            all_results.push((project_name.to_string(), 0, 0, 0, loc, rust_files.len()));
        }
    }
    
    // Print summary
    println!("=== GRAND TOTAL ({} projects) ===", projects.len());
    println!();
    
    if is_verus {
        let total_spec: usize = all_results.iter().map(|(_, s, _, _, _, _)| s).sum();
        let total_proof: usize = all_results.iter().map(|(_, _, p, _, _, _)| p).sum();
        let total_exec: usize = all_results.iter().map(|(_, _, _, e, _, _)| e).sum();
        let total_lines: usize = all_results.iter().map(|(_, _, _, _, t, _)| t).sum();
        let total_files: usize = all_results.iter().map(|(_, _, _, _, _, f)| f).sum();
        
        println!("  {:>8} spec / {:>8} proof / {:>8} exec",
            format_number(total_spec),
            format_number(total_proof),
            format_number(total_exec)
        );
        println!("  {:>8} total lines", format_number(total_lines));
        println!("  {} files in {} projects", total_files, projects.len());
    } else {
        let total_loc: usize = all_results.iter().map(|(_, _, _, _, t, _)| t).sum();
        let total_files: usize = all_results.iter().map(|(_, _, _, _, _, f)| f).sum();
        
        println!("  {:>8} total lines", format_number(total_loc));
        println!("  {} files in {} projects", total_files, projects.len());
    }
    
    println!();
    println!("Completed in {}ms", start.elapsed().as_millis());
    
    Ok(())
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    
    // Handle repository scanning mode
    if let Some(repo_dir) = &args.repositories {
        return count_repositories(repo_dir, &args.language, start);
    }
    
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

