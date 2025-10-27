// Copyright (C) Brian G. Milnes 2025

//! Review: Import order
//! 
//! Replaces: scripts/rust/review_import_order.py
//! RustRules.md Line 50: "Import order: after the module declaration add a blank line,
//! then all use std::… lines, then a blank line, then use statements from external crates,
//! then another blank line followed by use crate::Types::Types::*; if needed and the rest
//! of the internal crate::… imports."
//! 
//! RustRules.md Lines 75-86: "Inside src/ use crate::, outside src/ (tests/benches) use apas_ai::"
//! 
//! Binary: rusticate-review-import-order
//!
//! Uses AST parsing to find USE items and check their ordering

use anyhow::Result;
use rusticate::{StandardArgs, format_number, parse_source, find_nodes, find_rust_files};
use ra_ap_syntax::{SyntaxKind, SyntaxNode, ast::{self, AstNode}};
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
            .open("analyses/review_import_order.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    message: String,
    context: String,
}

#[derive(Debug, PartialEq, Eq)]
enum ImportSection {
    Std,      // std::, core::, alloc::
    External, // Other crates
    Internal, // crate:: or apas_ai::
}

fn classify_use_path(use_text: &str) -> ImportSection {
    // Extract the path after "use " and before any trailing tokens
    // e.g. "use crate::Foo;" -> "crate::Foo"
    // e.g. "use std::collections::HashMap;" -> "std::collections::HashMap"
    let trimmed = use_text.trim();
    let path = if let Some(start_idx) = trimmed.find("use ") {
        &trimmed[start_idx + 4..] // Skip "use "
    } else {
        trimmed
    };
    
    // Remove trailing semicolon and whitespace
    let path = path.trim().trim_end_matches(';').trim();
    
    if path.starts_with("std::") || path.starts_with("core::") || path.starts_with("alloc::") {
        ImportSection::Std
    } else if path.starts_with("crate::") || path.starts_with("apas_ai::") {
        ImportSection::Internal
    } else if path.starts_with("self::") || path.starts_with("super::") {
        // self/super are local, treat as internal
        ImportSection::Internal
    } else {
        ImportSection::External
    }
}

fn has_blank_line_between(node1: &SyntaxNode, node2: &SyntaxNode, source: &str) -> bool {
    let end_line = rusticate::line_number(node1, source);
    let start_line = rusticate::line_number(node2, source);
    
    if start_line <= end_line + 1 {
        return false; // Adjacent or same line
    }
    
    // Check if there's an empty line between them
    let lines: Vec<&str> = source.lines().collect();
    for line_idx in end_line..start_line - 1 {
        if let Some(line) = lines.get(line_idx) {
            let trimmed = line.trim();
            // Empty line (ignore comments)
            if trimmed.is_empty() {
                return true;
            }
        }
    }
    false
}

fn check_file(file_path: &Path, source: &str, in_src: bool) -> Result<Vec<Violation>> {
    let source_file = parse_source(source)?;
    let root = source_file.syntax();
    
    // Find all USE items
    let uses = find_nodes(root, SyntaxKind::USE);
    
    if uses.is_empty() {
        return Ok(Vec::new());
    }
    
    let mut violations = Vec::new();
    
    // Track the section we're in and detect transitions
    let mut current_section: Option<ImportSection> = None;
    let mut _seen_types_import = false;
    let mut seen_other_internal = false;
    let mut prev_use: Option<&SyntaxNode> = None;
    
    for use_node in &uses {
        let line_num = rusticate::line_number(use_node, source);
        let use_text = use_node.text().to_string();
        let section = classify_use_path(&use_text);
        
        // Check ordering violations
        match (&current_section, &section) {
            (Some(ImportSection::External), ImportSection::Std) => {
                violations.push(Violation {
                    file: file_path.to_path_buf(),
                    line_num,
                    message: "std import after external imports".to_string(),
                    context: use_text.trim().to_string(),
                });
            }
            (Some(ImportSection::Internal), ImportSection::Std) => {
                violations.push(Violation {
                    file: file_path.to_path_buf(),
                    line_num,
                    message: "std import after internal imports".to_string(),
                    context: use_text.trim().to_string(),
                });
            }
            (Some(ImportSection::Internal), ImportSection::External) => {
                violations.push(Violation {
                    file: file_path.to_path_buf(),
                    line_num,
                    message: "external import after internal imports".to_string(),
                    context: use_text.trim().to_string(),
                });
            }
            _ => {}
        }
        
        // Check for missing blank lines when transitioning sections
        if let (Some(prev_section), Some(prev_node)) = (&current_section, prev_use) {
            if prev_section != &section {
                // Section changed - should have blank line
                if !has_blank_line_between(prev_node, use_node, source) {
                    violations.push(Violation {
                        file: file_path.to_path_buf(),
                        line_num,
                        message: format!("missing blank line between {:?} and {:?} imports", prev_section, section),
                        context: use_text.trim().to_string(),
                    });
                }
            }
        }
        
        // Check crate:: vs apas_ai:: usage using AST
        let (has_apas_ai, has_crate) = if let Some(use_ast) = ast::Use::cast(use_node.clone()) {
            if let Some(use_tree) = use_ast.use_tree() {
                if let Some(path) = use_tree.path() {
                    let first_segment = path.segments().next().map(|s| s.to_string());
                    (first_segment == Some("apas_ai".to_string()), 
                     first_segment == Some("crate".to_string()))
                } else {
                    (false, false)
                }
            } else {
                (false, false)
            }
        } else {
            (false, false)
        };
        
        if has_apas_ai && in_src {
            violations.push(Violation {
                file: file_path.to_path_buf(),
                line_num,
                message: "use apas_ai:: in src/ (should be crate::)".to_string(),
                context: use_text.trim().to_string(),
            });
        } else if has_crate && !in_src {
            violations.push(Violation {
                file: file_path.to_path_buf(),
                line_num,
                message: "use crate:: in tests/benches (should be apas_ai::)".to_string(),
                context: use_text.trim().to_string(),
            });
        }
        
        // Check Types::Types::* ordering within internal section using AST
        if section == ImportSection::Internal {
            let is_types_import = if let Some(use_ast) = ast::Use::cast(use_node.clone()) {
                if let Some(use_tree) = use_ast.use_tree() {
                    if let Some(path) = use_tree.path() {
                        let segments: Vec<String> = path.segments().map(|s| s.to_string()).collect();
                        // Check for Types::Types pattern (consecutive "Types" segments)
                        segments.windows(2).any(|w| w[0] == "Types" && w[1] == "Types")
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            
            if is_types_import {
                if seen_other_internal {
                    // Types import comes after other internal imports
                    violations.push(Violation {
                        file: file_path.to_path_buf(),
                        line_num,
                        message: "use crate::Types::Types::* should come first within internal imports".to_string(),
                        context: use_text.trim().to_string(),
                    });
                }
                _seen_types_import = true;
            } else {
                // This is some other internal import
                seen_other_internal = true;
            }
        }
        
        current_section = Some(section);
        prev_use = Some(use_node);
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
    
    let mut all_violations = Vec::new();
    let mut total_files = 0;
    
    let files = find_rust_files(&search_dirs);
    
    for file in &files {
        total_files += 1;
        let in_src = file.components().any(|c| c.as_os_str() == "src");
        
        match fs::read_to_string(file) {
            Ok(source) => {
                match check_file(file, &source, in_src) {
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
        log!("✓ Import order correct: std → external → internal, with blank lines");
    } else {
        log!("✗ Found {} violation(s) (RustRules.md Lines 50, 75-86):", format_number(all_violations.len()));
        log!("");
        for v in &all_violations {
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                log!("{}:{}: {}", rel_path.display(), v.line_num, v.message);
                log!("  {}", v.context);
            }
        }
    }
    
    // Summary line
    let unique_files: std::collections::HashSet<_> = all_violations.iter().map(|v| &v.file).collect();
    log!("");
    log!("Summary: {} files checked, {} files with violations, {} total violations",
             format_number(total_files), format_number(unique_files.len()), format_number(all_violations.len()));
    
    let elapsed = start.elapsed().as_millis();
    log!("Completed in {}ms", elapsed);
    
    // Exit code: 0 if no violations, 1 if violations found
    if all_violations.is_empty() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

