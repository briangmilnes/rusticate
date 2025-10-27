// Copyright (C) Brian G. Milnes 2025

//! Review for inherent impl blocks with ONLY public methods
//! 
//! Replaces: (subset of find_helper_inherent_impls.py)
//! Rule: RustRules.md - Single Implementation Pattern
//! 
//! Finds inherent impls with only public methods that should be moved to trait impls

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition};
use std::fs;
use std::time::Instant;
use rusticate::{StandardArgs, find_rust_files, format_number, find_nodes, line_number};


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_public_only_inherent_impls.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct PublicOnlyImpl {
    file: String,
    line: usize,
    type_name: String,
    pub_methods: Vec<String>,
}

fn check_file(file_path: &std::path::Path, source: &str) -> Result<Vec<PublicOnlyImpl>> {
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    
    if !parsed.errors().is_empty() {
        return Ok(Vec::new());
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut results = Vec::new();
    
    // Find all IMPL nodes
    let impl_nodes = find_nodes(root, SyntaxKind::IMPL);
    
    for impl_node in impl_nodes {
        if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
            // Check if this is an inherent impl (no trait)
            if impl_ast.trait_().is_some() {
                continue;  // Skip trait impls
            }
            
            // Get the type name
            let type_name = if let Some(self_ty) = impl_ast.self_ty() {
                let text = self_ty.syntax().to_string();
                text.split('<').next().unwrap_or(&text).trim().to_string()
            } else {
                continue;
            };
            
            let mut pub_methods = Vec::new();
            let mut has_private = false;
            
            // Analyze methods in this impl
            if let Some(assoc_item_list) = impl_ast.assoc_item_list() {
                for item in assoc_item_list.assoc_items() {
                    if let ast::AssocItem::Fn(func) = item {
                        let syntax = func.syntax();
                        let text = syntax.to_string();
                        
                        // Extract function name - get NAME child node
                        let method_name = syntax.children()
                            .find(|n| n.kind() == SyntaxKind::NAME)
                            .and_then(|name_node| name_node.first_token())
                            .map(|t| t.text().to_string())
                            .unwrap_or_else(|| "unknown".to_string());
                        
                        // Check if public
                        let trimmed = text.trim_start();
                        let is_public = trimmed.starts_with("pub ");
                        
                        if is_public {
                            pub_methods.push(method_name);
                        } else {
                            has_private = true;
                        }
                    }
                }
            }
            
            // Only report impls with ONLY public methods (no private)
            if !pub_methods.is_empty() && !has_private {
                let line = line_number(impl_ast.syntax(), source);
                
                results.push(PublicOnlyImpl {
                    file: file_path.to_string_lossy().to_string(),
                    line,
                    type_name,
                    pub_methods,
                });
            }
        }
    }
    
    Ok(results)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    log!("Entering directory '{}'", base_dir.display());
    log!("");
    
    let files = find_rust_files(&args.paths);
    
    let mut violations = Vec::new();
    
    for file in &files {
        // Skip Types.rs
        if file.to_string_lossy().contains("Types.rs") {
            continue;
        }
        
        let source = match fs::read_to_string(file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        
        match check_file(file, &source) {
            Ok(impls) => {
                let rel_path = file.strip_prefix(&base_dir).unwrap_or(file);
                
                for mut info in impls {
                    info.file = rel_path.to_string_lossy().to_string();
                    violations.push(info);
                }
            }
            Err(_) => continue,
        }
    }
    
    // Report findings in Emacs compile-mode format
    if !violations.is_empty() {
        for v in &violations {
            log!("{}:{}: inherent impl with only public methods (move to trait impl)", v.file, v.line);
            log!("  impl {} {{ {} }}", v.type_name, v.pub_methods.join(", "));
        }
        
        log!("");
        log!("✗ Found {} inherent impl(s) with only public methods in {} file(s)",
            format_number(violations.len()),
            format_number(files.len()));
        log!("Completed in {}ms", start.elapsed().as_millis());
        std::process::exit(1);
    } else {
        log!("✓ No public-only inherent impls found in {} file(s)", format_number(files.len()));
        log!("Completed in {}ms", start.elapsed().as_millis());
        Ok(())
    }
}

