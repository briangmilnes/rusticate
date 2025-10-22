use ra_ap_syntax::{ast::{self, HasName, AstNode}, SyntaxKind, SyntaxNode, Edition};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::fs;

/// Check if a method body is a stub delegation (calls Self::method or Type::method)
fn is_stub_delegation(body: &str, method_name: &str, type_name: &str) -> bool {
    let body_trimmed = body.trim();
    
    // Pattern: { Self::method(...) } or { Type::method(...) }
    if body_trimmed.starts_with('{') && body_trimmed.ends_with('}') {
        let inner = body_trimmed[1..body_trimmed.len()-1].trim();
        
        // Check for Self::method_name( or Type::method_name(
        let self_call = format!("Self::{}(", method_name);
        let type_call = format!("{}::{}(", type_name, method_name);
        
        if inner.starts_with(&self_call) || inner.starts_with(&type_call) {
            return true;
        }
    }
    
    false
}

fn extract_type_name(impl_node: &SyntaxNode) -> Option<String> {
    if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
        if let Some(self_ty) = impl_ast.self_ty() {
            let type_text = self_ty.to_string();
            let base_name = type_text.split('<').next().unwrap_or(&type_text);
            return Some(base_name.trim().to_string());
        }
    }
    None
}

/// Extract trait name from a trait impl
fn extract_trait_name_from_impl(impl_node: &SyntaxNode) -> Option<String> {
    if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
        if let Some(trait_ref) = impl_ast.trait_() {
            let trait_text = trait_ref.to_string();
            let base_name = trait_text.split('<').next().unwrap_or(&trait_text);
            return Some(base_name.trim().to_string());
        }
    }
    None
}

/// Extract method names from a trait definition
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

/// Extract parameter list from a function, preserving mut and other attributes
fn extract_param_list(func: &ast::Fn) -> String {
    if let Some(param_list) = func.param_list() {
        param_list.to_string()
    } else {
        "()".to_string()
    }
}

fn process_file(file_path: &PathBuf) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(file_path)?;
    let parse = ra_ap_syntax::SourceFile::parse(&content, Edition::Edition2021);
    let root = parse.syntax_node();
    
    // Find all trait definitions and their methods
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
    
    // Find all impl blocks for each type
    let mut type_impls: HashMap<String, Vec<SyntaxNode>> = HashMap::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            if let Some(type_name) = extract_type_name(&node) {
                type_impls.entry(type_name).or_insert_with(Vec::new).push(node.clone());
            }
        }
    }
    
    let mut new_content = content.clone();
    let mut total_moved = 0;
    let mut total_removed = 0;
    
    // For each type that has multiple impls
    for (type_name, impls) in &type_impls {
        if impls.len() < 2 {
            continue;
        }
        
        // Separate inherent and trait impls
        let mut inherent_impl: Option<&SyntaxNode> = None;
        let mut trait_impls: Vec<(&SyntaxNode, String)> = Vec::new();
        
        for impl_node in impls {
            if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
                if let Some(trait_name) = extract_trait_name_from_impl(impl_node) {
                    trait_impls.push((impl_node, trait_name));
                } else {
                    inherent_impl = Some(impl_node);
                }
            }
        }
        
        if inherent_impl.is_none() || trait_impls.is_empty() {
            continue;
        }
        
        let inherent = inherent_impl.unwrap();
        
        // Extract methods from inherent impl with full signatures
        let mut inherent_methods: HashMap<String, (ast::Fn, String)> = HashMap::new();
        if let Some(impl_ast) = ast::Impl::cast(inherent.clone()) {
            if let Some(assoc_list) = impl_ast.assoc_item_list() {
                for item in assoc_list.assoc_items() {
                    if let ast::AssocItem::Fn(func) = item {
                        if let Some(name) = func.name() {
                            let method_name = name.to_string();
                            let full_method = func.syntax().to_string();
                            inherent_methods.insert(method_name, (func, full_method));
                        }
                    }
                }
            }
        }
        
        // Process each trait impl
        for (trait_impl, trait_name) in &trait_impls {
            // Get the trait definition's methods
            let trait_def_methods = trait_methods.get(trait_name).cloned().unwrap_or_default();
            
            let trait_impl_text = trait_impl.to_string();
            let mut modified_trait_impl = trait_impl_text.clone();
            let mut methods_to_remove_from_inherent: Vec<String> = Vec::new();
            
            // Check each method in trait impl
            if let Some(impl_ast) = ast::Impl::cast((*trait_impl).clone()) {
                if let Some(assoc_list) = impl_ast.assoc_item_list() {
                    for item in assoc_list.assoc_items() {
                        if let ast::AssocItem::Fn(func) = item {
                            if let Some(name) = func.name() {
                                let method_name = name.to_string();
                                
                                // Only process if this method is declared in the trait definition
                                if !trait_def_methods.contains(&method_name) {
                                    continue;
                                }
                                
                                // Get the body
                                if let Some(body) = func.body() {
                                    let body_str = body.to_string();
                                    
                                    // Check if it's a stub delegation
                                    if is_stub_delegation(&body_str, &method_name, type_name) {
                                        // Find the real implementation in inherent impl
                                        if let Some((inherent_func, _)) = inherent_methods.get(&method_name) {
                                            if let Some(inherent_body) = inherent_func.body() {
                                                let inherent_body_str = inherent_body.to_string();
                                                let inherent_params = extract_param_list(inherent_func);
                                                let trait_params = extract_param_list(&func);
                                                
                                                // Replace both params (to get mut) and body
                                                let trait_method_text = func.syntax().to_string();
                                                let mut updated_method = trait_method_text.clone();
                                                
                                                // Replace param list if different (e.g., adding mut)
                                                if inherent_params != trait_params {
                                                    updated_method = updated_method.replace(&trait_params, &inherent_params);
                                                }
                                                
                                                // Replace body
                                                updated_method = updated_method.replace(&body_str, &inherent_body_str);
                                                
                                                modified_trait_impl = modified_trait_impl.replace(&trait_method_text, &updated_method);
                                                
                                                // Mark this method for removal from inherent impl
                                                methods_to_remove_from_inherent.push(method_name.clone());
                                                total_moved += 1;
                                            }
                                        }
                                    } else {
                                        // Not a stub - but it's in the trait, so remove duplicate from inherent impl
                                        if inherent_methods.contains_key(&method_name) {
                                            methods_to_remove_from_inherent.push(method_name.clone());
                                            total_removed += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Replace trait impl in content
            if !methods_to_remove_from_inherent.is_empty() {
                new_content = new_content.replace(&trait_impl_text, &modified_trait_impl);
            }
            
            // Remove methods from inherent impl (only those that are in the trait)
            if !methods_to_remove_from_inherent.is_empty() {
                let inherent_text = inherent.to_string();
                let mut modified_inherent = inherent_text.clone();
                
                for method_name in &methods_to_remove_from_inherent {
                    if let Some((_, method_text)) = inherent_methods.get(method_name) {
                        // Find and remove the entire method
                        let lines: Vec<&str> = modified_inherent.lines().collect();
                        let mut new_lines = Vec::new();
                        let mut skip_until_brace_count = 0;
                        let mut in_method = false;
                        let method_sig_start = format!("pub fn {}(", method_name);
                        let method_sig_start2 = format!("fn {}(", method_name);
                        let mut skip_doc_comments = false;
                        
                        for (i, line) in lines.iter().enumerate() {
                            // Check if we're about to hit the method
                            if (line.contains(&method_sig_start) || line.contains(&method_sig_start2)) && !in_method {
                                in_method = true;
                                skip_until_brace_count = 0;
                                // Also remove doc comments immediately before
                                while !new_lines.is_empty() {
                                    let last: &&str = new_lines.last().unwrap();
                                    if last.trim().starts_with("///") || last.trim().is_empty() {
                                        new_lines.pop();
                                    } else {
                                        break;
                                    }
                                }
                            }
                            
                            if in_method {
                                // Count braces to know when method ends
                                for c in line.chars() {
                                    if c == '{' {
                                        skip_until_brace_count += 1;
                                    } else if c == '}' {
                                        skip_until_brace_count -= 1;
                                        if skip_until_brace_count == 0 {
                                            in_method = false;
                                            break;
                                        }
                                    }
                                }
                                // Skip this line (it's part of the method to remove)
                            } else {
                                new_lines.push(*line);
                            }
                        }
                        
                        modified_inherent = new_lines.join("\n");
                    }
                }
                
                // Check if inherent impl is now empty (only has whitespace between braces)
                let is_empty = check_if_impl_empty(&modified_inherent);
                
                if is_empty {
                    // Remove entire inherent impl
                    new_content = new_content.replace(&inherent_text, "");
                } else {
                    // Replace with modified inherent impl
                    new_content = new_content.replace(&inherent_text, &modified_inherent);
                }
            }
        }
    }
    
    if total_moved > 0 || total_removed > 0 {
        fs::write(file_path, new_content)?;
    }
    
    Ok((total_moved + total_removed, if total_moved > 0 || total_removed > 0 { 1 } else { 0 }))
}

fn check_if_impl_empty(impl_text: &str) -> bool {
    // Check if impl only has whitespace between the braces
    if let Some(start) = impl_text.find('{') {
        if let Some(end) = impl_text.rfind('}') {
            let between = &impl_text[start+1..end];
            return between.trim().is_empty();
        }
    }
    false
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
    let mut total_methods_fixed = 0;
    
    for path in &files {
        total_files_processed += 1;
        
        match process_file(path) {
            Ok((methods_fixed, _)) => {
                if methods_fixed > 0 {
                    total_files_modified += 1;
                    total_methods_fixed += methods_fixed;
                    println!("{}: fixed {} stub delegations", path.display(), methods_fixed);
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
    println!("  Stub delegations fixed: {}", total_methods_fixed);
    println!("Completed in {}ms", start.elapsed().as_millis());
    
    if total_files_modified > 0 {
        std::process::exit(1);
    }
}
