//! Review: Detect redundant inherent impls
//!
//! After inlining delegating trait methods, some files have both:
//! 1. An inherent impl with methods
//! 2. A trait impl with actual implementations (not delegation)
//!
//! The inherent impl is now redundant and should be removed.
//!
//! Binary: rusticate-review-redundant-inherent-impls

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasVisibility, HasName}, SyntaxKind, SourceFile, Edition};
use rusticate::{StandardArgs, find_rust_files};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone)]
struct FileInfo {
    path: PathBuf,
    struct_name: Option<String>,
    has_inherent_impl: bool,
    has_trait_impl: bool,
}

fn analyze_file(file_path: &PathBuf) -> Option<FileInfo> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();

    // Find the main struct name
    let mut struct_name = None;
    for node in root.descendants() {
        if node.kind() == SyntaxKind::STRUCT {
            if let Some(struct_node) = ast::Struct::cast(node.clone()) {
                if let Some(vis) = struct_node.visibility() {
                    if vis.to_string().contains("pub") {
                        if let Some(name) = struct_node.name() {
                            struct_name = Some(name.to_string());
                            break;
                        }
                    }
                }
            }
        }
    }

    if struct_name.is_none() {
        return None;
    }

    let target_struct = struct_name.as_ref().unwrap().clone();

    // Check for inherent impl
    let mut has_inherent_impl = false;
    let mut has_trait_impl = false;

    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            if let Some(impl_node) = ast::Impl::cast(node) {
                // Check if this is an impl for our target struct
                if let Some(self_ty) = impl_node.self_ty() {
                    let self_ty_str = self_ty.to_string();
                    if !self_ty_str.contains(&target_struct) {
                        continue;
                    }

                    // Check if it has methods
                    if let Some(assoc_list) = impl_node.assoc_item_list() {
                        let has_methods = assoc_list
                            .assoc_items()
                            .any(|item| matches!(item, ast::AssocItem::Fn(_)));

                        if !has_methods {
                            continue;
                        }

                        // Inherent impl: no trait_() present
                        if impl_node.trait_().is_none() {
                            has_inherent_impl = true;
                        } else {
                            // Trait impl: has trait_() present
                            has_trait_impl = true;
                        }
                    }
                }
            }
        }
    }

    Some(FileInfo {
        path: file_path.clone(),
        struct_name,
        has_inherent_impl,
        has_trait_impl,
    })
}

fn main() -> Result<()> {
    let start_time = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();

    let files = find_rust_files(&args.paths);

    let mut redundant_files = Vec::new();

    for file_path in &files {
        if let Some(info) = analyze_file(file_path) {
            if info.has_inherent_impl && info.has_trait_impl {
                redundant_files.push(info);
            }
        }
    }

    if redundant_files.is_empty() {
        println!("No redundant inherent impls found.");
        return Ok(());
    }

    println!(
        "Found {} files with redundant inherent impls:\n",
        redundant_files.len()
    );

    // Sort by file path for consistent output
    redundant_files.sort_by(|a, b| a.path.cmp(&b.path));

    for info in &redundant_files {
        let struct_name = info.struct_name.as_ref().unwrap();
        let rel_path = info.path.strip_prefix(&base_dir).unwrap_or(&info.path);
        println!("{}: {}", rel_path.display(), struct_name);
    }

    let elapsed = start_time.elapsed();
    eprintln!("\nCompleted in {}ms", elapsed.as_millis());

    Ok(())
}
