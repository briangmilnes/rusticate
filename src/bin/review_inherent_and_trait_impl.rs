// Copyright (C) Brian G. Milnes 2025

//! Review for duplicate functionality in inherent and trait impls
//! 
//! Replaces: (no Python equivalent - new rule)
//! Rule: RustRules.md Lines 215-246
//! 
//! Checks: If a type has both inherent impl and trait impl, flag public methods
//! that appear in both (violates Single Implementation Pattern)

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::Instant;
use rusticate::{StandardArgs, find_rust_files, format_number, find_nodes};


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_inherent_and_trait_impl.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn check_file(source: &str) -> Result<Vec<String>> {
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    
    if !parsed.errors().is_empty() {
        return Ok(Vec::new());
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut issues = Vec::new();
    
    // Map: Type name -> (inherent public methods, trait impl public methods with trait name)
    let mut type_impls: HashMap<String, (HashSet<String>, HashMap<String, String>)> = HashMap::new();
    
    // Find all IMPL nodes
    let impl_nodes = find_nodes(root, SyntaxKind::IMPL);
    
    for impl_node in impl_nodes {
        if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
            // Get the type being implemented for
            let type_name = if let Some(self_ty) = impl_ast.self_ty() {
                extract_type_name(&self_ty.syntax().to_string())
            } else {
                continue;
            };
            
            // Check if this is a trait impl or inherent impl
            let is_trait_impl = impl_ast.trait_().is_some();
            let trait_name = if is_trait_impl {
                impl_ast.trait_().map(|t| {
                    let text = t.syntax().to_string();
                    text.split(|c: char| !c.is_alphanumeric() && c != '_')
                        .find(|s| !s.is_empty())
                        .unwrap_or(&text)
                        .to_string()
                })
            } else {
                None
            };
            
            // Get all public functions in this impl
            if let Some(assoc_item_list) = impl_ast.assoc_item_list() {
                for item in assoc_item_list.assoc_items() {
                    if let ast::AssocItem::Fn(func) = item {
                        // Check if it's public by looking at the syntax
                        let func_text = func.syntax().to_string();
                        let is_public = func_text.trim_start().starts_with("pub ");
                        
                        if is_public {
                            // Get function name from syntax
                            if let Some(name_node) = func.syntax().children_with_tokens()
                                .find(|n| n.kind() == SyntaxKind::IDENT)
                            {
                                let method_name = name_node.to_string();
                                
                                let entry = type_impls.entry(type_name.clone()).or_insert_with(|| {
                                    (HashSet::new(), HashMap::new())
                                });
                                
                                if is_trait_impl {
                                    if let Some(ref trait_name) = trait_name {
                                        entry.1.insert(method_name, trait_name.clone());
                                    }
                                } else {
                                    entry.0.insert(method_name);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Check for overlaps
    for (type_name, (inherent_methods, trait_methods)) in &type_impls {
        for method in inherent_methods {
            if trait_methods.contains_key(method) {
                let trait_name = &trait_methods[method];
                issues.push(format!(
                    "Type '{type_name}' has public method '{method}' in both inherent impl and trait impl for '{trait_name}' (violates Single Implementation Pattern)"
                ));
            }
        }
    }
    
    Ok(issues)
}

fn extract_type_name(type_str: &str) -> String {
    // Extract base type name from "Foo<T>" -> "Foo"
    type_str.split('<').next().unwrap_or(type_str).trim().to_string()
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    log!("Entering directory '{}'", base_dir.display());
    log!("");
    
    let files = find_rust_files(&args.paths);
    let mut total_issues = 0;
    let mut files_with_issues = 0;
    
    for file in &files {
        let rel_path = file.strip_prefix(&base_dir).unwrap_or(file);
        
        let source = match fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading {}: {}", rel_path.display(), e);
                continue;
            }
        };
        
        match check_file(&source) {
            Ok(issues) => {
                if !issues.is_empty() {
                    files_with_issues += 1;
                    total_issues += issues.len();
                    
                    for issue in issues {
                        log!("{}:1: {}", rel_path.display(), issue);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error processing {}: {}", rel_path.display(), e);
            }
        }
    }
    
    log!("");
    if total_issues > 0 {
        log!(
            "✗ Found {} issue(s) in {} file(s) out of {} checked",
            format_number(total_issues),
            format_number(files_with_issues),
            format_number(files.len())
        );
        log!("Completed in {}ms", start.elapsed().as_millis());
        std::process::exit(1);
    } else {
        log!(
            "✓ No issues found in {} file(s)",
            format_number(files.len())
        );
        log!("Completed in {}ms", start.elapsed().as_millis());
        Ok(())
    }
}
