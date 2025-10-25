//! Review: Unnecessary UFCS and Qualified Paths
//!
//! Finds UFCS calls and qualified paths that could be simplified because
//! the trait is in scope via glob imports.
//!
//! Binary: review-unnecessary-ufcs-and-qualified-paths

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasName, HasVisibility}, SyntaxKind, SourceFile, Edition, SyntaxNode};
use rusticate::{find_rust_files, StandardArgs};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

macro_rules! log {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        println!("{}", msg);
    }};
}

#[derive(Debug, Clone)]
struct UfcsCall {
    line: usize,
    type_name: String,
    trait_name: String,
    method: String,
    full_text: String,
}

#[derive(Debug, Clone)]
struct QualifiedPath {
    line: usize,
    type_name: String,
    method: String,
    full_text: String,
}

struct FileReport {
    file: PathBuf,
    ufcs_calls: Vec<UfcsCall>,
    qualified_paths: Vec<QualifiedPath>,
    glob_imports: Vec<String>,
    explicit_trait_imports: HashSet<String>,
}

fn get_line_number(node: &SyntaxNode, content: &str) -> usize {
    let offset: usize = node.text_range().start().into();
    content[..offset].lines().count()
}

fn find_glob_imports(root: &SyntaxNode) -> Vec<String> {
    let mut imports = Vec::new();
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            let use_text = node.to_string();
            if use_text.contains("::*") {
                imports.push(use_text);
            }
        }
    }
    imports
}

fn find_explicit_trait_imports(root: &SyntaxNode) -> HashSet<String> {
    let mut traits = HashSet::new();
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            // Look for specific trait imports (names containing "Trait")
            for descendant in node.descendants() {
                if descendant.kind() == SyntaxKind::NAME_REF {
                    if let Some(name_ref) = ast::NameRef::cast(descendant) {
                        let text = name_ref.text().to_string();
                        if text.contains("Trait") {
                            traits.insert(text);
                        }
                    }
                }
            }
        }
    }
    traits
}

fn find_ufcs_calls(root: &SyntaxNode, content: &str) -> Vec<UfcsCall> {
    let mut calls = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::CALL_EXPR {
            if let Some(call_expr) = ast::CallExpr::cast(node.clone()) {
                if let Some(expr) = call_expr.expr() {
                    if let ast::Expr::PathExpr(path_expr) = expr {
                        if let Some(path) = path_expr.path() {
                            if let Some(qualifier) = path.qualifier() {
                                // Check if this looks like <Type as Trait>::method
                                let full_text = path_expr.syntax().to_string();
                                if full_text.contains(" as ") {
                                    // Parse the UFCS pattern
                                    if let Some(type_name) = extract_ufcs_type(&full_text) {
                                        if let Some(trait_name) = extract_ufcs_trait(&full_text) {
                                            if let Some(segment) = path.segment() {
                                                let method = segment.to_string();
                                                calls.push(UfcsCall {
                                                    line: get_line_number(&node, content),
                                                    type_name,
                                                    trait_name,
                                                    method,
                                                    full_text,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    calls
}

fn extract_ufcs_type(text: &str) -> Option<String> {
    // Extract type from: <Type as Trait>::method
    if let Some(start) = text.find('<') {
        if let Some(as_pos) = text.find(" as ") {
            if as_pos > start + 1 {
                let type_part = &text[start + 1..as_pos];
                return Some(type_part.trim().to_string());
            }
        }
    }
    None
}

fn extract_ufcs_trait(text: &str) -> Option<String> {
    // Extract trait from: <Type as Trait>::method
    if let Some(as_pos) = text.find(" as ") {
        // Find the matching > for the UFCS call
        // Need to handle nested generics like <Type<T> as Trait<T>>
        let after_as = &text[as_pos + 4..];
        let mut depth = 0;
        let mut end_pos = None;
        
        for (i, ch) in after_as.chars().enumerate() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    if depth == 0 {
                        end_pos = Some(i);
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        
        if let Some(end) = end_pos {
            let trait_part = &after_as[..end];
            return Some(trait_part.trim().to_string());
        }
    }
    None
}

fn resolve_module_path(glob_import: &str, current_file: &Path) -> Option<PathBuf> {
    // Parse: use crate::Chap19::ArraySeqMtEph::ArraySeqMtEph::*;
    // or: use apas_ai::Chap19::ArraySeqMtEph::ArraySeqMtEph::*;
    // to get the module file path
    
    if !glob_import.contains("use crate::") && !glob_import.contains("use ") {
        return None;
    }
    
    // Extract the module path
    let parts: Vec<&str> = glob_import
        .trim_start_matches("use ")
        .trim_end_matches("::*;")
        .trim_end_matches("::*")
        .trim_end_matches(";")
        .split("::")
        .collect();
    
    if parts.is_empty() {
        return None;
    }
    
    // Skip the first part if it's "crate" or the crate name (e.g., "apas_ai")
    let module_parts = if parts[0] == "crate" || !parts[0].starts_with("std") {
        &parts[1..]
    } else {
        return None;
    };
    
    // Find the crate root (go up until we find src/)
    let mut current = current_file.to_path_buf();
    while let Some(parent) = current.parent() {
        if parent.ends_with("src") || parent.ends_with("tests") || parent.ends_with("benches") {
            // Go up one more to get to the crate root, then into src/
            if let Some(crate_root) = parent.parent() {
                let src_dir = crate_root.join("src");
                if src_dir.exists() {
                    // Build the module path
                    let mut module_path = src_dir;
                    for part in module_parts {
                        module_path.push(part);
                    }
                    module_path.set_extension("rs");
                    
                    if module_path.exists() {
                        return Some(module_path);
                    }
                    
                    // Try as a module directory
                    module_path.pop();
                    module_path.push("mod.rs");
                    if module_path.exists() {
                        return Some(module_path);
                    }
                }
            }
            
            break;
        }
        current = parent.to_path_buf();
    }
    
    None
}

fn extract_exported_traits(module_path: &Path) -> HashSet<String> {
    let mut traits = HashSet::new();
    
    if let Ok(content) = fs::read_to_string(module_path) {
        let parsed = SourceFile::parse(&content, Edition::Edition2021);
        let tree = parsed.tree();
        let root = tree.syntax();
        
        // Find all pub trait definitions (including those inside pub mod blocks)
        for node in root.descendants() {
            if node.kind() == SyntaxKind::TRAIT {
                if let Some(trait_def) = ast::Trait::cast(node.clone()) {
                    if let Some(name) = trait_def.name() {
                        // Check if it's public
                        let is_pub = if let Some(visibility) = trait_def.visibility() {
                            visibility.to_string().contains("pub")
                        } else {
                            // If no visibility specified, it's public within its module
                            // For glob imports from a module, we consider it accessible
                            true
                        };
                        
                        if is_pub {
                            traits.insert(name.to_string());
                        }
                    }
                }
            }
        }
        
        // Also look for re-exports: pub use ...Trait;
        for node in root.descendants() {
            if node.kind() == SyntaxKind::USE {
                let use_text = node.to_string();
                if use_text.starts_with("pub use") && use_text.contains("Trait") {
                    // Extract trait names from the use statement
                    for descendant in node.descendants() {
                        if descendant.kind() == SyntaxKind::NAME_REF {
                            if let Some(name_ref) = ast::NameRef::cast(descendant) {
                                let text = name_ref.text().to_string();
                                if text.contains("Trait") {
                                    traits.insert(text);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    traits
}

fn is_trait_in_scope(
    trait_name: &str,
    glob_imports: &[String],
    explicit_traits: &HashSet<String>,
    current_file: &Path,
    trait_cache: &mut HashMap<String, HashSet<String>>,
) -> bool {
    // Check explicit imports first
    if explicit_traits.contains(trait_name) {
        return true;
    }
    
    // Check glob imports
    for glob_import in glob_imports {
        // Get traits from cache or parse the module
        let cache_key = glob_import.clone();
        if !trait_cache.contains_key(&cache_key) {
            if let Some(module_path) = resolve_module_path(glob_import, current_file) {
                let traits = extract_exported_traits(&module_path);
                trait_cache.insert(cache_key.clone(), traits);
            } else {
                trait_cache.insert(cache_key.clone(), HashSet::new());
            }
        }
        
        if let Some(traits) = trait_cache.get(&cache_key) {
            if traits.contains(trait_name) {
                return true;
            }
        }
    }
    
    false
}

fn analyze_file(file_path: &Path, trait_cache: &mut HashMap<String, HashSet<String>>) -> Result<FileReport> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let glob_imports = find_glob_imports(root);
    let explicit_trait_imports = find_explicit_trait_imports(root);
    let ufcs_calls = find_ufcs_calls(root, &content);
    
    // Filter to only unnecessary UFCS calls (where trait is in scope)
    let unnecessary_ufcs: Vec<UfcsCall> = ufcs_calls
        .into_iter()
        .filter(|call| {
            is_trait_in_scope(
                &call.trait_name,
                &glob_imports,
                &explicit_trait_imports,
                file_path,
                trait_cache,
            )
        })
        .collect();
    
    Ok(FileReport {
        file: file_path.to_path_buf(),
        ufcs_calls: unnecessary_ufcs,
        qualified_paths: Vec::new(), // TODO: implement qualified path detection
        glob_imports,
        explicit_trait_imports,
    })
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/review_unnecessary_ufcs_and_qualified_paths.log").ok();
    
    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let all_files = find_rust_files(&args.paths);
    
    log!("Entering directory '{}'", base_dir.display());
    println!();
    
    let mut trait_cache: HashMap<String, HashSet<String>> = HashMap::new();
    let mut reports = Vec::new();
    
    for file_path in &all_files {
        if let Ok(report) = analyze_file(file_path, &mut trait_cache) {
            if !report.ufcs_calls.is_empty() || !report.qualified_paths.is_empty() {
                reports.push(report);
            }
        }
    }
    
    // Sort by file path
    reports.sort_by(|a, b| a.file.cmp(&b.file));
    
    // Print results
    println!();
    log!("{}", "=".repeat(80));
    log!("UNNECESSARY UFCS CALLS:");
    log!("{}", "=".repeat(80));
    println!();
    
    let mut total_ufcs = 0;
    
    for report in &reports {
        if !report.ufcs_calls.is_empty() {
            let rel_path = report.file.strip_prefix(&base_dir).unwrap_or(&report.file);
            log!("{}:1:", rel_path.display());
            log!("  {} UFCS calls", report.ufcs_calls.len());
            
            for call in &report.ufcs_calls {
                log!("    Line {}: {} -> {}::{}", 
                     call.line, 
                     call.full_text,
                     call.type_name,
                     call.method);
            }
            
            println!();
            total_ufcs += report.ufcs_calls.len();
        }
    }
    
    if total_ufcs == 0 {
        log!("None found");
        println!();
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Total unnecessary UFCS calls: {}", total_ufcs);
    log!("  Files with issues: {}", reports.len());
    log!("{}", "=".repeat(80));
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

