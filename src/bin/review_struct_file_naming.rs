// Copyright (C) Brian G. Milnes 2025

//! Review: Struct naming must match file name
//! 
//! Replaces: scripts/rust/src/review_struct_file_naming.py
//! Rule: The primary struct in a file should have the same base name as the file
//! Example: FooStEph.rs should contain `pub struct FooStEph`, not just `pub struct Foo`
//! Git commit: 584a672b6a34782766863c5f76a461d3297a741a
//! Binary: rusticate-review-struct-file-naming

use anyhow::Result;
use rusticate::{StandardArgs, format_number, parse_source, find_nodes, find_rust_files};
use ra_ap_syntax::{SyntaxKind, ast::{self, AstNode}};
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
            .open("analyses/review_struct_file_naming.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    struct_name: String,
    file_stem: String,
}

fn check_file(file_path: &Path, source: &str) -> Result<Vec<Violation>> {
    let source_file = parse_source(source)?;
    let root = source_file.syntax();
    
    // Get expected struct name from file name
    let file_stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    
    // Find all public struct definitions
    let struct_nodes = find_nodes(root, SyntaxKind::STRUCT);
    let mut violations = Vec::new();
    
    for struct_node in struct_nodes {
        if let Some(struct_ast) = ast::Struct::cast(struct_node.clone()) {
            // Check if struct has pub visibility
            let has_pub = struct_node.siblings_with_tokens(rowan::Direction::Prev)
                .any(|s| s.to_string().contains("pub"));
            
            if !has_pub {
                continue; // Skip private structs
            }
            
            // Get struct name from children
            let struct_name = struct_node.children_with_tokens()
                .filter_map(|child| child.into_token())
                .find(|token| token.kind() == SyntaxKind::IDENT)
                .map(|token| token.text().to_string());
            
            if let Some(struct_name) = struct_name {
                
                // Check if name matches file stem
                if struct_name != file_stem {
                    // Allow "S" suffix variant (FooS vs Foo)
                    let name_with_s = format!("{struct_name}S");
                    let stem_with_s = format!("{file_stem}S");
                    
                    if name_with_s != file_stem && struct_name != stem_with_s {
                        let line_num = rusticate::line_number(struct_ast.syntax(), source);
                        violations.push(Violation {
                            file: file_path.to_path_buf(),
                            line_num,
                            struct_name,
                            file_stem: file_stem.clone(),
                        });
                    }
                }
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
    let files = find_rust_files(&search_dirs);
    let mut all_violations = Vec::new();
    
    for file in &files {
        match fs::read_to_string(file) {
            Ok(source) => {
                match check_file(file, &source) {
                    Ok(violations) => all_violations.extend(violations),
                    Err(e) => {
                        eprintln!("Warning: Failed to parse {}: {}", file.display(), e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to read {}: {}", file.display(), e);
            }
        }
    }
    
    // Report violations
    if all_violations.is_empty() {
        log!("✓ Struct/File Naming: No violations found");
    } else {
        log!("✗ Struct/File Naming violations found:");
        log!("");
        
        for v in &all_violations {
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                log!("{}:{}: struct '{}' doesn't match file name '{}'", 
                         rel_path.display(), v.line_num, v.struct_name, v.file_stem);
            }
        }
        
        log!("");
        log!("Struct names should match their file names (excluding .rs extension).");
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

