//! Fix: Binary Logging
//!
//! Adds file logging to analyses/binaryname.log for all rusticate binaries
//! Uses AST parsing to inject logging setup and replace println! with log! macro
//!
//! Binary: rusticate-fix-binary-logging

use anyhow::{Context, Result};
use ra_ap_syntax::{ast::{self, AstNode, HasName, HasModuleItem}, SyntaxKind, SourceFile, Edition};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn extract_binary_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn find_main_function(content: &str) -> Option<(usize, usize)> {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FN {
            if let Some(func) = ast::Fn::cast(node.clone()) {
                if let Some(name) = func.name() {
                    if name.to_string() == "main" {
                        if let Some(body) = func.body() {
                            let range = body.syntax().text_range();
                            let start: usize = range.start().into();
                            let end: usize = range.end().into();
                            
                            let body_text = &content[start..end];
                            if let Some(brace_pos) = body_text.find('{') {
                                let insert_pos = start + brace_pos + 1;
                                return Some((insert_pos, end - 1));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn has_file_logging(content: &str) -> bool {
    // Use AST to check for File::create calls with "analyses/" path
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut has_file_create = false;
    let mut has_analyses_path = false;
    
    for node in root.descendants() {
        // Check for File::create calls
        if node.kind() == SyntaxKind::CALL_EXPR {
            let text = node.to_string();
            if text.starts_with("File::create") || text.contains("::File::create") {
                has_file_create = true;
            }
        }
        
        // Check for "analyses/" in string literals
        if node.kind() == SyntaxKind::STRING {
            let text = node.to_string();
            if text.contains("analyses/") {
                has_analyses_path = true;
            }
        }
    }
    
    has_file_create && has_analyses_path
}

fn find_use_statements_end(content: &str) -> Option<usize> {
    let lines: Vec<&str> = content.lines().collect();
    let mut last_use_line = 0;
    
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ") || 
           (idx > 0 && lines[idx - 1].trim().starts_with("use ") && trimmed.is_empty()) {
            last_use_line = idx;
        } else if !trimmed.is_empty() && 
                  !trimmed.starts_with("//") && 
                  !trimmed.starts_with("use ") &&
                  last_use_line > 0 {
            break;
        }
    }
    
    if last_use_line > 0 {
        let mut pos = 0;
        for (idx, line) in lines.iter().enumerate() {
            if idx > last_use_line {
                break;
            }
            pos += line.len() + 1;
        }
        Some(pos)
    } else {
        None
    }
}

fn has_import_path(use_item: &ast::Use, target_path: &[&str]) -> bool {
    // Check if use statement imports from target_path (e.g., ["std", "fs"])
    if let Some(use_tree) = use_item.use_tree() {
        if let Some(path) = use_tree.path() {
            let segments: Vec<_> = path.segments().map(|s| s.to_string()).collect();
            // Check if segments start with target_path
            if segments.len() >= target_path.len() {
                return segments.iter()
                    .zip(target_path.iter())
                    .all(|(seg, target)| seg == target);
            }
        }
    }
    false
}

fn needs_use_std_fs(content: &str) -> bool {
    // Use AST to check for std::fs imports
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                if has_import_path(&use_item, &["std", "fs"]) {
                    return false;
                }
            }
        }
    }
    true
}

fn needs_use_std_io(content: &str) -> bool {
    // Use AST to check for std::io::Write import
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                // Check for std::io::Write
                if has_import_path(&use_item, &["std", "io"]) {
                    // Further check if "Write" is imported
                    if let Some(use_tree) = use_item.use_tree() {
                        let text = use_tree.syntax().text().to_string();
                        // Check if Write appears in the import (could be use std::io::Write or use std::io::{Write, ...})
                        if text.contains("Write") {
                            return false;
                        }
                    }
                }
            }
        }
    }
    true
}

fn inject_logging(content: &str, binary_name: &str) -> Result<String> {
    let mut new_content = content.to_string();
    
    // Step 1: Add use statements if needed
    if needs_use_std_fs(content) || needs_use_std_io(content) {
        if let Some(use_end_pos) = find_use_statements_end(content) {
            let mut imports = String::new();
            if needs_use_std_fs(content) {
                imports.push_str("use std::fs::{self, File};\n");
            }
            if needs_use_std_io(content) {
                imports.push_str("use std::io::Write;\n");
            }
            
            new_content.insert_str(use_end_pos, &imports);
        } else {
            // No use statements found, add after first item
            let parsed = SourceFile::parse(content, Edition::Edition2021);
            let tree = parsed.tree();
            
            for item in tree.items() {
                let range = item.syntax().text_range();
                let start: usize = range.start().into();
                
                let mut imports = String::new();
                if needs_use_std_fs(content) {
                    imports.push_str("use std::fs::{self, File};\n");
                }
                if needs_use_std_io(content) {
                    imports.push_str("use std::io::Write;\n");
                }
                imports.push('\n');
                
                new_content.insert_str(start, &imports);
                break;
            }
        }
    }
    
    // Re-parse to find main with updated positions
    let (main_start, _) = find_main_function(&new_content)
        .context("Could not find main function")?;
    
    // Step 2: Create the logging setup code with log! macro that uses println!
    let log_path = format!("analyses/{}.log", binary_name);
    // Use a placeholder for println to avoid it being replaced later
    let logging_setup = format!(
        r#"
    // Setup logging to {}
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = File::create("{}").ok();
    
    #[allow(unused_macros)]
    macro_rules! log {{
        ($($arg:tt)*) => {{{{
            let msg = format!($($arg)*);
            __PRINTLN__!("{{}}", msg);
            if let Some(ref mut f) = _log_file {{
                let _ = writeln!(f, "{{}}", msg);
            }}
        }}}};
    }}
"#, log_path, log_path);
    
    // Find actual insertion point
    let after_brace = &new_content[main_start..];
    let skip_whitespace = after_brace.chars()
        .take_while(|c| c.is_whitespace())
        .count();
    let insert_pos = main_start + skip_whitespace;
    
    new_content.insert_str(insert_pos, &logging_setup);
    
    // Step 3: Replace println! with log! in the main function body
    // Find the end of the macro definition to avoid replacing inside it
    let macro_end = new_content.find("macro_rules! log")
        .and_then(|start| {
            new_content[start..].find("}\n").map(|end| start + end + 2)
        })
        .unwrap_or(0);
    
    let before_replacement = &new_content[..macro_end];
    let after_macro = &new_content[macro_end..];
    
    // Replace println! but not eprintln! in code after the macro
    // Don't replace println!() with no args as log! requires format string
    let replaced = after_macro
        .replace("    println!(\"", "    log!(\"")
        .replace("    println!(&", "    log!(&")
        .replace("    println!(\"{", "    log!(\"{")
        .replace("\n    println!(\"", "\n    log!(\"")
        .replace("\n    println!(&", "\n    log!(&")
        .replace("\n    println!(\"{", "\n    log!(\"{");
    
    new_content = format!("{}{}", before_replacement, replaced);
    
    // Step 4: Replace the placeholder with actual println!
    new_content = new_content.replace("__PRINTLN__!", "println!");
    
    Ok(new_content)
}

fn fix_binary(file_path: &Path) -> Result<(String, bool)> {
    let binary_name = extract_binary_name_from_path(file_path);
    let content = fs::read_to_string(file_path)?;
    
    if has_file_logging(&content) {
        return Ok((binary_name, false));
    }
    
    let new_content = inject_logging(&content, &binary_name)?;
    fs::write(file_path, new_content)?;
    
    Ok((binary_name, true))
}

fn main() -> Result<()> {
    
    // Setup logging to analyses/fix_binary_logging.log
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = File::create("analyses/fix_binary_logging.log").ok();
    
    #[allow(unused_macros)]
    macro_rules! log {
        ($($arg:tt)*) => {{
            let msg = format!($($arg)*);
            println!("{}", msg);
            if let Some(ref mut f) = _log_file {
                let _ = writeln!(f, "{}", msg);
            }
        }};
    }
let start_time = Instant::now();
    
    let bin_dir = PathBuf::from("src/bin");
    
    if !bin_dir.exists() {
        eprintln!("Error: src/bin directory not found");
        std::process::exit(1);
    }
    
    log!("Entering directory '{}'", std::env::current_dir()?.display());
    println!();
    
    fs::create_dir_all("analyses")?;
    
    let mut fixed_count = 0;
    let mut errors = Vec::new();
    
    for entry in fs::read_dir(&bin_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let name = extract_binary_name_from_path(&path);
            
            // Skip our own tools
            if name == "fix_binary_logging" || name == "review_binary_logging" {
                continue;
            }
            
            match fix_binary(&path) {
                Ok((binary_name, was_fixed)) => {
                    if was_fixed {
                        log!("src/bin/{}.rs: Added logging to analyses/{}.log", 
                                 binary_name, binary_name);
                        fixed_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!("src/bin/{}.rs: Error: {}", name, e);
                    errors.push((name, e.to_string()));
                }
            }
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Binaries fixed: {}", fixed_count);
    log!("  Errors: {}", errors.len());
    
    if !errors.is_empty() {
        println!();
        log!("Errors:");
        for (name, err) in &errors {
            log!("  {}: {}", name, err);
        }
    }
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}
