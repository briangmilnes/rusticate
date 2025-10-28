//! Review: Detect potential trait method conflicts
//!
//! Identifies test/benchmark files that import multiple APAS modules via wildcards,
//! where those modules have traits with overlapping method names. These are call sites
//! that would break if methods move from inherent impls to trait default implementations.
//!
//! Example problem:
//!     use apas_ai::SetStEph::*;     // SetStEphTrait has .size()
//!     use apas_ai::Graph::*;         // GraphTrait has .size()
//!     let s = SetStEph::empty();
//!     s.size();  // ERROR: ambiguous after refactor!
//!
//! Binary: rusticate-review-trait-method-conflicts

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasVisibility, HasName}, SyntaxKind, SourceFile, Edition};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_trait_method_conflicts.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct ConflictInfo {
    file: PathBuf,
    imports: Vec<String>,
    conflicts: HashMap<String, Vec<(String, String)>>,
    module_methods: HashMap<String, HashSet<String>>,
}

fn extract_wildcard_imports(root: &ra_ap_syntax::SyntaxNode) -> Vec<String> {
    let mut imports = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_node) = ast::Use::cast(node) {
                if let Some(use_tree) = use_node.use_tree() {
                    if let Some(module_path) = extract_apas_wildcard_path(&use_tree) {
                        imports.push(module_path);
                    }
                }
            }
        }
    }
    
    imports
}

fn extract_apas_wildcard_path(use_tree: &ast::UseTree) -> Option<String> {
    // Check if this is a wildcard import
    use_tree.star_token()?;
    
    // Get the path before the ::*
    if let Some(path) = use_tree.path() {
        let path_str = path.to_string();
        
        // Must start with apas_ai::
        if !path_str.starts_with("apas_ai::") {
            return None;
        }
        
        // Extract the module path (everything after apas_ai::)
        let module_path = path_str.strip_prefix("apas_ai::").unwrap();
        
        // Should not be empty
        if module_path.is_empty() {
            return None;
        }
        
        return Some(module_path.to_string());
    }
    
    None
}

fn extract_trait_methods(file_path: &Path) -> HashSet<String> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return HashSet::new(),
    };
    
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut methods = HashSet::new();
    
    // Find all trait definitions
    for node in root.descendants() {
        if node.kind() == SyntaxKind::TRAIT {
            if let Some(trait_node) = ast::Trait::cast(node) {
                // Check if it's a pub trait
                if let Some(vis) = trait_node.visibility() {
                    if !vis.to_string().contains("pub") {
                        continue;
                    }
                } else {
                    continue;
                }
                
                // Extract methods from trait
                if let Some(assoc_list) = trait_node.assoc_item_list() {
                    for item in assoc_list.assoc_items() {
                        if let ast::AssocItem::Fn(func) = item {
                            if let Some(name) = func.name() {
                                methods.insert(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    
    methods
}

fn find_module_file(module_path: &str, repo_root: &Path) -> Option<PathBuf> {
    // Convert Chap05::SetStEph to src/Chap05/SetStEph.rs
    let parts: Vec<&str> = module_path.split("::").collect();
    
    if parts.is_empty() {
        return None;
    }
    
    // Try direct file: src/Chap05/SetStEph.rs
    let mut file_path = repo_root.join("src");
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            file_path = file_path.join(format!("{part}.rs"));
        } else {
            file_path = file_path.join(part);
        }
    }
    
    if file_path.exists() {
        return Some(file_path);
    }
    
    // Try mod.rs: src/Chap05/SetStEph/mod.rs
    file_path = repo_root.join("src");
    for part in parts.iter() {
        file_path = file_path.join(part);
    }
    file_path = file_path.join("mod.rs");
    
    if file_path.exists() {
        return Some(file_path);
    }
    
    None
}

fn check_file_for_conflicts(file_path: &Path, repo_root: &Path) -> Option<ConflictInfo> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return None,
    };
    
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Extract wildcard imports
    let wildcard_imports = extract_wildcard_imports(root);
    
    if wildcard_imports.len() < 2 {
        // No conflicts possible with 0 or 1 imports
        return None;
    }
    
    // Build map of module -> trait methods
    let mut module_methods = HashMap::new();
    for module_path in &wildcard_imports {
        if let Some(module_file) = find_module_file(module_path, repo_root) {
            let methods = extract_trait_methods(&module_file);
            if !methods.is_empty() {
                module_methods.insert(module_path.clone(), methods);
            }
        }
    }
    
    if module_methods.len() < 2 {
        // Need at least 2 modules with traits to have conflicts
        return None;
    }
    
    // Find overlapping method names
    let mut conflicts: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let modules: Vec<&String> = module_methods.keys().collect();
    
    for i in 0..modules.len() {
        for j in (i + 1)..modules.len() {
            let mod1 = modules[i];
            let mod2 = modules[j];
            let methods1 = &module_methods[mod1];
            let methods2 = &module_methods[mod2];
            
            let overlap: HashSet<_> = methods1.intersection(methods2).cloned().collect();
            
            for method in overlap {
                conflicts.entry(method)
                    .or_default()
                    .push((mod1.clone(), mod2.clone()));
            }
        }
    }
    
    if conflicts.is_empty() {
        return None;
    }
    
    Some(ConflictInfo {
        file: file_path.to_path_buf(),
        imports: wildcard_imports,
        conflicts,
        module_methods,
    })
}

fn search_dir(dir: &Path, files: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            } else if path.is_dir() {
                search_dir(&path, files);
            }
        }
    }
}

fn main() -> Result<()> {
    let start_time = Instant::now();
    
    let repo_root = PathBuf::from(".");
    
    // Check test and benchmark files
    let mut files_to_check = Vec::new();
    for dir_name in &["tests", "benches"] {
        let dir_path = repo_root.join(dir_name);
        search_dir(&dir_path, &mut files_to_check);
    }
    
    if files_to_check.is_empty() {
        log!("✓ No tests/ or benches/ directories found");
        return Ok(());
    }
    
    log!("Analyzing {} test/benchmark files for trait method conflicts...", files_to_check.len());
    log!("{}", "=".repeat(80));
    
    let mut all_conflicts = Vec::new();
    
    for file_path in &files_to_check {
        if let Some(conflict) = check_file_for_conflicts(file_path, &repo_root) {
            all_conflicts.push(conflict);
        }
    }
    
    if all_conflicts.is_empty() {
        log!("\n✓ No trait method conflicts detected!");
        log!("All test/benchmark files are safe for trait default implementation refactor.");
        let elapsed = start_time.elapsed();
        eprintln!("\nCompleted in {}ms", elapsed.as_millis());
        return Ok(());
    }
    
    // Report conflicts
    log!("\n✗ Found {} file(s) with potential trait method conflicts:\n", all_conflicts.len());
    
    let mut total_conflicting_methods = 0;
    
    // Sort by number of conflicts (descending)
    all_conflicts.sort_by(|a, b| b.conflicts.len().cmp(&a.conflicts.len()));
    
    for conflict in &all_conflicts {
        let rel_path = conflict.file.strip_prefix(&repo_root).unwrap_or(&conflict.file);
        total_conflicting_methods += conflict.conflicts.len();
        
        log!("\n{}:", rel_path.display());
        log!("  Imports {} modules with wildcards:", conflict.imports.len());
        for imp in &conflict.imports {
            let method_count = conflict.module_methods.get(imp).map(|m| m.len()).unwrap_or(0);
            log!("    - {} ({} trait methods)", imp, method_count);
        }
        
        log!("\n  {} conflicting method(s):", conflict.conflicts.len());
        let mut conflict_list: Vec<_> = conflict.conflicts.iter().collect();
        conflict_list.sort_by_key(|(name, _)| *name);
        
        for (method, module_pairs) in conflict_list {
            log!("    • {}()", method);
            for (mod1, mod2) in module_pairs {
                log!("        ↳ {} vs {}", mod1, mod2);
            }
        }
    }
    
    log!("\n{}", "=".repeat(80));
    log!("Summary:");
    log!("  Files with conflicts: {}", all_conflicts.len());
    log!("  Total conflicting methods: {}", total_conflicting_methods);
    log!("\nThese files will need fixes before moving methods to trait defaults:");
    log!("  1. Remove wildcard imports and use specific imports");
    log!("  2. Use fully-qualified syntax: Trait::method(&obj)");
    log!("  3. Use type ascription or turbofish to disambiguate");
    
    let elapsed = start_time.elapsed();
    eprintln!("\nCompleted in {}ms", elapsed.as_millis());
    
    Ok(())
}

