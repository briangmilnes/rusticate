//! Review: StT Compliance
//!
//! Detects public structs that don't satisfy StT requirements.
//!
//! StT (Single-Threaded Type) = Eq + Clone + Display + Debug + Sized
//!
//! Binary: rusticate-review-stt-compliance

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasName, HasVisibility, HasAttrs}, SyntaxKind, SourceFile, Edition};
use rusticate::{StandardArgs, find_rust_files};
use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone)]
struct NonSttStruct {
    name: String,
    line: usize,
    derives: HashSet<String>,
    missing: Vec<String>,
}

fn extract_derives(struct_node: &ast::Struct) -> HashSet<String> {
    let mut derives = HashSet::new();
    
    for attr in struct_node.attrs() {
        let attr_text = attr.to_string();
        if attr_text.starts_with("#[derive(") {
            // Extract traits: #[derive(Debug, Clone, ...)]
            if let Some(start) = attr_text.find('(') {
                if let Some(end) = attr_text.rfind(')') {
                    let traits_str = &attr_text[start + 1..end];
                    for trait_name in traits_str.split(',') {
                        derives.insert(trait_name.trim().to_string());
                    }
                }
            }
        }
    }
    
    derives
}

fn has_manual_impl(root: &ra_ap_syntax::SyntaxNode, struct_name: &str, trait_name: &str) -> bool {
    // Look for: impl Trait for StructName
    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            if let Some(impl_node) = ast::Impl::cast(node) {
                // Check if it's for our struct
                if let Some(self_ty) = impl_node.self_ty() {
                    let self_ty_str = self_ty.to_string();
                    if !self_ty_str.contains(struct_name) {
                        continue;
                    }
                    
                    // Check if it implements the trait
                    if let Some(trait_ref) = impl_node.trait_() {
                        let trait_str = trait_ref.to_string();
                        if trait_str.contains(trait_name) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

fn analyze_file(file_path: &Path) -> Vec<NonSttStruct> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();

    let mut non_stt_structs = Vec::new();

    for node in root.descendants() {
        if node.kind() == SyntaxKind::STRUCT {
            if let Some(struct_node) = ast::Struct::cast(node.clone()) {
                // Check if it's public
                let is_public = if let Some(vis) = struct_node.visibility() {
                    vis.to_string().contains("pub")
                } else {
                    false
                };
                
                if !is_public {
                    continue;
                }
                
                let struct_name = if let Some(name) = struct_node.name() {
                    name.to_string()
                } else {
                    continue;
                };
                
                // Get line number
                let line_num = content[..node.text_range().start().into()]
                    .chars()
                    .filter(|&c| c == '\n')
                    .count() + 1;
                
                // Extract derives
                let derives = extract_derives(&struct_node);
                
                // Check for required traits
                let has_clone = derives.contains("Clone") || has_manual_impl(root, &struct_name, "Clone");
                let has_display = derives.contains("Display") || has_manual_impl(root, &struct_name, "Display");
                let has_debug = derives.contains("Debug") || has_manual_impl(root, &struct_name, "Debug");
                let has_eq = derives.contains("Eq") || has_manual_impl(root, &struct_name, "Eq");
                
                let mut missing = Vec::new();
                if !has_clone {
                    missing.push("Clone".to_string());
                }
                if !has_display {
                    missing.push("Display".to_string());
                }
                if !has_debug {
                    missing.push("Debug".to_string());
                }
                if !has_eq {
                    missing.push("Eq".to_string());
                }
                
                if !missing.is_empty() {
                    non_stt_structs.push(NonSttStruct {
                        name: struct_name,
                        line: line_num,
                        derives,
                        missing,
                    });
                }
            }
        }
    }

    non_stt_structs
}

fn main() -> Result<()> {
    let start_time = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();

    // Only check src/ files
    let src_files: Vec<_> = find_rust_files(&args.paths)
        .into_iter()
        .filter(|p| p.starts_with(base_dir.join("src")))
        .collect();

    println!("Analyzing {} source files for StT compliance...", src_files.len());
    println!("{}", "=".repeat(80));
    println!();
    println!("StT requirements: Eq + Clone + Display + Debug + Sized");
    println!();

    let mut all_violations = Vec::new();

    for file_path in &src_files {
        let violations = analyze_file(file_path);
        if !violations.is_empty() {
            all_violations.push((file_path.clone(), violations));
        }
    }

    if all_violations.is_empty() {
        println!("\n✓ All public structs satisfy StT requirements!");
        let elapsed = start_time.elapsed();
        eprintln!("\nCompleted in {}ms", elapsed.as_millis());
        return Ok(());
    }

    // Group by what's missing
    let mut missing_clone = 0;
    let mut missing_display = 0;
    let mut missing_debug = 0;
    let mut missing_eq = 0;

    for (_, violations) in &all_violations {
        for v in violations {
            if v.missing.contains(&"Clone".to_string()) {
                missing_clone += 1;
            }
            if v.missing.contains(&"Display".to_string()) {
                missing_display += 1;
            }
            if v.missing.contains(&"Debug".to_string()) {
                missing_debug += 1;
            }
            if v.missing.contains(&"Eq".to_string()) {
                missing_eq += 1;
            }
        }
    }

    let total_count: usize = all_violations.iter().map(|(_, v)| v.len()).sum();

    println!("✗ Found {} struct(s) that don't satisfy StT:\n", total_count);

    println!("Summary by missing trait:");
    println!("  Missing Clone:   {}", missing_clone);
    println!("  Missing Display: {}", missing_display);
    println!("  Missing Debug:   {}", missing_debug);
    println!("  Missing Eq:      {}", missing_eq);

    println!("\n{}", "=".repeat(80));
    println!("Detailed list:\n");

    // Sort by number of violations (descending)
    let mut sorted_violations = all_violations;
    sorted_violations.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    for (file_path, violations) in &sorted_violations {
        let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
        println!("{}:", rel_path.display());
        for v in violations {
            let missing_str = v.missing.join(", ");
            let mut derives_vec: Vec<_> = v.derives.iter().cloned().collect();
            derives_vec.sort();
            let derives_str = if derives_vec.is_empty() {
                "none".to_string()
            } else {
                derives_vec.join(", ")
            };
            println!("  Line {}: {}", v.line, v.name);
            println!("    Has derives: {}", derives_str);
            println!("    Missing: {}", missing_str);
        }
        println!();
    }

    let elapsed = start_time.elapsed();
    eprintln!("Completed in {}ms", elapsed.as_millis());

    Ok(())
}

