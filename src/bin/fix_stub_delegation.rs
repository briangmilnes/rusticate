use ra_ap_syntax::{ast::{self, HasName, AstNode, make}, SyntaxKind, SyntaxNode, Edition};
use ra_ap_syntax::ted;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::fs;

/// Represents a method in an impl block
#[derive(Debug, Clone)]
struct MethodInfo {
    name: String,
    body: String,
}

/// Represents an impl block (inherent or trait)
#[derive(Debug)]
struct ImplInfo {
    node: SyntaxNode,
    methods: HashMap<String, MethodInfo>,
    is_trait_impl: bool,
}

fn extract_methods(impl_node: &SyntaxNode) -> HashMap<String, MethodInfo> {
    let mut methods = HashMap::new();
    
    if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
        if let Some(assoc_item_list) = impl_ast.assoc_item_list() {
            for item in assoc_item_list.assoc_items() {
                if let ast::AssocItem::Fn(func) = item {
                    if let Some(name) = func.name() {
                        let method_name = name.to_string();
                        let body = func.body()
                            .map(|b| b.to_string())
                            .unwrap_or_default();
                        
                        methods.insert(method_name.clone(), MethodInfo {
                            name: method_name,
                            body,
                        });
                    }
                }
            }
        }
    }
    
    methods
}

fn is_trait_impl(impl_node: &SyntaxNode) -> bool {
    if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
        impl_ast.trait_().is_some()
    } else {
        false
    }
}

fn extract_type_name(impl_node: &SyntaxNode) -> Option<String> {
    if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
        if let Some(self_ty) = impl_ast.self_ty() {
            // Extract just the base type name (before < if generic)
            let type_text = self_ty.to_string();
            let base_name = type_text.split('<').next().unwrap_or(&type_text);
            return Some(base_name.trim().to_string());
        }
    }
    None
}

fn remove_methods_from_inherent_impl(
    impl_node: &SyntaxNode,
    methods_to_remove: &HashSet<String>,
) -> Option<String> {
    if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
        if let Some(assoc_item_list) = impl_ast.assoc_item_list() {
            // Clone the tree so we can mutate it
            let root = impl_node.clone().clone_for_update();
            
            // Find the impl in the cloned tree
            let impl_in_new_tree = ast::Impl::cast(root.clone()).unwrap();
            let assoc_list = impl_in_new_tree.assoc_item_list().unwrap();
            
            // Collect methods to remove (need to collect first to avoid mutation during iteration)
            let mut items_to_remove = Vec::new();
            for item in assoc_list.assoc_items() {
                if let ast::AssocItem::Fn(func) = &item {
                    if let Some(name) = func.name() {
                        if methods_to_remove.contains(&name.to_string()) {
                            items_to_remove.push(item.syntax().clone());
                        }
                    }
                }
            }
            
            // Check if we're removing all methods
            let total_methods = assoc_list.assoc_items()
                .filter(|item| matches!(item, ast::AssocItem::Fn(_)))
                .count();
            
            if items_to_remove.len() == total_methods {
                // No methods left, remove entire impl block
                return None;
            }
            
            // Remove the methods using tree editing operations
            for item in items_to_remove {
                ted::remove(item);
            }
            
            return Some(root.to_string());
        }
    }
    Some(impl_node.to_string())
}

fn extract_trait_method_names(trait_node: &SyntaxNode) -> HashSet<String> {
    let mut method_names = HashSet::new();
    
    if let Some(trait_ast) = ast::Trait::cast(trait_node.clone()) {
        if let Some(assoc_item_list) = trait_ast.assoc_item_list() {
            for item in assoc_item_list.assoc_items() {
                if let ast::AssocItem::Fn(func) = item {
                    if let Some(name) = func.name() {
                        method_names.insert(name.to_string());
                    }
                }
            }
        }
    }
    
    method_names
}

fn extract_trait_name_from_impl(impl_node: &SyntaxNode) -> Option<String> {
    if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
        if let Some(trait_ref) = impl_ast.trait_() {
            // Get the trait name from the path
            let trait_text = trait_ref.to_string();
            // Extract base name (before any generic params)
            let base_name = trait_text.split('<').next().unwrap_or(&trait_text);
            return Some(base_name.trim().to_string());
        }
    }
    None
}

fn process_file(file_path: &Path) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(file_path)?;
    let parse = ra_ap_syntax::SourceFile::parse(&content, Edition::Edition2021);
    let root = parse.syntax_node();
    
    // Find all trait definitions and extract their method names
    let mut trait_methods: HashMap<String, HashSet<String>> = HashMap::new();
    for node in root.descendants() {
        if node.kind() == SyntaxKind::TRAIT {
            if let Some(trait_ast) = ast::Trait::cast(node.clone()) {
                if let Some(name) = trait_ast.name() {
                    let trait_name = name.to_string();
                    let methods = extract_trait_method_names(&node);
                    trait_methods.insert(trait_name, methods);
                }
            }
        }
    }
    
    // Find all impl blocks
    let mut inherent_impls: HashMap<String, ImplInfo> = HashMap::new();
    let mut type_to_trait: HashMap<String, String> = HashMap::new(); // type -> trait name
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            let methods = extract_methods(&node);
            let is_trait = is_trait_impl(&node);
            
            if let Some(type_name) = extract_type_name(&node) {
                if is_trait {
                    // Track which trait this type implements
                    if let Some(trait_name) = extract_trait_name_from_impl(&node) {
                        type_to_trait.insert(type_name, trait_name);
                    }
                } else {
                    inherent_impls.insert(type_name, ImplInfo {
                        node: node.clone(),
                        methods,
                        is_trait_impl: false,
                    });
                }
            }
        }
    }
    
    let mut total_removed = 0;
    let mut impls_removed = 0;
    let mut new_content = content.clone();
    
    // For each type with an inherent impl
    for (type_name, inherent) in &inherent_impls {
        // Check if this type has a trait impl
        if let Some(trait_name) = type_to_trait.get(type_name) {
            // Get the methods declared in the trait definition
            if let Some(trait_method_set) = trait_methods.get(trait_name) {
                // Find which methods in the inherent impl are also in the trait
                let methods_to_remove: HashSet<String> = inherent.methods.keys()
                    .filter(|name| trait_method_set.contains(*name))
                    .cloned()
                    .collect();
                
                if !methods_to_remove.is_empty() {
                    let old_impl = inherent.node.to_string();
                    
                    if let Some(new_impl) = remove_methods_from_inherent_impl(&inherent.node, &methods_to_remove) {
                        // Replace with modified impl (keeping private helper methods)
                        new_content = new_content.replace(&old_impl, &new_impl);
                        total_removed += methods_to_remove.len();
                    } else {
                        // Remove entire impl block (all methods were in trait)
                        new_content = new_content.replace(&old_impl, "");
                        total_removed += methods_to_remove.len();
                        impls_removed += 1;
                    }
                }
            }
        }
    }
    
    if total_removed > 0 {
        fs::write(file_path, new_content)?;
    }
    
    Ok((total_removed, impls_removed))
}

fn main() {
    let args = match rusticate::StandardArgs::parse() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    
    let start = std::time::Instant::now();
    let files = rusticate::find_rust_files(&args.paths);
    
    let mut total_files_processed = 0;
    let mut total_files_modified = 0;
    let mut total_methods_removed = 0;
    let mut total_impls_removed = 0;
    
    for path in &files {
        total_files_processed += 1;
        
        match process_file(path) {
            Ok((methods_removed, impls_removed)) => {
                if methods_removed > 0 {
                    total_files_modified += 1;
                    total_methods_removed += methods_removed;
                    total_impls_removed += impls_removed;
                    println!("{}: removed {} methods from inherent impl{}", 
                        path.display(),
                        methods_removed,
                        if impls_removed > 0 { 
                            format!(", removed {} inherent impl block(s)", impls_removed) 
                        } else { 
                            String::new() 
                        }
                    );
                }
            }
            Err(e) => {
                eprintln!("Error processing {}: {}", path.display(), e);
            }
        }
    }
    
    println!();
    println!("Summary:");
    println!("  Files processed: {}", total_files_processed);
    println!("  Files modified: {}", total_files_modified);
    println!("  Methods removed from inherent impls: {}", total_methods_removed);
    println!("  Inherent impl blocks removed: {}", total_impls_removed);
    println!("Completed in {}ms", start.elapsed().as_millis());
    
    if total_files_modified > 0 {
        std::process::exit(1);
    }
}

