// Copyright (C) Brian G. Milnes 2025

//! Duplicate method detection
//! 
//! Detects when methods/functions with the same name appear multiple times in a module.
//! Handles expected patterns:
//! - trait + impl (normal trait implementation)
//! - trait + pub fn (functional trait pattern)
//! - Multiple impls for standard traits (Debug/Display, etc.)
//! - Multiple impls with different type parameters (IntoIterator for &T, &mut T, T)

use ra_ap_syntax::{ast::{self, AstNode, HasName, HasVisibility}, SyntaxKind, SourceFile, Edition};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use anyhow::Result;

use crate::find_nodes;

#[derive(Debug, Clone)]
pub struct MethodLocation {
    pub name: String,
    pub line: usize,
    pub location_type: String, // "trait", "impl", "pub fn"
    pub first_line: String,
    pub impl_trait: Option<String>,
    pub impl_header: Option<String>,
    pub trait_name: Option<String>, // For trait methods, which trait they belong to
}

#[derive(Debug)]
pub struct DuplicateIssue {
    pub name: String,
    pub locations: Vec<MethodLocation>,
}

fn line_number(node: &ra_ap_syntax::SyntaxNode, source: &str) -> usize {
    let offset: usize = node.text_range().start().into();
    source[..offset].lines().count() + 1
}

fn get_first_line(node: &ra_ap_syntax::SyntaxNode, source: &str) -> String {
    let start_offset: usize = node.text_range().start().into();
    let end_offset: usize = node.text_range().end().into();
    
    let text = &source[start_offset..end_offset];
    
    // Skip past any doc comments to find the actual function signature
    for line in text.lines() {
        let trimmed = line.trim();
        // Skip doc comments and empty lines
        if !trimmed.starts_with("///") && !trimmed.starts_with("//!") && 
           !trimmed.starts_with("/**") && !trimmed.starts_with("/*!") &&
           !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    
    // Fallback to first line if we can't find a signature
    text.lines().next().unwrap_or("").trim().to_string()
}

fn find_module_block(root: &ra_ap_syntax::SyntaxNode) -> Option<ra_ap_syntax::SyntaxNode> {
    for node in root.descendants() {
        if node.kind() == SyntaxKind::MODULE {
            if let Some(module) = ast::Module::cast(node.clone()) {
                if module.visibility().is_some() {
                    return Some(node);
                }
            }
        }
    }
    None
}

pub fn find_duplicate_methods(file_path: &Path) -> Result<Vec<DuplicateIssue>> {
    let source = std::fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&source, Edition::Edition2021);
    
    if !parsed.errors().is_empty() {
        return Ok(Vec::new());
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find the pub mod block
    let module_node = match find_module_block(root) {
        Some(n) => n,
        None => return Ok(Vec::new()),
    };
    
    let mut method_locations: Vec<MethodLocation> = Vec::new();
    
    // Find all trait methods
    let trait_nodes = find_nodes(&module_node, SyntaxKind::TRAIT);
    for trait_node in trait_nodes {
        if let Some(trait_ast) = ast::Trait::cast(trait_node.clone()) {
            // Get the trait name
            let trait_name = trait_ast.name().map(|n| n.text().to_string());
            
            let fn_nodes = find_nodes(&trait_node, SyntaxKind::FN);
            for fn_node in fn_nodes {
                if let Some(fn_ast) = ast::Fn::cast(fn_node.clone()) {
                    if let Some(name_node) = fn_ast.name() {
                        let name = name_node.text().to_string();
                        let line = line_number(&fn_node, &source);
                        let first_line = get_first_line(&fn_node, &source);
                        method_locations.push(MethodLocation {
                            name,
                            line,
                            location_type: "trait".to_string(),
                            first_line,
                            impl_trait: None,
                            impl_header: None,
                            trait_name: trait_name.clone(),
                        });
                    }
                }
            }
        }
    }
    
    // Find all impl methods/functions
    let impl_nodes = find_nodes(&module_node, SyntaxKind::IMPL);
    for impl_node in impl_nodes {
        if let Some(_impl_ast) = ast::Impl::cast(impl_node.clone()) {
            // Extract the full impl header (first line)
            let full_impl_text = impl_node.to_string();
            let impl_header_line = full_impl_text.lines().next().unwrap_or("").trim().to_string();
            
            // Extract trait name if this is a trait impl
            let impl_trait_name = if let Some(for_pos) = impl_header_line.find(" for ") {
                let before_for = &impl_header_line[..for_pos];
                let after_impl = before_for.trim_start_matches("impl").trim();
                let trait_part = if let Some(gt_pos) = after_impl.rfind('>') {
                    &after_impl[gt_pos + 1..]
                } else {
                    after_impl
                };
                Some(trait_part.trim().to_string())
            } else {
                None
            };
            
            let fn_nodes = find_nodes(&impl_node, SyntaxKind::FN);
            for fn_node in fn_nodes {
                if let Some(fn_ast) = ast::Fn::cast(fn_node.clone()) {
                    if let Some(name_node) = fn_ast.name() {
                        let name = name_node.text().to_string();
                        let line = line_number(&fn_node, &source);
                        let first_line = get_first_line(&fn_node, &source);
                        method_locations.push(MethodLocation {
                            name,
                            line,
                            location_type: "impl".to_string(),
                            first_line,
                            impl_trait: impl_trait_name.clone(),
                            impl_header: Some(impl_header_line.clone()),
                            trait_name: None,
                        });
                    }
                }
            }
        }
    }
    
    // Find all standalone pub functions at module level
    let fn_nodes = find_nodes(&module_node, SyntaxKind::FN);
    for fn_node in fn_nodes {
        // Check if this function is at module level
        let mut is_module_level = true;
        let mut parent = fn_node.parent();
        while let Some(p) = parent {
            if p.kind() == SyntaxKind::IMPL || p.kind() == SyntaxKind::TRAIT {
                is_module_level = false;
                break;
            }
            if p == module_node {
                break;
            }
            parent = p.parent();
        }
        
        if is_module_level {
            if let Some(fn_ast) = ast::Fn::cast(fn_node.clone()) {
                if fn_ast.visibility().is_some() {
                    if let Some(name_node) = fn_ast.name() {
                        let name = name_node.text().to_string();
                        let line = line_number(&fn_node, &source);
                        let first_line = get_first_line(&fn_node, &source);
                        method_locations.push(MethodLocation {
                            name,
                            line,
                            location_type: "pub fn".to_string(),
                            first_line,
                            impl_trait: None,
                            impl_header: None,
                            trait_name: None,
                        });
                    }
                }
            }
        }
    }
    
    // Group by name and find duplicates
    let mut name_groups: HashMap<String, Vec<MethodLocation>> = HashMap::new();
    for loc in method_locations {
        name_groups.entry(loc.name.clone()).or_insert_with(Vec::new).push(loc);
    }
    
    let mut issues = Vec::new();
    let standard_traits = ["Debug", "Display", "Clone", "Copy", "PartialEq", "Eq", "Hash", "Default"];
    
    for (name, locations) in name_groups {
        if locations.len() > 1 {
            let _has_trait = locations.iter().any(|l| l.location_type == "trait");
            let has_impl = locations.iter().any(|l| l.location_type == "impl");
            let has_pub_fn = locations.iter().any(|l| l.location_type == "pub fn");
            
            // Check if all impl locations are for standard traits
            let impl_locs: Vec<_> = locations.iter().filter(|l| l.location_type == "impl").collect();
            let all_standard_trait_impls = !impl_locs.is_empty() && impl_locs.iter().all(|l| {
                l.impl_trait.as_ref().map(|t| standard_traits.contains(&t.as_str())).unwrap_or(false)
            });
            
            // Flag as bug if we have impl + pub fn (with or without trait)
            // Exception: if all impls are for standard traits, it might be OK
            if has_impl && has_pub_fn && !all_standard_trait_impls {
                issues.push(DuplicateIssue {
                    name: name.clone(),
                    locations: locations.clone(),
                });
            }
            // Flag if there are multiple impls - unless they're all for standard traits
            // OR they have different impl headers (like IntoIterator for &T, &mut T, T)
            else if impl_locs.len() > 1 && !all_standard_trait_impls {
                // Check if all impl headers are different
                let impl_headers: Vec<_> = impl_locs.iter()
                    .filter_map(|l| l.impl_header.as_ref())
                    .collect();
                let unique_headers: HashSet<_> = impl_headers.iter().collect();
                
                // Only flag if there are duplicate impl headers (same trait for same type)
                if unique_headers.len() < impl_headers.len() {
                    issues.push(DuplicateIssue {
                        name: name.clone(),
                        locations: locations.clone(),
                    });
                }
            }
            // Flag if there are multiple trait method definitions
            // But only if they're from the SAME trait (which would be a parse error)
            // Methods with same name in DIFFERENT traits are OK (e.g., EntryTrait::delete vs ParaHashTableTrait::delete)
            else if locations.iter().filter(|l| l.location_type == "trait").count() > 1 {
                let trait_method_locs: Vec<_> = locations.iter().filter(|l| l.location_type == "trait").collect();
                let trait_names: HashSet<_> = trait_method_locs.iter()
                    .filter_map(|l| l.trait_name.as_ref())
                    .collect();
                
                // Only flag if multiple methods from the SAME trait (or unknown trait)
                if trait_names.len() < trait_method_locs.len() {
                    issues.push(DuplicateIssue {
                        name: name.clone(),
                        locations: locations.clone(),
                    });
                }
            }
            // trait + pub fn (no impl) is OK - functional trait pattern
            // Multiple impl for different standard traits is OK - Debug/Display/etc.
        }
    }
    
    Ok(issues)
}

