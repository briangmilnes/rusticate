// Copyright (C) Brian G. Milnes 2025

//! Fix: Add missing pub mod declarations
//! 
//! For each .rs file (except binaries, lib.rs, main.rs) that is missing
//! a 'pub mod X {}' declaration, this tool adds one at the top of the file
//! based on the file's stem name.
//! 
//! Example: For `src/foo.rs` missing pub mod, adds:
//! ```rust
//! pub mod foo {}
//! ```
//! 
//! Binary: rusticate-fix-pub-mod

use anyhow::Result;
use rusticate::{StandardArgs, format_number, parse_source, find_rust_files};
use ra_ap_syntax::{SyntaxKind, ast::{self, AstNode, HasVisibility}, Edition};
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
            .open("analyses/fix_pub_mod.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

#[derive(Debug)]
struct Fix {
    file: PathBuf,
    module_name: String,
}

fn has_pub_mod(content: &str) -> bool {
    // Parse source and find MODULE node with pub visibility
    let parsed = ra_ap_syntax::SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    for node in root.children() {
        if node.kind() == SyntaxKind::MODULE {
            if let Some(module) = ast::Module::cast(node.clone()) {
                if let Some(vis) = module.visibility() {
                    if vis.to_string() == "pub" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn should_skip_file(file_path: &Path) -> bool {
    let file_name = file_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    
    // Skip lib.rs, main.rs, mod.rs (entry points/module declarations)
    if file_name == "lib.rs" || file_name == "main.rs" || file_name == "mod.rs" {
        return true;
    }
    
    // Skip test files (test_*.rs or in tests/)
    if file_name.starts_with("test_") {
        return true;
    }
    
    // Check if file is in tests/, benches/, or src/bin/ directories
    if let Some(parent) = file_path.parent() {
        if let Some(parent_name) = parent.file_name().and_then(|n| n.to_str()) {
            if parent_name == "bin" || parent_name == "tests" || parent_name == "benches" {
                return true;
            }
        }
    }
    
    false
}

fn get_module_name(file_path: &Path) -> Option<String> {
    file_path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

fn add_pub_mod(content: &str, module_name: &str) -> String {
    let pub_mod_declaration = format!("pub mod {} {{}}\n\n", module_name);
    
    // Add after copyright/license header if present
    let lines: Vec<&str> = content.lines().collect();
    let mut insert_pos = 0;
    
    // Skip copyright/license comments at the top
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.is_empty() {
            insert_pos = i + 1;
        } else {
            break;
        }
    }
    
    // Insert pub mod at the determined position
    let mut new_lines = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        new_lines.push(*line);
        if i == insert_pos - 1 {
            // Add blank line if previous line wasn't blank
            if !lines[i].trim().is_empty() {
                new_lines.push("");
            }
            new_lines.push(pub_mod_declaration.trim());
        }
    }
    
    new_lines.join("\n")
}

fn check_and_fix_file(file_path: &Path) -> Result<Option<Fix>> {
    if should_skip_file(file_path) {
        return Ok(None);
    }
    
    let content = fs::read_to_string(file_path)?;
    let _source_file = parse_source(&content)?; // Validate parsing
    
    if !has_pub_mod(&content) {
        let module_name = match get_module_name(file_path) {
            Some(name) => name,
            None => return Ok(None),
        };
        
        let new_content = add_pub_mod(&content, &module_name);
        fs::write(file_path, new_content)?;
        
        Ok(Some(Fix {
            file: file_path.to_path_buf(),
            module_name,
        }))
    } else {
        Ok(None)
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
    let mut all_fixes = Vec::new();
    
    for file in &files {
        match check_and_fix_file(file) {
            Ok(Some(fix)) => all_fixes.push(fix),
            Ok(None) => {}, // No fix needed or skipped
            Err(e) => {
                // Skip files that fail to parse
                eprintln!("Warning: Failed to process {}: {}", file.display(), e);
            }
        }
    }
    
    // Report fixes
    if all_fixes.is_empty() {
        log!("✓ No files needed pub mod declarations added");
    } else {
        log!("✓ Added pub mod declarations to {} file(s):", format_number(all_fixes.len()));
        log!("");
        for fix in &all_fixes {
            // Use relative path from base_dir (Emacs will use compilation directory)
            if let Ok(rel_path) = fix.file.strip_prefix(&base_dir) {
                log!("{}:1: Added pub mod {}", rel_path.display(), fix.module_name);
            } else {
                log!("{}:1: Added pub mod {}", fix.file.display(), fix.module_name);
            }
        }
        log!("");
        log!("Summary: {} files checked, {} pub mod declarations added", 
            format_number(files.len()), 
            format_number(all_fixes.len()));
    }
    
    log!("Completed in {}ms", start.elapsed().as_millis());
    Ok(())
}

