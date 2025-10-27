// Copyright (C) Brian G. Milnes 2025

//! Fix: Add dual stdout+file logging to binaries
//!
//! Adds module-level log! macro that writes to both stdout and analyses/<tool>.log
//! Replaces println! calls with log! calls
//!
//! Binary: fix-logging

use anyhow::Result;
use ra_ap_syntax::{ast, AstNode, Edition, SourceFile, SyntaxKind, SyntaxNode, TextRange, TextSize};
use rusticate::{find_rust_files, StandardArgs};
use std::fs;
use std::path::Path;
use std::time::Instant;

fn extract_tool_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn has_logging(content: &str) -> bool {
    content.contains("analyses/") && content.contains(".log")
        || content.contains("macro_rules! log")
}

fn has_std_fs_import(root: &SyntaxNode) -> bool {
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node) {
                if let Some(use_tree) = use_item.use_tree() {
                    if let Some(path) = use_tree.path() {
                        // Check if path is exactly "std::fs" (not std::fs::something)
                        let segments: Vec<_> = path.segments().collect();
                        if segments.len() == 2 {
                            if let (Some(first), Some(second)) = (segments.get(0), segments.get(1)) {
                                if first.to_string() == "std" && second.to_string() == "fs" {
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

fn find_last_use_statement_end(root: &SyntaxNode) -> Option<TextSize> {
    let mut last_use_end = None;
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            last_use_end = Some(node.text_range().end());
        }
    }
    
    last_use_end
}

fn find_first_item_start(root: &SyntaxNode) -> Option<TextSize> {
    for node in root.children() {
        match node.kind() {
            SyntaxKind::FN | SyntaxKind::STRUCT | SyntaxKind::ENUM | SyntaxKind::IMPL | SyntaxKind::TRAIT => {
                return Some(node.text_range().start());
            }
            _ => {}
        }
    }
    None
}

fn build_log_macro(tool_name: &str) -> String {
    format!(r#"
macro_rules! log {{
    ($($arg:tt)*) => {{{{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{{}}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/{}.log")
        {{
            let _ = writeln!(file, "{{}}", msg);
        }}
    }}}};
}}
"#, tool_name)
}

fn should_replace_println(macro_call: &ast::MacroCall) -> bool {
    // Check if this is a println! macro (not eprintln!)
    if let Some(path) = macro_call.path() {
        let path_text = path.syntax().text().to_string();
        path_text == "println"
    } else {
        false
    }
}

fn rewrite_node(node: &SyntaxNode) -> String {
    // Check if this node is a println! macro call that should be replaced
    if node.kind() == SyntaxKind::MACRO_CALL {
        if let Some(macro_call) = ast::MacroCall::cast(node.clone()) {
            if should_replace_println(&macro_call) {
                // Reconstruct the macro call with "log!" instead of "println!"
                let mut result = String::new();
                
                for child in node.children_with_tokens() {
                    match child {
                        rowan::NodeOrToken::Node(child_node) => {
                            // Recursively process child nodes (path, token_tree, etc.)
                            if child_node.kind() == SyntaxKind::PATH {
                                // Replace "println" with "log"
                                result.push_str("log");
                            } else {
                                result.push_str(&rewrite_node(&child_node));
                            }
                        }
                        rowan::NodeOrToken::Token(token) => {
                            // Keep all tokens (!, parens, args, etc.)
                            result.push_str(token.text());
                        }
                    }
                }
                return result;
            }
        }
    }
    
    // Otherwise, recursively process children
    let mut result = String::new();
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(child_node) => {
                result.push_str(&rewrite_node(&child_node));
            }
            rowan::NodeOrToken::Token(token) => {
                result.push_str(token.text());
            }
        }
    }
    result
}

fn apply_replacements(_content: &str, tool_name: &str, root: &SyntaxNode) -> String {
    // First, rewrite the AST to replace println! with log!
    let rewritten = rewrite_node(root);
    
    // Now apply insertions to the rewritten content
    // Parse the rewritten content to get fresh AST positions
    let parsed = SourceFile::parse(&rewritten, Edition::Edition2021);
    let tree = parsed.tree();
    let new_root = tree.syntax();
    
    let macro_code = build_log_macro(tool_name);
    let macro_insert_pos = if let Some(first_item) = find_first_item_start(new_root) {
        usize::from(first_item)
    } else if let Some(last_use_end) = find_last_use_statement_end(new_root) {
        usize::from(last_use_end)
    } else {
        0
    };
    
    let std_fs_insert = if !has_std_fs_import(new_root) {
        find_last_use_statement_end(new_root).map(|pos| usize::from(pos))
    } else {
        None
    };
    
    // Build list of (position, text) insertions and sort by position (descending)
    let mut insertions = vec![(macro_insert_pos, macro_code)];
    if let Some(pos) = std_fs_insert {
        insertions.push((pos, "\nuse std::fs;".to_string()));
    }
    insertions.sort_by_key(|(pos, _)| std::cmp::Reverse(*pos));
    
    // Apply insertions in descending order so earlier positions stay valid
    let mut result = rewritten;
    for (pos, text) in insertions {
        result.insert_str(pos, &text);
    }
    
    result
}

fn fix_file(path: &Path) -> Result<bool> {
    let content = fs::read_to_string(path)?;
    
    if has_logging(&content) {
        return Ok(false);
    }
    
    let tool_name = extract_tool_name(path);
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let new_content = apply_replacements(&content, &tool_name, root);
    
    if new_content != content {
        fs::write(path, &new_content)?;
        return Ok(true);
    }
    
    Ok(false)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    
    // Get files from the provided paths
    let files = find_rust_files(args.paths());
    
    let mut modified_files = Vec::new();
    let mut skipped_files = Vec::new();
    
    for file in &files {
        match fix_file(file) {
            Ok(true) => {
                modified_files.push(file.clone());
                println!("✓ Added logging to: {}", file.display());
            }
            Ok(false) => {
                skipped_files.push(file.clone());
            }
            Err(e) => {
                eprintln!("✗ Error processing {}: {}", file.display(), e);
            }
        }
    }
    
    println!("\n{}", "=".repeat(80));
    println!("Files processed: {}", files.len());
    println!("Files modified: {}", modified_files.len());
    println!("Files skipped (already have logging): {}", skipped_files.len());
    println!("Completed in {:?}", start.elapsed());
    println!("{}", "=".repeat(80));
    
    Ok(())
}

