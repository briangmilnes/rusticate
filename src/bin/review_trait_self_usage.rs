//! Review: Trait Self Usage
//!
//! Detects trait methods that return concrete types instead of Self.
//!
//! Example violation:
//!     pub trait SetTrait<T> {
//!         fn empty() -> Set<T>;  // Should be: -> Self
//!     }
//!
//! Binary: rusticate-review-trait-self-usage

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasName}, SyntaxKind, SourceFile, Edition};
use rusticate::{StandardArgs, find_rust_files};
use std::path::Path;
use std::time::Instant;

fn main() -> Result<()> {
    let start_time = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();

    let files = find_rust_files(&args.paths);

    println!("Reviewing {} Rust files for trait Self usage...", files.len());

    let mut all_violations = 0;
    let mut files_with_violations = 0;

    for file_path in &files {
        let violations = check_file(file_path);
        if violations > 0 {
            files_with_violations += 1;
            all_violations += violations;
        }
    }

    if all_violations == 0 {
        println!("✓ Trait Self Usage: No violations found");
        let elapsed = start_time.elapsed();
        eprintln!("\nCompleted in {}ms", elapsed.as_millis());
        return Ok(());
    }

    println!("\n✗ Found {} violation(s) in {} file(s)", all_violations, files_with_violations);
    println!("\nNote: Trait methods should return Self, &Self, or &mut Self instead of concrete types");

    let elapsed = start_time.elapsed();
    eprintln!("\nCompleted in {}ms", elapsed.as_millis());

    Ok(())
}

fn check_file(file_path: &Path) -> usize {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();

    let mut violations = 0;

    // Look for trait definitions
    for node in root.descendants() {
        if node.kind() == SyntaxKind::TRAIT {
            if let Some(trait_node) = ast::Trait::cast(node) {
                let trait_name = if let Some(name) = trait_node.name() {
                    name.to_string()
                } else {
                    continue;
                };

                // Extract base name (remove "Trait" suffix if present)
                let base_name = if trait_name.ends_with("Trait") {
                    &trait_name[..trait_name.len() - 5]
                } else {
                    &trait_name
                };

                // Check methods in trait
                if let Some(assoc_list) = trait_node.assoc_item_list() {
                    for item in assoc_list.assoc_items() {
                        if let ast::AssocItem::Fn(func) = item {
                            if let Some(ret_type) = func.ret_type() {
                                let ret_str = ret_type.to_string().replace("->", "").trim().to_string();
                                
                                // Check if return type mentions the base type name but is not Self
                                if ret_str.contains(base_name) && !ret_str.contains("Self") {
                                    violations += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    violations
}

