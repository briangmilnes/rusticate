//! Review: Inherent + Trait Impl Pattern
//!
//! Detects structs that have BOTH inherent impl AND trait impl blocks.
//! Most structs should have ONLY trait impl, not both.
//!
//! Binary: rusticate-review-inherent-plus-trait-impl

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition};
use rusticate::{StandardArgs, find_rust_files};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

const STANDARD_TRAITS: &[&str] = &[
    "Debug", "Clone", "Copy", "PartialEq", "Eq", "PartialOrd", "Ord",
    "Hash", "Display", "Default", "From", "Into", "AsRef", "AsMut",
    "Deref", "DerefMut", "Drop", "Iterator", "IntoIterator",
    "Send", "Sync", "Sized", "Unpin"
];


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_inherent_plus_trait_impl.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct Violation {
    struct_name: String,
    inherent_lines: Vec<usize>,
    traits: HashMap<String, Vec<usize>>,
}

fn analyze_file(file_path: &Path) -> Vec<Violation> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();

    // Track struct_name -> {'inherent': [lines], 'traits': {trait_name: [lines]}}
    let mut struct_impls: HashMap<String, (Vec<usize>, HashMap<String, Vec<usize>>)> = HashMap::new();

    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            if let Some(impl_node) = ast::Impl::cast(node.clone()) {
                let line_num = content[..node.text_range().start().into()]
                    .chars()
                    .filter(|&c| c == '\n')
                    .count() + 1;

                if let Some(self_ty) = impl_node.self_ty() {
                    let self_ty_str = self_ty.to_string();
                    // Extract base type name
                    let struct_name = if let Some(pos) = self_ty_str.find('<') {
                        self_ty_str[..pos].trim().to_string()
                    } else {
                        self_ty_str.trim().to_string()
                    };

                    if impl_node.trait_().is_none() {
                        // Inherent impl
                        struct_impls.entry(struct_name).or_insert((Vec::new(), HashMap::new())).0.push(line_num);
                    } else {
                        // Trait impl
                        if let Some(trait_ref) = impl_node.trait_() {
                            let trait_str = trait_ref.to_string();
                            // Extract trait name (last component)
                            let trait_name = trait_str.split("::").last().unwrap_or(&trait_str);
                            
                            // Remove generics
                            let trait_name = if let Some(pos) = trait_name.find('<') {
                                trait_name[..pos].trim().to_string()
                            } else {
                                trait_name.trim().to_string()
                            };

                            // Skip standard traits
                            if !STANDARD_TRAITS.contains(&trait_name.as_str()) {
                                struct_impls.entry(struct_name).or_insert((Vec::new(), HashMap::new()))
                                    .1.entry(trait_name).or_default().push(line_num);
                            }
                        }
                    }
                }
            }
        }
    }

    // Find structs with BOTH inherent AND trait impls
    let mut violations = Vec::new();
    for (struct_name, (inherent_lines, traits)) in struct_impls {
        if !inherent_lines.is_empty() && !traits.is_empty() {
            violations.push(Violation {
                struct_name,
                inherent_lines,
                traits,
            });
        }
    }

    violations
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

    let mut all_violations: HashMap<_, Vec<Violation>> = HashMap::new();

    for file_path in &src_files {
        let violations = analyze_file(file_path);
        if !violations.is_empty() {
            all_violations.insert(file_path.clone(), violations);
        }
    }

    if all_violations.is_empty() {
        log!("\n✓ All structs use trait impl only (no inherent+trait duplication)!");
        let elapsed = start_time.elapsed();
        eprintln!("\nCompleted in {}ms", elapsed.as_millis());
        return Ok(());
    }

    let total_count: usize = all_violations.values().map(|v| v.len()).sum();

    log!("✗ Inherent + Trait Impl Pattern: {} struct(s)\n", total_count);
    log!("Structs should use TRAIT impl only, not both inherent impl and trait impl.\n");
    log!("{}", "=".repeat(80));

    for (file_path, file_violations) in all_violations.iter() {
        let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
        log!("\n{}:", rel_path.display());

        for violation in file_violations {
            log!("  {}:", violation.struct_name);
            let inherent_str = violation.inherent_lines.iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            log!("    Inherent impl at line(s): {}", inherent_str);
            
            let mut trait_names: Vec<_> = violation.traits.keys().collect();
            trait_names.sort();
            for trait_name in trait_names {
                let lines = &violation.traits[trait_name];
                let lines_str = lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ");
                log!("    Trait impl {} at line(s): {}", trait_name, lines_str);
            }
        }
    }

    log!("\n{}", "=".repeat(80));
    log!("Total: {} struct(s) with both inherent and trait impls", total_count);
    log!("\nRecommendation: Remove inherent impl, keep only trait impl.");

    let elapsed = start_time.elapsed();
    eprintln!("\nCompleted in {}ms", elapsed.as_millis());

    Ok(())
}

