// Copyright (C) Brian G. Milnes 2025

//! Fix import ordering and blank lines
//! 
//! Replaces: scripts/rust/fix_import_order.py
//! Rule: RustRules.md Lines 50, 75-86
//! 
//! Correct order:
//! 1. std/core/alloc imports
//! 2. blank line
//! 3. external crate imports
//! 4. blank line
//! 5. internal imports (crate::Types::Types::* first, then others alphabetically)
//!    - NO blank line between Types::Types and other internal imports

use anyhow::Result;
use ra_ap_syntax::{SyntaxKind, SyntaxNode, SourceFile, Edition, ast::AstNode};
use std::fs;
use std::time::Instant;
use rusticate::{StandardArgs, find_rust_files};


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/fix_import_order.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ImportSection {
    Std,
    External,
    Internal,       // Internal imports (Types::Types sorted first)
}

#[derive(Debug)]
struct Import {
    section: ImportSection,
    text: String,
    is_types: bool,  // True if this is Types::Types::*
}

fn classify_import(use_text: &str) -> (ImportSection, bool) {
    // Extract path after "use " and before trailing semicolon
    let trimmed = use_text.trim();
    let path = if let Some(start_idx) = trimmed.find("use ") {
        &trimmed[start_idx + 4..]
    } else {
        trimmed
    };
    let path = path.trim().trim_end_matches(';').trim();
    
    let is_types = path.contains("Types::Types");
    
    if path.starts_with("std::") || path.starts_with("core::") || path.starts_with("alloc::") {
        (ImportSection::Std, false)
    } else if path.starts_with("crate::") || path.starts_with("apas_ai::") {
        (ImportSection::Internal, is_types)
    } else if path.starts_with("self::") || path.starts_with("super::") {
        (ImportSection::Internal, false)
    } else {
        (ImportSection::External, false)
    }
}

fn fix_imports(source: &str) -> Result<String> {
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors: {:?}", parsed.errors()));
    }
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find ONLY top-level module USE nodes (not inside fn/impl/trait blocks)
    // Structure: USE -> ITEM_LIST -> (MODULE or SOURCE_FILE)
    // We need to check the immediate parent chain, not just any ancestor
    let use_nodes: Vec<_> = root.descendants()
        .filter(|node| {
            if node.kind() != SyntaxKind::USE {
                return false;
            }
            
            // Get parent (should be ITEM_LIST for top-level items)
            let parent = match node.parent() {
                Some(p) => p,
                None => return false,
            };
            
            // Parent must be ITEM_LIST
            if parent.kind() != SyntaxKind::ITEM_LIST {
                return false;
            }
            
            // Grandparent must be MODULE or SOURCE_FILE (not IMPL, FN, TRAIT, etc.)
            let grandparent = match parent.parent() {
                Some(gp) => gp,
                None => return false,
            };
            
            matches!(grandparent.kind(), SyntaxKind::MODULE | SyntaxKind::SOURCE_FILE)
        })
        .collect();
    
    if use_nodes.is_empty() {
        return Ok(source.to_string());
    }
    
    // Group USE nodes into contiguous blocks
    // A block is contiguous if USE statements are consecutive in source with no other items between
    let mut use_blocks: Vec<Vec<&SyntaxNode>> = Vec::new();
    let mut current_block = vec![&use_nodes[0]];
    
    for i in 1..use_nodes.len() {
        let prev_end: usize = use_nodes[i-1].text_range().end().into();
        let curr_start: usize = use_nodes[i].text_range().start().into();
        
        // Check if there's only whitespace/comments between prev and current
        let between = &source[prev_end..curr_start];
        let is_contiguous = between.trim().is_empty();
        
        if is_contiguous {
            current_block.push(&use_nodes[i]);
        } else {
            // Start new block
            use_blocks.push(current_block);
            current_block = vec![&use_nodes[i]];
        }
    }
    use_blocks.push(current_block);
    
    // Fix each block separately and collect all edits
    // We process blocks from END to START so byte offsets remain valid
    let mut result = source.to_string();
    
    for block in use_blocks.iter().rev() {
        result = fix_import_block(&result, block)?;
    }
    
    Ok(result)
}

fn fix_import_block(source: &str, use_nodes: &[&SyntaxNode]) -> Result<String> {
    if use_nodes.is_empty() {
        return Ok(source.to_string());
    }
    
    let first_use_node = use_nodes[0];
    let last_use_node = use_nodes[use_nodes.len() - 1];
    
    let first_use_start: usize = first_use_node.text_range().start().into();
    let last_use_end: usize = last_use_node.text_range().end().into();
    
    // Get line boundaries for this import block
    let import_block_line_start = source[..first_use_start]
        .rfind('\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    
    // Find the newline after the last USE (or EOF)
    let import_block_line_end = if last_use_end < source.len() {
        source[last_use_end..]
            .find('\n')
            .map(|p| last_use_end + p)
            .unwrap_or(source.len())
    } else {
        source.len()
    };
    
    // Extract indentation
    let indent = &source[import_block_line_start..first_use_start];
    let before_imports = &source[..import_block_line_start];
    
    // Check for pub mod before this block
    let has_pub_mod = before_imports.lines()
        .rev()
        .take(5)
        .any(|line| line.contains("pub mod") && line.contains('{'));
    
    // Extract and classify all imports in this block
    let mut imports = Vec::new();
    for use_node in use_nodes {
        let use_text = use_node.text().to_string();
        let (section, is_types) = classify_import(&use_text);
        
        imports.push(Import {
            section,
            text: use_text,
            is_types,
        });
    }
    
    // Sort imports
    imports.sort_by(|a, b| {
        match a.section.cmp(&b.section) {
            std::cmp::Ordering::Equal => {
                match (&a.section, a.is_types, b.is_types) {
                    (ImportSection::Internal, true, false) => std::cmp::Ordering::Less,
                    (ImportSection::Internal, false, true) => std::cmp::Ordering::Greater,
                    _ => a.text.cmp(&b.text),
                }
            }
            other => other,
        }
    });
    
    // Build replacement text
    let mut fixed_imports = String::new();
    
    // Add blank line after pub mod if this is the first block after it
    if has_pub_mod && !before_imports.trim_end().ends_with('{') {
        fixed_imports.push('\n');
    }
    
    let mut prev_section: Option<ImportSection> = None;
    for import in &imports {
        // Blank line between sections
        if let Some(ref prev) = prev_section {
            if prev != &import.section {
                fixed_imports.push('\n');
            }
        }
        
        fixed_imports.push_str(indent);
        fixed_imports.push_str(&import.text);
        fixed_imports.push('\n');
        prev_section = Some(import.section.clone());
    }
    
    // Preserve trailing newlines after imports
    let trailing_text = &source[import_block_line_end..import_block_line_end.min(import_block_line_end + 2)];
    let num_trailing_newlines = trailing_text.chars().filter(|&c| c == '\n').count();
    
    // Add one blank line after this block if none
    if num_trailing_newlines < 2 {
        fixed_imports.push('\n');
    }
    
    // Reconstruct source with fixed block
    let mut result = String::new();
    result.push_str(&source[..import_block_line_start]);
    result.push_str(&fixed_imports);
    let skip_to = import_block_line_end + num_trailing_newlines;
    result.push_str(&source[skip_to..]);
    
    Ok(result)
}

fn main() -> Result<()> {
    let start = Instant::now();
    
    // Parse dry-run flag manually
    let all_args: Vec<String> = std::env::args().collect();
    let dry_run = all_args.iter().any(|a| a == "--dry-run");
    
    // Parse standard args, filtering out tool-specific flags
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    // Print compilation directory for Emacs compile-mode
    log!("Entering directory '{}'", base_dir.display());
    log!("");
    
    if dry_run {
        log!("DRY RUN MODE: Will not modify files");
        log!("");
    }
    
    let search_dirs = args.get_search_dirs();
    let files = find_rust_files(&search_dirs);
    
    let mut fixed_count = 0;
    let mut already_correct = 0;
    let mut failed_count = 0;
    
    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                if !dry_run {
                    eprintln!("Warning: Failed to read {}: {}", file.display(), e);
                }
                failed_count += 1;
                continue;
            }
        };
        
        let fixed = match fix_imports(&source) {
            Ok(f) => f,
            Err(e) => {
                if !dry_run {
                    eprintln!("Warning: Failed to parse {}: {}", file.display(), e);
                }
                failed_count += 1;
                continue;
            }
        };
        
        if source == fixed {
            already_correct += 1;
        } else {
            // Show what changed - extract import sections for before/after
            let rel_path = file.strip_prefix(&base_dir).unwrap_or(file);
            
            // Find first import line number
            let first_import_line = source.lines()
                .enumerate()
                .find(|(_, line)| line.trim().starts_with("use "))
                .map(|(idx, _)| idx + 1)
                .unwrap_or(1);
            
            log!("{}:{}: Fixed import order", rel_path.display(), first_import_line);
            
            // Show before/after of import section (just the use statements)
            let source_imports: Vec<&str> = source.lines()
                .filter(|line| line.trim().starts_with("use "))
                .take(10) // Limit to first 10 use statements
                .collect();
            
            let fixed_imports: Vec<&str> = fixed.lines()
                .filter(|line| line.trim().starts_with("use "))
                .take(10)
                .collect();
            
            if !source_imports.is_empty() {
                log!("  Before:");
                for line in source_imports.iter().take(5) {
                    log!("    {}", line);
                }
                if source_imports.len() > 5 {
                    log!("    ... ({} more lines)", source_imports.len() - 5);
                }
            }
            
            if !fixed_imports.is_empty() {
                log!("  After:");
                for line in fixed_imports.iter().take(5) {
                    log!("    {}", line);
                }
                if fixed_imports.len() > 5 {
                    log!("    ... ({} more lines)", fixed_imports.len() - 5);
                }
            }
            log!("");
            
            if !dry_run {
                match fs::write(file, &fixed) {
                    Ok(()) => {
                        fixed_count += 1;
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to write {}: {}", file.display(), e);
                        failed_count += 1;
                    }
                }
            } else {
                fixed_count += 1;
            }
        }
    }
    
    // Summary
    if dry_run {
        log!("Would fix {} file(s), {} already correct", fixed_count, already_correct);
    } else {
        log!("✓ Fixed {} file(s), {} already correct", fixed_count, already_correct);
    }
    
    if failed_count > 0 {
        log!("✗ Failed to process {} file(s)", failed_count);
    }
    
    let elapsed = start.elapsed().as_millis();
    log!("Completed in {}ms", elapsed);
    
    Ok(())
}

