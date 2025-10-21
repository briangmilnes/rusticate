// Copyright (C) Brian G. Milnes 2025

//! Fix: Convert specific function/type imports to wildcard imports
//! 
//! Transforms imports like:
//!   use Module::Module::function_name;
//! into:
//!   use Module::Module::*;
//! 
//! This is useful after refactoring when functions become trait methods,
//! allowing the module to control what's public via the wildcard import.
//! 
//! Binary: rusticate-fix-our-uses-to-wildcards

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, Edition, SourceFile, SyntaxKind};
use std::fs;
use std::time::Instant;
use rusticate::{find_rust_files, StandardArgs};

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;

    let base_dir = args.base_dir();
    println!("Entering directory '{}'", base_dir.display());
    println!();

    let files = find_rust_files(&args.paths);

    let mut fixed_count = 0;
    for file_path in files {
        let source = fs::read_to_string(&file_path)?;
        let new_source = fix_uses_to_wildcards(&source)?;

        if new_source != source {
            fs::write(&file_path, &new_source)?;
            println!("{}:1: Fixed imports to wildcards", file_path.display());
            fixed_count += 1;
        }
    }

    println!();
    let file_word = if fixed_count == 1 { "file" } else { "files" };
    println!("Fixed {} {}", fixed_count, file_word);
    println!("Completed in {}ms", start.elapsed().as_millis());

    Ok(())
}

/// Replace `use Module::Module::item;` with `use Module::Module::*;` using AST
fn fix_uses_to_wildcards(source: &str) -> Result<String> {
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Ok(source.to_string()); // If parse fails, return unchanged
    }

    let tree = parsed.tree();
    let root = tree.syntax();

    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    // Find all USE items
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                if let Some(use_tree) = use_item.use_tree() {
                    let use_text = use_tree.to_string();

                    // Skip if already a wildcard import
                    if use_text.ends_with("::*") {
                        continue;
                    }

                    // Check if this matches Module::Module::something pattern
                    if let Some(path) = use_tree.path() {
                        let segments: Vec<_> = path.segments().map(|s| s.to_string()).collect();

                        // Find where a name appears twice in sequence (e.g., DFSStEph::DFSStEph)
                        let mut module_path_parts = Vec::new();
                        let mut found_double = false;
                        for (i, segment) in segments.iter().enumerate() {
                            module_path_parts.push(segment.clone());
                            if i > 0 && segments[i - 1] == *segment {
                                found_double = true;
                                // Check if there are more segments after the double (i.e., it's not just Module::Module)
                                if i + 1 < segments.len() {
                                    // Build new import: path::to::Module::Module::*
                                    let new_import = format!("{}::*", module_path_parts.join("::"));

                                    // Replace the entire use_tree
                                    let start: usize = use_tree.syntax().text_range().start().into();
                                    let end: usize = use_tree.syntax().text_range().end().into();
                                    replacements.push((start, end, new_import));
                                }
                                break;
                            }
                        }

                        // If no double found, check for single module name (rare case)
                        if !found_double && segments.len() > 1 {
                            // Skip - only convert Module::Module::item patterns
                        }
                    }
                }
            }
        }
    }

    // Apply replacements from end to start
    let mut result = source.to_string();
    replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));

    for (start, end, new_text) in replacements {
        result.replace_range(start..end, &new_text);
    }

    Ok(result)
}

