//! Review: Impl Trait Bounds
//!
//! Shows inherent impl blocks with their trait bounds for comparison.
//! Helps identify bound mismatches or inconsistencies.
//!
//! Binary: rusticate-review-impl-trait-bounds

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasName}, SyntaxKind, SourceFile, Edition};
use rusticate::{StandardArgs, find_rust_files};
use std::path::Path;
use std::time::Instant;

fn main() -> Result<()> {
    let start_time = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();

    // Only check src/ files
    let src_files: Vec<_> = find_rust_files(&args.paths)
        .into_iter()
        .filter(|p| p.starts_with(base_dir.join("src")))
        .collect();

    println!("INHERENT IMPL BLOCKS WITH TRAIT BOUNDS COMPARISON");
    println!("{}", "=".repeat(80));
    println!();

    let mut total_impls = 0;
    let mut with_traits = 0;
    let mut without_traits = 0;

    for file_path in &src_files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let parsed = SourceFile::parse(&content, Edition::Edition2021);
        let tree = parsed.tree();
        let root = tree.syntax();

        for node in root.descendants() {
            if node.kind() == SyntaxKind::IMPL {
                if let Some(impl_node) = ast::Impl::cast(node.clone()) {
                    // Only check inherent impls (no trait)
                    if impl_node.trait_().is_some() {
                        continue;
                    }

                    // Only check those with generics (check string representation)
                    let impl_str = impl_node.to_string();
                    let first_line = impl_str.lines().next().unwrap_or("");
                    if !first_line.contains('<') || !first_line.contains('>') {
                        continue;
                    }

                    total_impls += 1;

                    let line_num = content[..node.text_range().start().into()]
                        .chars()
                        .filter(|&c| c == '\n')
                        .count() + 1;

                    let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
                    
                    // Try to find corresponding trait
                    let type_name = if let Some(self_ty) = impl_node.self_ty() {
                        let type_str = self_ty.to_string();
                        if let Some(pos) = type_str.find('<') {
                            type_str[..pos].trim().to_string()
                        } else {
                            type_str.trim().to_string()
                        }
                    } else {
                        "unknown".to_string()
                    };

                    // Look for trait definition in same file
                    let trait_name = format!("{}Trait", type_name);
                    let has_trait = content.contains(&format!("pub trait {}", trait_name));

                    if has_trait {
                        with_traits += 1;
                    } else {
                        without_traits += 1;
                    }

                    println!("{}:{}", rel_path.display(), line_num);
                    println!("  Type: {}", type_name);
                    println!("  Has trait: {}", if has_trait { "YES" } else { "NO" });
                    println!("  Inherent impl: {}", first_line);
                    println!();
                }
            }
        }
    }

    println!("{}", "=".repeat(80));
    println!("SUMMARY");
    println!("{}", "=".repeat(80));
    println!("Total impl blocks with generics: {}", total_impls);
    println!("  With trait definitions: {}", with_traits);
    println!("  Without trait definitions: {}", without_traits);

    let elapsed = start_time.elapsed();
    eprintln!("\nCompleted in {}ms", elapsed.as_millis());

    Ok(())
}

