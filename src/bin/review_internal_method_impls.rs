// Copyright (C) Brian G. Milnes 2025

//! Review for inherent impl blocks that should be eliminated
//! 
//! Replaces: scripts/rust/src/find_helper_inherent_impls.py
//! Rule: RustRules.md - Single Implementation Pattern
//! 
//! Finds:
//! 1. Inherent impls with ONLY internal methods (can eliminate completely)
//! 2. Inherent impls with mixed pub/internal (extract internal methods)
//! 3. Inherent impls with only public methods (should be in trait impl)

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition};
use std::fs;
use std::time::Instant;
use rusticate::{StandardArgs, find_rust_files, format_number, find_nodes, line_number};

#[derive(Debug)]
struct InherentImpl {
    file: String,
    line: usize,
    type_name: String,
    pub_methods: Vec<String>,
    private_methods: Vec<String>,
}

impl InherentImpl {
    fn category(&self) -> &str {
        if self.pub_methods.is_empty() && !self.private_methods.is_empty() {
            "ONLY_PRIVATE"
        } else if !self.pub_methods.is_empty() && !self.private_methods.is_empty() {
            "MIXED"
        } else if !self.pub_methods.is_empty() && self.private_methods.is_empty() {
            "ONLY_PUBLIC"
        } else {
            "EMPTY"
        }
    }
}

fn check_file(file_path: &std::path::Path, source: &str) -> Result<Vec<InherentImpl>> {
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
            let mut private_methods = Vec::new();
            
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
                        
                        // Check if public - look at first few tokens
                        // Function starts with either "pub " or just "fn "
                        let trimmed = text.trim_start();
                        let is_public = trimmed.starts_with("pub ");
                        
                        if is_public {
                            pub_methods.push(method_name);
                        } else {
                            private_methods.push(method_name);
                        }
                    }
                }
            }
            
            // Only report if there are methods
            if !pub_methods.is_empty() || !private_methods.is_empty() {
                let line = line_number(impl_ast.syntax(), source);
                
                results.push(InherentImpl {
                    file: file_path.to_string_lossy().to_string(),
                    line,
                    type_name,
                    pub_methods,
                    private_methods,
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
    
    println!("Entering directory '{}'", base_dir.display());
    println!();
    
    let files = find_rust_files(&args.paths);
    
    let mut only_private = Vec::new();
    let mut mixed = Vec::new();
    let mut only_public = Vec::new();
    
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
                
                for impl_info in impls {
                    let mut info = impl_info;
                    info.file = rel_path.to_string_lossy().to_string();
                    
                    match info.category() {
                        "ONLY_PRIVATE" => only_private.push(info),
                        "MIXED" => mixed.push(info),
                        "ONLY_PUBLIC" => only_public.push(info),
                        _ => {}
                    }
                }
            }
            Err(_) => continue,
        }
    }
    
    // Report findings in Emacs compile-mode format
    let mut has_issues = false;
    
    if !only_private.is_empty() {
        has_issues = true;
        for info in &only_private {
            println!("{}:{}: inherent impl with only internal methods (eliminate)", info.file, info.line);
            println!("  impl {} {{ {} }}", info.type_name, info.private_methods.join(", "));
        }
    }
    
    if !mixed.is_empty() {
        has_issues = true;
        for info in &mixed {
            println!("{}:{}: inherent impl with mixed pub/internal (extract internal)", info.file, info.line);
            println!("  impl {} {{ pub: {}; internal: {} }}", 
                info.type_name, 
                info.pub_methods.join(", "), 
                info.private_methods.join(", "));
        }
    }
    
    if !only_public.is_empty() {
        has_issues = true;
        for info in &only_public {
            println!("{}:{}: inherent impl with only public methods (move to trait)", info.file, info.line);
            println!("  impl {} {{ {} }}", info.type_name, info.pub_methods.join(", "));
        }
    }
    
    // Summary
    let total_issues = only_private.len() + mixed.len() + only_public.len();
    
    if has_issues {
        println!("{}", "=".repeat(80));
        println!("SUMMARY:");
        println!("  Only internal methods (ELIMINATE): {}", format_number(only_private.len()));
        println!("  Mixed pub/internal (EXTRACT internal): {}", format_number(mixed.len()));
        println!("  Only public (MOVE to trait): {}", format_number(only_public.len()));
        println!("  TOTAL inherent impl violations: {}", format_number(total_issues));
        println!("Completed in {}ms", start.elapsed().as_millis());
        std::process::exit(1);
    } else {
        println!("✓ No problematic inherent impls found in {} file(s)", format_number(files.len()));
        println!("Completed in {}ms", start.elapsed().as_millis());
        Ok(())
    }
}

