//! Review: No Trait Method Duplication
//!
//! Detects cases where trait methods are duplicated as inherent methods on the same type.
//!
//! RustRules.md: "No Trait Method Duplication (MANDATORY)"
//! - Never duplicate trait method implementations as inherent methods
//! - Trait methods are the single source of truth
//!
//! Binary: rusticate-review-no-trait-method-duplication

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasName}, SyntaxKind, SourceFile, Edition};
use rusticate::{StandardArgs, find_rust_files};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone)]
struct ImplBlock {
    is_trait_impl: bool,
    type_name: String,
    trait_name: Option<String>,
    methods: Vec<(String, usize)>, // (method_name, line_number)
}

fn extract_impl_blocks(file_path: &Path) -> Vec<ImplBlock> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();

    let mut impl_blocks = Vec::new();

    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            // Get line number before moving node
            let impl_start_offset: usize = node.text_range().start().into();
            
            if let Some(impl_node) = ast::Impl::cast(node) {
                // Get the type being implemented
                let type_name = if let Some(self_ty) = impl_node.self_ty() {
                    // Extract base type name from potentially complex type
                    extract_base_type_name(&self_ty.to_string())
                } else {
                    continue;
                };

                // Check if this is a trait impl or inherent impl
                let (is_trait_impl, trait_name) = if let Some(trait_ref) = impl_node.trait_() {
                    // This is a trait impl
                    let trait_path = trait_ref.to_string();
                    // Extract just the trait name (last component)
                    let trait_name = trait_path.split("::").last().unwrap_or(&trait_path).to_string();
                    (true, Some(trait_name))
                } else {
                    // This is an inherent impl
                    (false, None)
                };

                // Extract methods from this impl
                let mut methods = Vec::new();
                if let Some(assoc_list) = impl_node.assoc_item_list() {
                    for item in assoc_list.assoc_items() {
                        if let ast::AssocItem::Fn(func) = item {
                            if let Some(name) = func.name() {
                                let method_name = name.to_string();
                                // Get line number (use impl start as approximation)
                                let line_num = content[..impl_start_offset]
                                    .chars()
                                    .filter(|&c| c == '\n')
                                    .count() + 1;
                                methods.push((method_name, line_num));
                            }
                        }
                    }
                }

                if !methods.is_empty() {
                    impl_blocks.push(ImplBlock {
                        is_trait_impl,
                        type_name,
                        trait_name,
                        methods,
                    });
                }
            }
        }
    }

    impl_blocks
}

fn extract_base_type_name(type_str: &str) -> String {
    // Handle cases like:
    // "BSTAVLMtEph<T>" -> "BSTAVLMtEph"
    // "Vec<T>" -> "Vec"
    // "&mut Self" -> "Self"
    
    let cleaned = type_str.trim();
    
    // Remove leading & and mut
    let cleaned = cleaned.trim_start_matches('&').trim_start_matches("mut").trim();
    
    // Find the first < or whitespace and take everything before it
    if let Some(pos) = cleaned.find('<') {
        cleaned[..pos].trim().to_string()
    } else if let Some(pos) = cleaned.find(char::is_whitespace) {
        cleaned[..pos].trim().to_string()
    } else {
        cleaned.to_string()
    }
}

#[derive(Debug)]
struct Violation {
    type_name: String,
    method_name: String,
    inherent_line: usize,
    trait_name: String,
    trait_line: usize,
}

fn find_duplicate_methods(impl_blocks: &[ImplBlock]) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Group impl blocks by type
    let mut by_type: HashMap<String, (Vec<&ImplBlock>, Vec<&ImplBlock>)> = HashMap::new();
    
    for block in impl_blocks {
        let entry = by_type.entry(block.type_name.clone()).or_insert((Vec::new(), Vec::new()));
        if block.is_trait_impl {
            entry.1.push(block);
        } else {
            entry.0.push(block);
        }
    }

    // Check each type for duplicates
    for (type_name, (inherent_blocks, trait_blocks)) in by_type {
        if inherent_blocks.is_empty() || trait_blocks.is_empty() {
            continue;
        }

        // Build map of all trait methods
        let mut trait_methods: HashMap<String, (String, usize)> = HashMap::new();
        for trait_block in &trait_blocks {
            for (method_name, line_num) in &trait_block.methods {
                let trait_name = trait_block.trait_name.as_ref().unwrap().clone();
                trait_methods.insert(method_name.clone(), (trait_name, *line_num));
            }
        }

        // Check inherent methods against trait methods
        for inherent_block in &inherent_blocks {
            for (method_name, line_num) in &inherent_block.methods {
                if let Some((trait_name, trait_line)) = trait_methods.get(method_name) {
                    violations.push(Violation {
                        type_name: type_name.clone(),
                        method_name: method_name.clone(),
                        inherent_line: *line_num,
                        trait_name: trait_name.clone(),
                        trait_line: *trait_line,
                    });
                }
            }
        }
    }

    violations
}

fn main() -> Result<()> {
    let start_time = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();

    let files = find_rust_files(&args.paths);

    let mut all_violations = Vec::new();

    for file_path in &files {
        let impl_blocks = extract_impl_blocks(file_path);
        let violations = find_duplicate_methods(&impl_blocks);

        for violation in violations {
            let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
            all_violations.push((rel_path.to_path_buf(), violation));
        }
    }

    if all_violations.is_empty() {
        println!("✓ No Trait Method Duplication: PASS");
        let elapsed = start_time.elapsed();
        eprintln!("\nCompleted in {}ms", elapsed.as_millis());
        return Ok(());
    }

    // Report violations
    println!("✗ No Trait Method Duplication: FAIL\n");
    println!("Found {} duplicate method(s):\n", all_violations.len());

    for (file_path, violation) in &all_violations {
        println!(
            "{}:{}: Duplicate method '{}' in inherent impl for {} (also in {} trait impl at line {})",
            file_path.display(),
            violation.inherent_line,
            violation.method_name,
            violation.type_name,
            violation.trait_name,
            violation.trait_line
        );
    }

    println!("\nFix: Delete the inherent method and keep only the trait method implementation.");

    let elapsed = start_time.elapsed();
    eprintln!("\nCompleted in {}ms", elapsed.as_millis());

    Ok(())
}

