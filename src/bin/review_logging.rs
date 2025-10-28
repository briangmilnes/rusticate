// Copyright (C) Brian G. Milnes 2025

//! Review: Check for dual stdout+file logging in binaries
//!
//! Checks that all rusticate binaries have:
//! - macro_rules! log definition
//! - Logging to analyses/<tool>.log files
//! - use std::fs import
//!
//! Binary: rusticate-review-logging

use anyhow::Result;
use ra_ap_syntax::{ast, AstNode, Edition, SourceFile, SyntaxKind};
use rusticate::StandardArgs;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

fn has_log_macro(content: &str) -> bool {
    content.contains("macro_rules! log")
}

fn has_analyses_logging(content: &str) -> bool {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Check for "analyses/" in string literals
    for node in root.descendants() {
        if node.kind() == SyntaxKind::STRING {
            let text = node.text().to_string();
            if text.contains("analyses/") && text.contains(".log") {
                return true;
            }
        }
    }
    false
}

fn has_std_fs_import(content: &str) -> bool {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node) {
                if let Some(use_tree) = use_item.use_tree() {
                    if let Some(path) = use_tree.path() {
                        let segments: Vec<_> = path.segments().collect();
                        if segments.len() == 2 {
                            if let (Some(first), Some(second)) = (segments.first(), segments.get(1)) {
                                let first_text = first.syntax().text().to_string();
                                let second_text = second.syntax().text().to_string();
                                if first_text == "std" && second_text == "fs" {
                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = std::env::current_dir()?;
    
    // Default to src/bin/ if no arguments or -c (codebase) mode
    let search_dirs = if args.paths.len() == 1 && args.paths[0] == base_dir {
        vec![base_dir.join("src").join("bin")]
    } else {
        args.paths.clone()
    };
    
    let mut missing_logging = Vec::new();
    let mut partial_logging = Vec::new();
    let mut complete_logging = Vec::new();
    
    for dir in &search_dirs {
        if !dir.exists() {
            continue;
        }
        
        let files: Vec<PathBuf> = if dir.is_file() {
            vec![dir.clone()]
        } else {
            fs::read_dir(dir)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "rs"))
                .collect()
        };
        
        for file in files {
            let content = fs::read_to_string(&file)?;
            let rel_path = file.strip_prefix(&base_dir)
                .unwrap_or(&file)
                .display()
                .to_string();
            
            let has_macro = has_log_macro(&content);
            let has_analyses = has_analyses_logging(&content);
            let has_fs = has_std_fs_import(&content);
            
            if !has_macro && !has_analyses {
                missing_logging.push((rel_path, "No logging".to_string()));
            } else if has_macro && has_analyses && has_fs {
                complete_logging.push(rel_path);
            } else {
                let mut issues = Vec::new();
                if !has_macro {
                    issues.push("missing log! macro");
                }
                if !has_analyses {
                    issues.push("missing analyses/ logging");
                }
                if !has_fs {
                    issues.push("missing std::fs import");
                }
                partial_logging.push((rel_path, issues.join(", ")));
            }
        }
    }
    
    // Print results
    if !missing_logging.is_empty() {
        println!("Missing logging:");
        for (file, issue) in &missing_logging {
            println!("  {file}: {issue}");
        }
        println!();
    }
    
    if !partial_logging.is_empty() {
        println!("Partial logging:");
        for (file, issues) in &partial_logging {
            println!("  {file}: {issues}");
        }
        println!();
    }
    
    if !complete_logging.is_empty() {
        println!("Complete logging ({} files):", complete_logging.len());
        for file in &complete_logging {
            println!("  {file}");
        }
        println!();
    }
    
    let total = missing_logging.len() + partial_logging.len() + complete_logging.len();
    let complete_count = complete_logging.len();
    let issues_count = missing_logging.len() + partial_logging.len();
    
    println!("Summary: {total} files checked, {complete_count} with complete logging, {issues_count} with issues");
    
    let elapsed = start.elapsed().as_millis();
    println!("Completed in {elapsed}ms");
    
    Ok(())
}

