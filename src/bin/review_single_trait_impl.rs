// Copyright (C) Brian G. Milnes 2025

//! Review: Single trait implementation per struct (MANDATORY)
//! 
//! Replaces: scripts/rust/src/review_single_trait_impl.py
//! Rule: Each trait should have only ONE impl block per struct
//! Git commit: 584a672b6a34782766863c5f76a461d3297a741a
//! Binary: rusticate-review-single-trait-impl
//!
//! Uses AST parsing to find multiple impl blocks for the same trait+struct combination

use anyhow::Result;
use rusticate::{StandardArgs, format_number, parse_source, find_nodes, find_rust_files};
use ra_ap_syntax::{SyntaxKind, ast::{self, AstNode}};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraitImplKey {
    trait_name: String,
    struct_name: String,
}

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    trait_name: String,
    struct_name: String,
    lines: Vec<usize>,
}

fn is_standard_trait(name: &str) -> bool {
    matches!(name,
        "Debug" | "Clone" | "Copy" | "PartialEq" | "Eq" | 
        "PartialOrd" | "Ord" | "Hash" | "Display" | "Default" |
        "From" | "Into" | "AsRef" | "AsMut" | "Deref" | "DerefMut" |
        "Drop" | "Iterator" | "IntoIterator" | "Send" | "Sync" |
        "Sized" | "Unpin"
    )
}

fn check_file(file_path: &Path, source: &str) -> Result<Vec<Violation>> {
    let source_file = parse_source(source)?;
    let root = source_file.syntax();
    
    // Find all IMPL nodes
    let impl_nodes = find_nodes(root, SyntaxKind::IMPL);
    
    // Track: (trait_name, struct_name) -> [line_numbers]
    let mut trait_impls: HashMap<TraitImplKey, Vec<usize>> = HashMap::new();
    
    for impl_node in impl_nodes {
        // Try to parse as Impl AST node
        if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
            // Check if this is a trait impl (has trait_() method)
            if let Some(trait_type) = impl_ast.trait_() {
                // Get trait name from the Type
                // Cast to PathType to access path() method
                let trait_text = trait_type.syntax().text().to_string();
                
                // Extract trait name (first identifier)
                let trait_name = trait_text
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .find(|s| !s.is_empty())
                    .unwrap_or(&trait_text)
                    .to_string();
                
                // Skip standard traits
                if is_standard_trait(&trait_name) {
                    continue;
                }
                
                // Get struct name (self_ty)
                if let Some(self_ty) = impl_ast.self_ty() {
                    // Extract type name
                    let type_text = self_ty.syntax().text().to_string();
                    // Simple extraction: get first identifier
                    let struct_name = type_text
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .find(|s| !s.is_empty())
                        .unwrap_or(&type_text)
                        .to_string();
                    
                    // Get line number (need to call .syntax() on impl_ast)
                    let line_num = rusticate::line_number(impl_ast.syntax(), source);
                    
                    let key = TraitImplKey {
                        trait_name: trait_name.clone(),
                        struct_name: struct_name.clone(),
                    };
                    
                    trait_impls.entry(key).or_default().push(line_num);
                }
            }
        }
    }
    
    // Find violations (multiple impls for same trait+struct)
    let mut violations = Vec::new();
    for (key, lines) in trait_impls {
        if lines.len() > 1 {
            violations.push(Violation {
                file: file_path.to_path_buf(),
                trait_name: key.trait_name,
                struct_name: key.struct_name,
                lines,
            });
        }
    }
    
    Ok(violations)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    // Print compilation directory for Emacs compile-mode
    println!("Entering directory '{}'", base_dir.display());
    println!();
    
    let search_dirs = args.get_search_dirs();
    
    // This rule only applies to src/ (not tests/ or benches/)
    let src_dirs: Vec<_> = search_dirs.iter()
        .filter(|p| p.is_dir() && (p.ends_with("src") || p.components().any(|c| c.as_os_str() == "src")))
        .cloned()
        .collect();
    
    if src_dirs.is_empty() {
        println!("✓ No src/ directories to check");
        let elapsed = start.elapsed().as_millis();
        println!("Completed in {}ms", elapsed);
        return Ok(());
    }
    
    let files = find_rust_files(&src_dirs);
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
        println!("✓ All traits have single implementations!");
    } else {
        println!("✗ Multiple Trait Implementations: {} violation(s)", format_number(all_violations.len()));
        println!();
        println!("Each trait should have only ONE impl block for each struct.");
        println!();
        
        for v in &all_violations {
            if let Ok(rel_path) = v.file.strip_prefix(&base_dir) {
                println!("{}:{}: Multiple impl blocks for trait '{}' on struct '{}'", 
                         rel_path.display(), v.lines[0], v.trait_name, v.struct_name);
                println!("  Found at lines: {}", v.lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", "));
            }
        }
    }
    
    // Summary
    println!();
    println!("Summary: {} files checked, {} violations", 
             format_number(files.len()), format_number(all_violations.len()));
    
    let elapsed = start.elapsed().as_millis();
    println!("Completed in {}ms", elapsed);
    
    if all_violations.is_empty() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

