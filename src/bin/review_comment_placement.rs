// Copyright (C) Brian G. Milnes 2025

//! Review: Comment placement inside proper blocks
//! 
//! Checks that comments are properly placed inside pub mod, trait, impl, or function blocks
//! Binary: rusticate-review-comment-placement
//!
//! Uses AST parsing to verify comment positions

use anyhow::Result;
use ra_ap_syntax::{ast, ast::HasVisibility, AstNode, NodeOrToken, SourceFile, SyntaxKind, SyntaxNode, SyntaxToken};
use rusticate::{StandardArgs, find_rust_files};
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
            .open("analyses/review_comment_placement.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct CommentIssue {
    file: PathBuf,
    line: usize,
    comment_text: String,
    issue_type: String,
}

/// Find the pub mod block in the file
fn find_module_block(root: &ra_ap_syntax::SyntaxNode) -> Option<(usize, usize)> {
    for node in root.descendants() {
        if node.kind() == SyntaxKind::MODULE {
            if let Some(module) = ast::Module::cast(node.clone()) {
                // Check if it's pub
                if module.visibility().is_some() {
                    // Found pub mod - get its range
                    let start = node.text_range().start().into();
                    let end = node.text_range().end().into();
                    return Some((start, end));
                }
            }
        }
    }
    None
}

fn check_file(file_path: &Path) -> Result<Vec<CommentIssue>> {
    let mut issues = Vec::new();
    let source = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&source, ra_ap_syntax::Edition::Edition2021);

    if !parsed.errors().is_empty() {
        return Ok(issues); // Skip files with parse errors
    }

    let tree = parsed.tree();
    let root = tree.syntax();

    // Skip the "outside pub mod block" check for lib.rs and main.rs
    // since they have different structures (multiple pub mods or no wrapping mod)
    let is_crate_root = file_path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n == "lib.rs" || n == "main.rs")
        .unwrap_or(false);

    // Find the pub mod block (if any)
    let module_range = if is_crate_root {
        None
    } else {
        find_module_block(root)
    };

    // Traverse all tokens looking for comments
    for token in root.descendants_with_tokens().filter_map(|e| e.into_token()) {
        if token.kind() == SyntaxKind::COMMENT {
            let comment_text = token.text().to_string();
            let byte_offset: usize = token.text_range().start().into();
            let line = get_line_number(&source, byte_offset);

            // Allow copyright comments at the top
            if comment_text.contains("Copyright") {
                continue;
            }

            // Allow inline comments (comments with code before them on the same line)
            if is_inline_comment(&source, byte_offset) {
                continue;
            }

            // Allow doc comments before pub mod (they document the module)
            let is_doc_comment = comment_text.starts_with("//!") || comment_text.starts_with("///") || 
                                 comment_text.starts_with("/*!") || comment_text.starts_with("/**");

            // Check if comment is outside pub mod block
            if let Some((mod_start, mod_end)) = module_range {
                if byte_offset < mod_start {
                    // Before the pub mod - only allow doc comments
                    if !is_doc_comment {
                        issues.push(CommentIssue {
                            file: file_path.to_path_buf(),
                            line,
                            comment_text: comment_text.trim().to_string(),
                            issue_type: "comment outside pub mod block".to_string(),
                        });
                    }
                    continue;
                } else if byte_offset > mod_end {
                    // After the pub mod - not allowed
                    issues.push(CommentIssue {
                        file: file_path.to_path_buf(),
                        line,
                        comment_text: comment_text.trim().to_string(),
                        issue_type: "comment outside pub mod block".to_string(),
                    });
                    continue;
                }
            }
            
            // Check if comment is properly placed within trait/impl/fn
            if !is_comment_in_proper_context(&token) {
                issues.push(CommentIssue {
                    file: file_path.to_path_buf(),
                    line,
                    comment_text: comment_text.trim().to_string(),
                    issue_type: "comment not in proper context".to_string(),
                });
            }
        }
    }

    Ok(issues)
}

fn get_line_number(source: &str, byte_offset: usize) -> usize {
    source[..byte_offset].lines().count()
}

fn is_inline_comment(source: &str, byte_offset: usize) -> bool {
    // Check if there's code before the comment on the same line
    let before = &source[..byte_offset];
    if let Some(line_start) = before.rfind('\n') {
        let line_before_comment = &before[line_start + 1..];
        // If there's non-whitespace content before the comment, it's inline
        line_before_comment.chars().any(|c| !c.is_whitespace())
    } else {
        // First line - check if there's anything before the comment
        before.chars().any(|c| !c.is_whitespace())
    }
}

fn is_first_item_in_block(token: &SyntaxToken, block_node: &SyntaxNode) -> bool {
    // Check if this comment is the first meaningful item INSIDE the block
    // (after the opening brace, ignoring whitespace)
    let mut found_opening_brace = false;
    let mut found_any_item = false;
    
    for child in block_node.children_with_tokens() {
        match child {
            | NodeOrToken::Token(t) => {
                // Track if we've passed the opening brace
                if t.kind() == SyntaxKind::L_CURLY {
                    found_opening_brace = true;
                    continue;
                }
                
                // Only check after the opening brace
                if !found_opening_brace {
                    continue;
                }
                
                // Skip whitespace
                if t.kind() == SyntaxKind::WHITESPACE {
                    continue;
                }
                
                // If this is our comment and we haven't seen any items yet, it's first
                if t.text_range() == token.text_range() && !found_any_item {
                    return true;
                }
                
                // Any other token means we're past the first position
                found_any_item = true;
            }
            | NodeOrToken::Node(n) => {
                // Only check nodes after the opening brace
                if !found_opening_brace {
                    continue;
                }
                
                // If this node contains our comment and we haven't seen any items yet,
                // the comment is the first item (even if it's inside this node)
                if n.text_range().contains_range(token.text_range()) && !found_any_item {
                    return true;
                }
                
                // Found a node, so we're past the first position
                found_any_item = true;
            }
        }
    }
    false
}

fn is_comment_in_proper_context(token: &SyntaxToken) -> bool {
    let text = token.text();
    
    // Check if this is an outer doc comment (/// or /**) - documents following item
    let is_outer_doc = text.starts_with("///") || text.starts_with("/**");
    // Check if this is an inner doc comment (//! or /*!) - documents containing item
    let is_inner_doc = text.starts_with("//!") || text.starts_with("/*!");
    
    // Check if we're inside ASSOC_ITEM_LIST (trait/impl) or STMT_LIST (function body)
    let mut parent = token.parent();
    let mut found_specific_block = false;
    
    while let Some(node) = parent {
        match node.kind() {
            // ASSOC_ITEM_LIST holds trait/impl items
            SyntaxKind::ASSOC_ITEM_LIST => {
                // Outer doc comments (///) are OK - they document the following function/method
                // Inner doc comments (//!) should be before the trait/impl, not inside
                // Regular comments as first line should be before the trait/impl
                if is_first_item_in_block(token, &node) {
                    return is_outer_doc;
                }
                found_specific_block = true;
                return true;
            }
            // STMT_LIST holds function body statements
            SyntaxKind::STMT_LIST => {
                // Check if this STMT_LIST is directly inside a FN (function body)
                // vs inside a nested block (if, match, etc.)
                let is_function_body = node.parent()
                    .map(|p| p.kind() == SyntaxKind::BLOCK_EXPR)
                    .unwrap_or(false)
                    && node.parent()
                        .and_then(|p| p.parent())
                        .map(|pp| pp.kind() == SyntaxKind::FN)
                        .unwrap_or(false);
                
                if is_function_body && is_first_item_in_block(token, &node) {
                    // Inside function bodies, outer doc comments (///) are OK (document inner items)
                    // Inner doc comments (//!) should be before the function
                    // Regular comments as first line should be before the function
                    return is_outer_doc;
                }
                found_specific_block = true;
                return true;
            }
            SyntaxKind::STRUCT | SyntaxKind::ENUM => {
                return true;
            }
            SyntaxKind::MODULE => {
                // If we haven't found a more specific block, we're in module space
                // All doc comments are OK here (they document items)
                // Regular comments are OK here too (between items)
                if !found_specific_block {
                    return true;
                }
            }
            _ => {}
        }
        parent = node.parent();
    }

    // If we get here, we're somewhere outside module space
    // Outer doc comments are OK (they document following items)
    // Inner doc comments and regular comments are not OK
    is_outer_doc || is_inner_doc
}

fn main() -> Result<()> {
    let args = StandardArgs::parse()?;
    let start = Instant::now();
    
    // Get search directories from args
    let search_dirs = args.get_search_dirs();
    let base_dir = search_dirs.first()
        .and_then(|d| d.parent())
        .unwrap_or(std::path::Path::new("."));
    
    log!("Entering directory '{}'", base_dir.display());
    log!("");
    
    let files = find_rust_files(&search_dirs);
    let mut total_issues = 0;

    for file in &files {
        let issues = check_file(file)?;
        for issue in issues {
            log!(
                "{}:{}:\t{}: {}",
                issue.file.display(),
                issue.line,
                issue.issue_type,
                issue.comment_text
            );
            total_issues += 1;
        }
    }

    if total_issues == 0 {
        log!("No comment placement issues found.");
    } else {
        log!("\nTotal issues: {}", total_issues);
    }
    
    log!("Completed in {}ms", start.elapsed().as_millis());

    Ok(())
}

