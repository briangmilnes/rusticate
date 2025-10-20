// Copyright (C) Brian G. Milnes 2025

//! Review: Stub Delegation Anti-Pattern
//! 
//! Detects inherent impl blocks that duplicate trait impl functionality.
//! 
//! Anti-pattern:
//! - Type has BOTH an inherent impl AND a trait impl
//! - Trait impl methods just delegate to inherent impl methods (or vice versa)
//! - One of the impls is redundant and should be removed
//! 
//! Example from BSTSetAVLMtEph:
//!   impl<T> BSTSetAVLMtEph<T> {
//!       pub fn size(&self) -> N { ... }  // real implementation
//!   }
//!   impl<T> BSTSetAVLMtEphTrait<T> for BSTSetAVLMtEph<T> {
//!       fn size(&self) -> N { self.size() }  // stub delegation!
//!   }
//! 
//! Binary: rusticate-review-stub-delegation

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use rusticate::{StandardArgs, find_rust_files, format_number, find_nodes, line_number};

#[derive(Debug)]
struct ImplInfo {
    line: usize,
    methods: Vec<String>,
    is_trait_impl: bool,
    trait_name: Option<String>,
}

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    type_name: String,
    inherent_line: usize,
    trait_line: usize,
    trait_name: String,
    common_methods: Vec<String>,
}

fn extract_type_name(self_ty: &ast::Type) -> String {
    let text = self_ty.syntax().text().to_string();
    // Extract base type name without generic parameters
    text.split('<').next().unwrap_or(&text).trim().to_string()
}

fn extract_method_names(impl_ast: &ast::Impl) -> Vec<String> {
    let mut methods = Vec::new();
    
    if let Some(assoc_item_list) = impl_ast.assoc_item_list() {
        for item in assoc_item_list.assoc_items() {
            if let ast::AssocItem::Fn(func) = item {
                let syntax = func.syntax();
                
                // Extract function name - get NAME child node
                let method_name = syntax.children()
                    .find(|n| n.kind() == SyntaxKind::NAME)
                    .and_then(|name_node| name_node.first_token())
                    .map(|t| t.text().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                
                methods.push(method_name);
            }
        }
    }
    
    methods
}

fn check_file(file_path: &Path, source: &str) -> Result<Vec<Violation>> {
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    
    if !parsed.errors().is_empty() {
        return Ok(Vec::new());
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find all impl blocks
    let impl_nodes = find_nodes(root, SyntaxKind::IMPL);
    
    // Group impls by type name
    let mut impls_by_type: HashMap<String, Vec<ImplInfo>> = HashMap::new();
    
    for impl_node in impl_nodes {
        if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
            let type_name = if let Some(self_ty) = impl_ast.self_ty() {
                extract_type_name(&self_ty)
            } else {
                continue;
            };
            
            let methods = extract_method_names(&impl_ast);
            
            if methods.is_empty() {
                continue;
            }
            
            let is_trait_impl = impl_ast.trait_().is_some();
            let trait_name = impl_ast.trait_().map(|t| t.syntax().text().to_string());
            
            let line = line_number(impl_ast.syntax(), source);
            
            let info = ImplInfo {
                line,
                methods,
                is_trait_impl,
                trait_name,
            };
            
            impls_by_type.entry(type_name).or_default().push(info);
        }
    }
    
    // Check for stub delegation pattern
    let mut violations = Vec::new();
    
    for (type_name, impls) in impls_by_type {
        // Need at least one inherent impl and one trait impl
        let inherent_impls: Vec<_> = impls.iter().filter(|i| !i.is_trait_impl).collect();
        let trait_impls: Vec<_> = impls.iter().filter(|i| i.is_trait_impl).collect();
        
        if inherent_impls.is_empty() || trait_impls.is_empty() {
            continue;
        }
        
        // Check for overlapping methods between inherent and trait impls
        for inherent in &inherent_impls {
            let inherent_methods: HashSet<_> = inherent.methods.iter().collect();
            
            for trait_impl in &trait_impls {
                let trait_methods: HashSet<_> = trait_impl.methods.iter().collect();
                
                // Find common methods
                let common: Vec<String> = inherent_methods
                    .intersection(&trait_methods)
                    .map(|s| s.to_string())
                    .collect();
                
                if !common.is_empty() {
                    violations.push(Violation {
                        file: file_path.to_path_buf(),
                        type_name: type_name.clone(),
                        inherent_line: inherent.line,
                        trait_line: trait_impl.line,
                        trait_name: trait_impl.trait_name.clone().unwrap_or_else(|| "Unknown".to_string()),
                        common_methods: common,
                    });
                }
            }
        }
    }
    
    Ok(violations)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    println!("Entering directory '{}'", base_dir.display());
    println!();
    
    let files = find_rust_files(&args.paths);
    
    let mut all_violations = Vec::new();
    
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
            Ok(violations) => {
                let rel_path = file.strip_prefix(&base_dir).unwrap_or(file);
                
                for mut v in violations {
                    v.file = rel_path.to_path_buf();
                    all_violations.push(v);
                }
            }
            Err(_) => continue,
        }
    }
    
    // Report findings in Emacs compile-mode format
    if !all_violations.is_empty() {
        for v in &all_violations {
            println!("{}:{}: stub delegation between inherent impl and trait impl", 
                v.file.display(), v.inherent_line);
            println!("  {} has both inherent impl (line {}) and trait impl {} (line {})",
                v.type_name, v.inherent_line, v.trait_name, v.trait_line);
            println!("  {} overlapping methods: {}",
                v.common_methods.len(),
                v.common_methods.join(", "));
        }
        
        println!();
        println!("✗ Found {} stub delegation violations in {} file(s)",
            format_number(all_violations.len()),
            format_number(files.len()));
        println!("Completed in {}ms", start.elapsed().as_millis());
        std::process::exit(1);
    } else {
        println!("✓ No stub delegation found in {} file(s)", format_number(files.len()));
        println!("Completed in {}ms", start.elapsed().as_millis());
        Ok(())
    }
}

