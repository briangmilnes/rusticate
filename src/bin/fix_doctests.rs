// Copyright (C) Brian G. Milnes 2025

//! Fix: Doctest issues using AST
//! 
//! Automatically fixes common doctest failures by analyzing AST and adding missing imports
//! 
//! Binary: rusticate-fix-doctests
//!
//! Uses AST parsing and byte-offset manipulation to insert imports precisely

use anyhow::Result;
use ra_ap_syntax::{SourceFile, Edition, SyntaxKind, ast::AstNode};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use rusticate::{StandardArgs, format_number, find_rust_files};

#[derive(Debug, Clone)]
struct DoctestFix {
    line: usize,
    byte_offset: usize,  // Where to insert (after ```rust line)
    suggested_imports: Vec<String>,
}

/// Find doctests in source using byte offsets
fn find_doctests_with_offsets(source: &str) -> Vec<(usize, usize, String)> {
    // Returns: (line_number, byte_offset_after_rust_marker, code_content)
    let mut doctests = Vec::new();
    let mut current_line = 1;
    let mut byte_pos = 0;
    let mut in_doctest = false;
    let mut doctest_start_line = 0;
    let mut doctest_insert_offset = 0;
    let mut current_doctest = String::new();
    
    for line in source.lines() {
        let line_start = byte_pos;
        let line_len = line.len();
        
        let trimmed = line.trim_start();
        
        if trimmed.starts_with("//!") || trimmed.starts_with("///") {
            let content = trimmed
                .trim_start_matches("//!")
                .trim_start_matches("///")
                .trim();
            
            if content.starts_with("```rust") || content == "```" && !in_doctest {
                in_doctest = true;
                doctest_start_line = current_line;
                // Insert point is at the end of this line (after newline)
                doctest_insert_offset = line_start + line_len + 1; // +1 for \n
                current_doctest.clear();
            } else if content.starts_with("```") && in_doctest {
                in_doctest = false;
                if !current_doctest.trim().is_empty() {
                    doctests.push((doctest_start_line, doctest_insert_offset, current_doctest.clone()));
                }
            } else if in_doctest {
                current_doctest.push_str(content);
                current_doctest.push('\n');
            }
        }
        
        byte_pos = line_start + line_len + 1; // +1 for \n
        current_line += 1;
    }
    
    doctests
}

/// Infer missing imports from code patterns
fn infer_missing_imports(code: &str) -> Vec<String> {
    let mut imports = HashSet::new();
    
    // Parse to check for syntax errors (indicates missing imports)
    let parsed = SourceFile::parse(code, Edition::Edition2021);
    let has_errors = !parsed.errors().is_empty();
    
    // Check if there are already use statements using AST if code parses
    if !has_errors {
        let tree = parsed.tree();
        let root = tree.syntax();
        // Check for USE nodes in the AST
        let has_use = root.descendants().any(|node| node.kind() == SyntaxKind::USE);
        if has_use {
            return Vec::new(); // Code has imports and no errors, no fixes needed
        }
    }
    
    // Check for common APAS patterns
    // Try AST-based detection first for parseable fragments
    let mut has_triple_pattern = false;
    if !has_errors {
        // If code parses, check for tuple/array patterns using AST
        let tree = parsed.tree();
        let root = tree.syntax();
        for node in root.descendants() {
            let kind = node.kind();
            if kind == SyntaxKind::TUPLE_EXPR || kind == SyntaxKind::ARRAY_EXPR {
                // Check if it looks like APAS triple usage without explicit import
                if !code.contains("use") && !code.contains("Triple") {
                    has_triple_pattern = true;
                    break;
                }
            }
        }
    } else {
        // Fallback to heuristics for unparseable fragments
        // Note: String checks here because doctest fragment may not be valid Rust
        has_triple_pattern = (code.contains("[(") || code.contains("(\"")) 
            && !code.contains("use") && !code.contains("Triple");
    }
    
    let has_graph_macro = code.contains("GraphStEph") && code.contains("Lit!");
    
    if has_triple_pattern || has_graph_macro {
        imports.insert("use apas_ai::Types::Types::Triple;".to_string());
    }
    
    // Check for type usage without imports
    let type_patterns = [
        ("Pair", "use apas_ai::Types::Types::Pair;"),
        ("N", "use apas_ai::Types::Types::N;"),
        ("OrderedFloat", "use ordered_float::OrderedFloat;"),
    ];
    
    for (name, import) in &type_patterns {
        if code.contains(name) && !code.contains("use") {
            imports.insert(import.to_string());
        }
    }
    
    // If we found nothing specific but there are errors, suggest wildcard
    if imports.is_empty() && has_errors && !code.contains("use") {
        imports.insert("use apas_ai::Types::Types::*;".to_string());
    }
    
    imports.into_iter().collect()
}

/// Apply fix using byte-offset insertion (not line-based surgery)
fn apply_fix_with_offsets(
    source: &str,
    byte_offset: usize,
    imports: &[String],
    comment_prefix: &str,
    indent: &str,
) -> String {
    let mut result = String::new();
    
    // Everything before insert point
    result.push_str(&source[..byte_offset]);
    
    // Insert each import as a new doc comment line
    for import in imports {
        result.push_str(indent);
        result.push_str(comment_prefix);
        result.push_str(import);
        result.push('\n');
    }
    
    // Everything after insert point
    result.push_str(&source[byte_offset..]);
    
    result
}

fn check_file(file_path: &PathBuf) -> Result<Vec<DoctestFix>> {
    let source = fs::read_to_string(file_path)?;
    let doctests = find_doctests_with_offsets(&source);
    
    if doctests.is_empty() {
        return Ok(Vec::new());
    }
    
    let mut fixes = Vec::new();
    
    for (line_num, byte_offset, code) in doctests {
        let suggested_imports = infer_missing_imports(&code);
        
        if !suggested_imports.is_empty() {
            fixes.push(DoctestFix {
                line: line_num,
                byte_offset,
                suggested_imports,
            });
        }
    }
    
    Ok(fixes)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let cli_args: Vec<String> = std::env::args().collect();
    let dry_run = cli_args.iter().any(|a| a == "--dry-run");
    
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    println!("Entering directory '{}'", base_dir.display());
    println!();
    
    if dry_run {
        println!("DRY RUN MODE: Will not modify files");
        println!();
    }
    
    let files = find_rust_files(&args.paths);
    let mut total_fixes = 0;
    let mut files_fixed = 0;
    
    for file in &files {
        let rel_path = file.strip_prefix(&base_dir).unwrap_or(file);
        
        match check_file(file) {
            Ok(fixes) => {
                if !fixes.is_empty() {
                    files_fixed += 1;
                    
                    // Group fixes by file (should all be same file)
                    if !dry_run {
                        // Apply all fixes to this file
                        let mut source = fs::read_to_string(file)?;
                        
                        // Sort fixes by byte_offset in reverse order so we can apply from end to start
                        let mut sorted_fixes = fixes.clone();
                        sorted_fixes.sort_by_key(|f| std::cmp::Reverse(f.byte_offset));
                        
                        for fix in &sorted_fixes {
                            // Determine comment prefix and indentation
                            let lines: Vec<&str> = source.lines().collect();
                            let line_idx = fix.line - 1;
                            let comment_prefix = if line_idx < lines.len() 
                                && lines[line_idx].trim_start().starts_with("//!") {
                                "//! "
                            } else {
                                "/// "
                            };
                            
                            let indent = if line_idx < lines.len() {
                                lines[line_idx]
                                    .chars()
                                    .take_while(|c| c.is_whitespace())
                                    .collect::<String>()
                            } else {
                                String::new()
                            };
                            
                            source = apply_fix_with_offsets(
                                &source,
                                fix.byte_offset,
                                &fix.suggested_imports,
                                comment_prefix,
                                &indent,
                            );
                        }
                        
                        fs::write(file, source)?;
                    }
                    
                    // Print fixes
                    for fix in &fixes {
                        total_fixes += 1;
                        println!("{}:{}: Fixed doctest", rel_path.display(), fix.line);
                        println!("  Added imports:");
                        for import in &fix.suggested_imports {
                            println!("    {}", import);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error processing {}: {}", rel_path.display(), e);
            }
        }
    }
    
    println!();
    if total_fixes > 0 {
        println!(
            "✓ Fixed {} doctest(s) in {} file(s) out of {} checked",
            format_number(total_fixes),
            format_number(files_fixed),
            format_number(files.len())
        );
    } else {
        println!("✓ No doctest issues found in {} file(s)", format_number(files.len()));
    }
    println!("Completed in {}ms", start.elapsed().as_millis());
    
    Ok(())
}
