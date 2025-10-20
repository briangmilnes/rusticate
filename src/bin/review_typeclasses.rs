// Copyright (C) Brian G. Milnes 2025

//! Review: Type Classes (Structs, Traits, Impls)
//! 
//! Provides detailed classification of types, traits, and implementations:
//! 
//! For each module:
//! - Lists all structs (classified as internal/external based on pub visibility)
//! - Lists all traits (classified as internal/external)
//! - Lists all impl blocks with detailed classification:
//!   * Inherent vs trait impl
//!   * For internal vs external types
//!   * Method visibility (pub/internal)
//!   * Stub delegation detection
//! 
//! Output sorted by filename for chapter ordering
//! 
//! Binary: rusticate-review-typeclasses

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasVisibility, HasName}, SyntaxKind, SourceFile, Edition};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use rusticate::{StandardArgs, find_rust_files, format_number, find_nodes};

#[derive(Debug)]
struct StructInfo {
    name: String,
    line: usize,
    _is_public: bool,
}

#[derive(Debug)]
struct EnumInfo {
    name: String,
    line: usize,
    _is_public: bool,
}

#[derive(Debug)]
struct TypeAliasInfo {
    name: String,
    line: usize,
    _is_public: bool,
}

#[derive(Debug)]
struct TraitInfo {
    name: String,
    line: usize,
    is_public: bool,
}

#[derive(Debug)]
struct FunctionInfo {
    name: String,
    line: usize,
}

#[derive(Debug)]
struct ImplInfo {
    line: usize,
    header: String,
    type_name: String,
    _trait_name: Option<String>,
    is_trait_impl: bool,
    is_type_internal: bool,
    pub_methods: Vec<String>,
    pub_functions: Vec<String>,
    internal_methods: Vec<String>,
    internal_functions: Vec<String>,
    method_bodies: std::collections::HashMap<String, String>,
    method_param_types: std::collections::HashMap<String, Vec<String>>,  // method_name -> param types (excluding self)
}

#[derive(Debug, Clone)]
struct Fix {
    line: usize,
    description: String,
    recommendation: String,
}

#[derive(Debug)]
struct ModuleAnalysis {
    file: PathBuf,
    _module_name: Option<String>,
    module_line: usize,
    structs: Vec<StructInfo>,
    enums: Vec<EnumInfo>,
    type_aliases: Vec<TypeAliasInfo>,
    traits: Vec<TraitInfo>,
    functions: Vec<FunctionInfo>,
    impls: Vec<ImplInfo>,
}

fn extract_module_info(source: &str) -> Option<(String, usize)> {
    if let Some(start) = source.find("pub mod ") {
        let rest = &source[start + 8..];
        if let Some(end) = rest.find(" {") {
            let name = rest[..end].trim().to_string();
            // Count line number
            let line = source[..start].lines().count() + 1;
            return Some((name, line));
        }
    }
    None
}

fn extract_type_name(self_ty: &ast::Type) -> String {
    let text = self_ty.syntax().text().to_string();
    text.split('<').next().unwrap_or(&text).trim().to_string()
}

fn line_number(node: &ra_ap_syntax::SyntaxNode, source: &str) -> usize {
    let offset: usize = node.text_range().start().into();
    source[..offset].lines().count() + 1
}

fn analyze_file(file_path: &Path, source: &str) -> Result<Option<ModuleAnalysis>> {
    let (module_name, module_line) = extract_module_info(source)
        .map(|(name, line)| (Some(name), line))
        .unwrap_or((None, 1));
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Ok(None);
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find all structs
    let struct_nodes = find_nodes(root, SyntaxKind::STRUCT);
    let mut structs = Vec::new();
    for node in struct_nodes {
        if let Some(struct_ast) = ast::Struct::cast(node.clone()) {
            let struct_name = node.children()
                .find(|n| n.kind() == SyntaxKind::NAME)
                .and_then(|name_node| name_node.first_token())
                .map(|t| t.text().to_string());
            
            if let Some(name) = struct_name {
                let is_public = struct_ast.visibility().is_some();
                let line = line_number(&node, source);
                structs.push(StructInfo { name, line, _is_public: is_public });
            }
        }
    }
    
    // Find all enums
    let enum_nodes = find_nodes(root, SyntaxKind::ENUM);
    let mut enums = Vec::new();
    for node in enum_nodes {
        if let Some(enum_ast) = ast::Enum::cast(node.clone()) {
            let enum_name = node.children()
                .find(|n| n.kind() == SyntaxKind::NAME)
                .and_then(|name_node| name_node.first_token())
                .map(|t| t.text().to_string());
            
            if let Some(name) = enum_name {
                let is_public = enum_ast.visibility().is_some();
                let line = line_number(&node, source);
                enums.push(EnumInfo { name, line, _is_public: is_public });
            }
        }
    }
    
    // Find all type aliases
    let type_alias_nodes = find_nodes(root, SyntaxKind::TYPE_ALIAS);
    let mut type_aliases = Vec::new();
    for node in type_alias_nodes {
        if let Some(type_alias_ast) = ast::TypeAlias::cast(node.clone()) {
            let type_name = node.children()
                .find(|n| n.kind() == SyntaxKind::NAME)
                .and_then(|name_node| name_node.first_token())
                .map(|t| t.text().to_string());
            
            if let Some(name) = type_name {
                let is_public = type_alias_ast.visibility().is_some();
                let line = line_number(&node, source);
                type_aliases.push(TypeAliasInfo { name, line, _is_public: is_public });
            }
        }
    }
    
    // Find all traits
    let trait_nodes = find_nodes(root, SyntaxKind::TRAIT);
    let mut traits = Vec::new();
    for node in trait_nodes {
        if let Some(_trait_ast) = ast::Trait::cast(node.clone()) {
            let trait_name = node.children()
                .find(|n| n.kind() == SyntaxKind::NAME)
                .and_then(|name_node| name_node.first_token())
                .map(|t| t.text().to_string());
            
            if let Some(name) = trait_name {
                let text = node.to_string();
                let is_public = text.trim_start().starts_with("pub ");
                let line = line_number(&node, source);
                traits.push(TraitInfo { name, line, is_public });
            }
        }
    }
    
    // Find all pub functions at module level (not in impl blocks)
    let fn_nodes = find_nodes(root, SyntaxKind::FN);
    let mut functions = Vec::new();
    for node in fn_nodes {
        // Check if this function is at module level (not inside an IMPL)
        let mut is_module_level = true;
        let mut parent = node.parent();
        while let Some(p) = parent {
            if p.kind() == SyntaxKind::IMPL {
                is_module_level = false;
                break;
            }
            if matches!(p.kind(), SyntaxKind::MODULE | SyntaxKind::SOURCE_FILE) {
                break;
            }
            parent = p.parent();
        }
        
        if is_module_level {
            if let Some(fn_ast) = ast::Fn::cast(node.clone()) {
                let is_public = fn_ast.visibility().is_some();
                
                if is_public {
                    if let Some(name_token) = fn_ast.name() {
                        let name = name_token.text().to_string();
                        let line = line_number(&node, source);
                        functions.push(FunctionInfo { name, line });
                    }
                }
            }
        }
    }
    
    // Identify the main struct (matches module name or module name + "S")
    let main_struct_name = module_name.as_ref().and_then(|mod_name| {
        structs.iter()
            .find(|s| s.name == *mod_name || s.name == format!("{}S", mod_name))
            .map(|s| s.name.clone())
    });
    
    // Build set of internal types (not the main struct)
    let internal_types: HashSet<String> = structs.iter()
        .filter(|s| Some(&s.name) != main_struct_name.as_ref())
        .map(|s| s.name.clone())
        .collect();
    
    // Find all impl blocks
    let impl_nodes = find_nodes(root, SyntaxKind::IMPL);
    let mut impls = Vec::new();
    
    for impl_node in impl_nodes {
        if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
            let type_name = if let Some(self_ty) = impl_ast.self_ty() {
                extract_type_name(&self_ty)
            } else {
                continue;
            };
            
            let is_type_internal = internal_types.contains(&type_name);
            let trait_name = impl_ast.trait_().map(|t| t.syntax().text().to_string());
            let is_trait_impl = trait_name.is_some();
            
            // Extract impl header (first line)
            let impl_text = impl_node.to_string();
            let header = impl_text.lines().next().unwrap_or("").trim().to_string();
            
            // Classify methods/functions by visibility and whether they have self parameter
            let mut pub_methods = Vec::new();
            let mut pub_functions = Vec::new();
            let mut internal_methods = Vec::new();
            let mut internal_functions = Vec::new();
            let mut method_bodies = std::collections::HashMap::new();
            let mut method_param_types = std::collections::HashMap::new();
            
            if let Some(assoc_item_list) = impl_ast.assoc_item_list() {
                for item in assoc_item_list.assoc_items() {
                    if let ast::AssocItem::Fn(func) = item {
                        let syntax = func.syntax();
                        let text = syntax.to_string();
                        
                        let method_name = syntax.children()
                            .find(|n| n.kind() == SyntaxKind::NAME)
                            .and_then(|name_node| name_node.first_token())
                            .map(|t| t.text().to_string())
                            .unwrap_or_else(|| "unknown".to_string());
                        
                        // Extract parameter types using AST (excluding self)
                        let mut param_types = Vec::new();
                        if let Some(param_list) = func.param_list() {
                            for param in param_list.params() {
                                if let Some(ty) = param.ty() {
                                    param_types.push(ty.syntax().to_string());
                                }
                            }
                        }
                        method_param_types.insert(method_name.clone(), param_types);
                        
                        // Extract method body (everything after the function signature)
                        let body = if let Some(body_node) = func.body() {
                            body_node.syntax().to_string()
                        } else {
                            text.clone()
                        };
                        
                        method_bodies.insert(method_name.clone(), body);
                        
                        let is_public = text.trim_start().starts_with("pub ");
                        
                        // Check if it has a self parameter (method) or not (associated function)
                        let has_self = if let Some(param_list) = func.param_list() {
                            param_list.self_param().is_some()
                        } else {
                            false
                        };
                        
                        if is_public {
                            if has_self {
                                pub_methods.push(method_name);
                            } else {
                                pub_functions.push(method_name);
                            }
                        } else {
                            if has_self {
                                internal_methods.push(method_name);
                            } else {
                                internal_functions.push(method_name);
                            }
                        }
                    }
                }
            }
            
            let line = line_number(impl_ast.syntax(), source);
            
            impls.push(ImplInfo {
                line,
                header,
                type_name,
                _trait_name: trait_name,
                is_trait_impl,
                is_type_internal,
                pub_methods,
                pub_functions,
                internal_methods,
                internal_functions,
                method_bodies,
                method_param_types,
            });
        }
    }
    
    Ok(Some(ModuleAnalysis {
        file: file_path.to_path_buf(),
        _module_name: module_name,
        module_line,
        structs,
        enums,
        type_aliases,
        traits,
        functions,
        impls,
    }))
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    println!("Entering directory '{}'", base_dir.display());
    println!();
    
    let files = find_rust_files(&args.paths);
    
    let mut all_analyses = Vec::new();
    
    for file in &files {
        let file_str = file.to_string_lossy();
        
        // Skip Types.rs, lib.rs, tests, benches, and attic
        if file_str.contains("Types.rs") 
            || file_str.contains("lib.rs")
            || file_str.contains("/tests/") 
            || file_str.contains("/benches/")
            || file_str.contains("/attic/") {
            continue;
        }
        
        let source = match fs::read_to_string(file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        
        match analyze_file(file, &source) {
            Ok(Some(analysis)) => {
                let mut rel_analysis = analysis;
                rel_analysis.file = file.strip_prefix(&base_dir)
                    .unwrap_or(file)
                    .to_path_buf();
                all_analyses.push(rel_analysis);
            }
            _ => continue,
        }
    }
    
    // Sort by filename for chapter ordering
    all_analyses.sort_by(|a, b| a.file.cmp(&b.file));
    
    // Detect stub delegation patterns
    let mut stub_delegations: HashMap<String, Vec<String>> = HashMap::new();
    for analysis in &all_analyses {
        // Group impls by type
        let mut impls_by_type: HashMap<String, Vec<&ImplInfo>> = HashMap::new();
        for impl_info in &analysis.impls {
            impls_by_type.entry(impl_info.type_name.clone())
                .or_default()
                .push(impl_info);
        }
        
        // Check for overlapping methods between inherent and trait impls
        for (_type_name, impls) in impls_by_type {
            let inherent: Vec<_> = impls.iter().filter(|i| !i.is_trait_impl).collect();
            let trait_impls: Vec<_> = impls.iter().filter(|i| i.is_trait_impl).collect();
            
            for inh in &inherent {
                let inh_methods: HashSet<_> = inh.pub_methods.iter().collect();
                for tr in &trait_impls {
                    let tr_methods: HashSet<_> = tr.pub_methods.iter().collect();
                    let overlap: Vec<String> = inh_methods.intersection(&tr_methods)
                        .map(|s| s.to_string())
                        .collect();
                    
                    if !overlap.is_empty() {
                        stub_delegations.entry(inh.type_name.clone())
                            .or_default()
                            .extend(overlap);
                    }
                }
            }
        }
    }
    
    // Report
    let mut has_issues = false;
    let mut total_bugs = 0;
    let mut total_warnings = 0;
    let mut total_oks = 0;
    let mut clean_modules = 0;
    let mut total_fixes = 0;
    let mut modules_with_no_fixes = 0;
    
    // Track issue types for Pareto analysis
    let mut bug_counts: HashMap<String, usize> = HashMap::new();
    let warn_counts: HashMap<String, usize> = HashMap::new();  // Reserved for future WARNING tracking
    
    for analysis in &all_analyses {
        let mut module_bugs = 0;
        let module_warnings = 0;  // Reserved for future WARNING labels
        let mut module_oks = 0;
        let mut module_fixes: Vec<Fix> = Vec::new();
        // Print module header
        if let Some(ref mod_name) = analysis._module_name {
            println!("{}:{}: pub mod {} {{ - OK", analysis.file.display(), analysis.module_line, mod_name);
            module_oks += 1;
        } else {
            println!("{}:{}: missing module - BUG", analysis.file.display(), analysis.module_line);
            has_issues = true;
            module_bugs += 1;
            *bug_counts.entry("missing module".to_string()).or_insert(0) += 1;
        }
        
        // Check for external data types (any pub struct, pub enum, or pub type)
        let has_pub_struct = analysis.structs.iter().any(|s| s._is_public);
        let has_pub_enum = analysis.enums.iter().any(|e| e._is_public);
        let has_pub_type_alias = analysis.type_aliases.iter().any(|t| t._is_public);
        
        if !has_pub_struct && !has_pub_enum && !has_pub_type_alias {
            println!("{}:{}:\tno pub data type (struct, enum, or type alias) - BUG", 
                analysis.file.display(), analysis.module_line);
            has_issues = true;
            module_bugs += 1;
            *bug_counts.entry("no pub data type (struct, enum, or type alias)".to_string()).or_insert(0) += 1;
            
            // Add fix recommendation based on impl patterns and actual usage
            if let Some(ref _mod_name) = analysis._module_name {
                let mut proposed_type: Option<String> = None;
                
                // Check impl patterns to propose a better type
                for impl_info in &analysis.impls {
                    if impl_info.is_trait_impl {
                        // Extract "for X" from impl header
                        if let Some(for_pos) = impl_info.header.find(" for ") {
                            let after_for = impl_info.header[for_pos + 5..].trim();
                            // Remove trailing " {" if present
                            let impl_for_type = if let Some(brace_pos) = after_for.find(" {") {
                                after_for[..brace_pos].trim()
                            } else {
                                after_for
                            };
                            
                            // Extract actual parameter types from methods (prefer this over impl type)
                            let mut found_param_type = false;
                            for (_method_name, param_types) in &impl_info.method_param_types {
                                // Get first parameter type (which may include references)
                                if let Some(first_param_type) = param_types.first() {
                                    // Add lifetime if it's a reference
                                    if first_param_type.starts_with('&') {
                                        proposed_type = Some(format!("pub type T<'a, S> = {}; // from method parameter", first_param_type));
                                    } else {
                                        proposed_type = Some(format!("pub type T<S> = {}; // from method parameter", first_param_type));
                                    }
                                    found_param_type = true;
                                    break;
                                }
                            }
                            
                            // Fallback to impl type if no parameters found
                            if !found_param_type {
                                if impl_for_type == "T" {
                                    proposed_type = Some(format!("pub type T = (); // impl for T but no method parameters found"));
                                } else if impl_for_type.contains("<") {
                                    proposed_type = Some(format!("pub type T<S> = {}; // from impl type", impl_for_type));
                                } else {
                                    proposed_type = Some(format!("pub type T = {};", impl_for_type));
                                }
                            }
                            break;
                        }
                    }
                }
                
                // If no impl found, check for pub fn to extract return/param types
                if proposed_type.is_none() && !analysis.functions.is_empty() {
                    // Parse first pub function to extract common types (N, etc.)
                    proposed_type = Some(format!("pub type T = N; // common type for algorithm modules"));
                }
                
                if let Some(recommendation) = proposed_type {
                    module_fixes.push(Fix {
                        line: analysis.module_line,
                        description: "no pub data type".to_string(),
                        recommendation,
                    });
                }
            }
        }
        
        // List structs
        if !analysis.structs.is_empty() {
            // Determine main struct (matches module name or module name + "S")
            let main_struct = analysis._module_name.as_ref().and_then(|mod_name| {
                analysis.structs.iter()
                    .find(|s| s.name == *mod_name || s.name == format!("{}S", mod_name))
                    .map(|s| &s.name)
            });
            
            for s in &analysis.structs {
                let is_main = Some(&s.name) == main_struct;
                let visibility = if is_main { "external" } else { "internal" };
                println!("{}:{}:\tstruct {} ({}) - OK", analysis.file.display(), s.line, s.name, visibility);
                module_oks += 1;
            }
        }
        
        // List enums
        if !analysis.enums.is_empty() {
            // Determine main enum (matches module name or module name + "S")
            let main_enum = analysis._module_name.as_ref().and_then(|mod_name| {
                analysis.enums.iter()
                    .find(|e| e.name == *mod_name || e.name == format!("{}S", mod_name))
                    .map(|e| &e.name)
            });
            
            for e in &analysis.enums {
                let is_main = Some(&e.name) == main_enum;
                let visibility = if is_main { "external" } else { "internal" };
                println!("{}:{}:\tenum {} ({}) - OK", analysis.file.display(), e.line, e.name, visibility);
            }
        }
        
        // List type aliases
        if !analysis.type_aliases.is_empty() {
            // Determine main type alias (matches module name or module name + "S")
            let main_type_alias = analysis._module_name.as_ref().and_then(|mod_name| {
                analysis.type_aliases.iter()
                    .find(|t| t.name == *mod_name || t.name == format!("{}S", mod_name))
                    .map(|t| &t.name)
            });
            
            for t in &analysis.type_aliases {
                let is_main = Some(&t.name) == main_type_alias;
                let visibility = if is_main {
                    "external"
                } else if t._is_public {
                    "external"
                } else {
                    "internal"
                };
                println!("{}:{}:\ttype {} ({}) - OK", analysis.file.display(), t.line, t.name, visibility);
            }
        }
        
        // Check for external trait
        let has_external_trait = analysis.traits.iter().any(|t| t.is_public);
        if !has_external_trait {
            println!("{}:{}:\tno external trait - BUG", 
                analysis.file.display(), analysis.module_line);
            has_issues = true;
            module_bugs += 1;
            *bug_counts.entry("no external trait".to_string()).or_insert(0) += 1;
        }
        
        // List traits
        if !analysis.traits.is_empty() {
            for t in &analysis.traits {
                let visibility = if t.is_public { "external" } else { "internal" };
                let label = if t.is_public { "OK" } else { "WARNING" };
                println!("{}:{}:\ttrait {} ({}) - {}", analysis.file.display(), t.line, t.name, visibility, label);
            }
        }
        
        // List pub functions at module level
        if !analysis.functions.is_empty() {
            for f in &analysis.functions {
                println!("{}:{}:\tpub fn {}", analysis.file.display(), f.line, f.name);
            }
        }
        
        // Group standard trait impls for summary
        let standard_traits = ["PartialEq", "Eq", "Debug", "Display", "Hash", "Clone", "Copy", "Default"];
        let mut standard_impls = Vec::new();
        let mut custom_impls = Vec::new();
        
        for impl_info in &analysis.impls {
            // Only consider it a standard trait impl if the trait name appears between "impl" and " for "
            // Not in generic bounds like impl<T: Clone>
            let is_standard = if let Some(for_pos) = impl_info.header.find(" for ") {
                let before_for = &impl_info.header[..for_pos];
                // Check if any standard trait appears after the last '>' (to skip generic bounds)
                let search_start = before_for.rfind('>').map(|p| p + 1).unwrap_or(0);
                let trait_section = &before_for[search_start..];
                standard_traits.iter().any(|t| trait_section.contains(t))
            } else {
                false
            };
            
            if is_standard {
                standard_impls.push(impl_info);
            } else {
                custom_impls.push(impl_info);
            }
        }
        
        // Summarize standard trait impls
        if !standard_impls.is_empty() {
            let traits: Vec<String> = standard_impls.iter()
                .filter_map(|impl_info| {
                    for trait_name in &standard_traits {
                        if impl_info.header.contains(trait_name) {
                            return Some(trait_name.to_string());
                        }
                    }
                    None
                })
                .collect();
            
            let first_line = standard_impls.first().map(|i| i.line).unwrap_or(0);
            let last_line = standard_impls.last().map(|i| i.line).unwrap_or(0);
            
            println!("{}:{}-{}:\tstandard trait impls ({}) - OK", 
                analysis.file.display(), first_line, last_line, traits.join(", "));
            module_oks += 1;
        }
        
        // List custom impls with classification
        for impl_info in &custom_impls {
            let type_vis = if impl_info.is_type_internal { "internal" } else { "external" };
            
            // Determine severity label for this impl
            let impl_label = if !impl_info.is_trait_impl {
                // Inherent impl
                if impl_info.pub_methods.is_empty() && impl_info.pub_functions.is_empty() {
                    // Only internal methods/functions - BUG
                    "BUG"
                } else if !impl_info.pub_methods.is_empty() {
                    // Has pub methods - BUG (should be in trait)
                    "BUG"
                } else {
                    // Only pub functions (constructors) - OK
                    "OK"
                }
            } else {
                // Trait impl - OK
                "OK"
            };
            
            // Print impl header with type visibility on same line
            println!("{}:{}:\t{} (for {} type) - {}", 
                analysis.file.display(), impl_info.line, impl_info.header, type_vis, impl_label);
            
            if impl_label == "BUG" {
                has_issues = true;
                module_bugs += 1;
                *bug_counts.entry("inherent impl with pub methods or only internal".to_string()).or_insert(0) += 1;
            }
            
            // Show pub methods if any
            if !impl_info.pub_methods.is_empty() {
                println!("{}:{}:\t\tpub methods: {}", 
                    analysis.file.display(), impl_info.line, impl_info.pub_methods.join(", "));
            }
            
            // Show pub functions (associated functions) if any
            if !impl_info.pub_functions.is_empty() {
                println!("{}:{}:\t\tpub functions: {}", 
                    analysis.file.display(), impl_info.line, impl_info.pub_functions.join(", "));
            }
            
            // Check for methods with unused self parameter
            let mut unused_self_methods = Vec::new();
            for method_name in impl_info.pub_methods.iter().chain(impl_info.internal_methods.iter()) {
                if let Some(body) = impl_info.method_bodies.get(method_name) {
                    // Simple heuristic: if body doesn't contain "self" at all, it's unused
                    if !body.contains("self") {
                        unused_self_methods.push(method_name.clone());
                    }
                }
            }
            
                if !unused_self_methods.is_empty() {
                for method_name in &unused_self_methods {
                    println!("{}:{}:\t\tmethod {} has unused self parameter - BUG", 
                        analysis.file.display(), impl_info.line, method_name);
                    has_issues = true;
                    module_bugs += 1;
                    *bug_counts.entry("method with unused self parameter".to_string()).or_insert(0) += 1;
                }
            }
            
            // Show internal methods/functions count if there are any (but don't list them unless there's an issue)
            let total_internal = impl_info.internal_methods.len() + impl_info.internal_functions.len();
            if total_internal > 0 && impl_info.pub_methods.is_empty() && impl_info.pub_functions.is_empty() {
                let breakdown = if impl_info.internal_methods.len() > 0 && impl_info.internal_functions.len() > 0 {
                    format!("{} internal ({} methods, {} functions)", 
                        total_internal, impl_info.internal_methods.len(), impl_info.internal_functions.len())
                } else if impl_info.internal_functions.len() > 0 {
                    format!("{} internal functions", impl_info.internal_functions.len())
                } else {
                    format!("{} internal methods", impl_info.internal_methods.len())
                };
                println!("{}:{}:\t\t{}", 
                    analysis.file.display(), impl_info.line, breakdown);
            }
            
            // Check for stub delegation
            if let Some(overlapping) = stub_delegations.get(&impl_info.type_name) {
                if impl_info.is_trait_impl {
                    println!("{}:{}:\t\t⚠ used by stubbed inherent impl: {}", 
                        analysis.file.display(), impl_info.line, overlapping.join(", "));
                    has_issues = true;
                } else if !impl_info.pub_methods.is_empty() || !impl_info.pub_functions.is_empty() {
                    println!("{}:{}:\t\t⚠ stub delegation to trait impl: {}", 
                        analysis.file.display(), impl_info.line, overlapping.join(", "));
                    has_issues = true;
                }
            }
        }
        
        // Check if there's at least one trait impl
        let has_trait_impl = analysis.impls.iter().any(|impl_info| impl_info.is_trait_impl);
        if !has_trait_impl {
            println!("{}:{}:\tno Trait impl - BUG", 
                analysis.file.display(), analysis.module_line);
            has_issues = true;
            module_bugs += 1;
            *bug_counts.entry("no Trait impl".to_string()).or_insert(0) += 1;
        }
        
        // Check for duplicate method names across all impls in this file
        #[derive(Debug, Clone)]
        struct MethodOccurrence {
            line: usize,
            type_name: String,
            is_trait_impl: bool,
            is_pub: bool,
        }
        
        let mut method_occurrences: std::collections::HashMap<String, Vec<MethodOccurrence>> = std::collections::HashMap::new();
        for impl_info in &analysis.impls {
            for method in &impl_info.pub_methods {
                method_occurrences.entry(method.clone()).or_insert_with(Vec::new).push(MethodOccurrence {
                    line: impl_info.line,
                    type_name: impl_info.type_name.clone(),
                    is_trait_impl: impl_info.is_trait_impl,
                    is_pub: true,
                });
            }
            for func in &impl_info.pub_functions {
                method_occurrences.entry(func.clone()).or_insert_with(Vec::new).push(MethodOccurrence {
                    line: impl_info.line,
                    type_name: impl_info.type_name.clone(),
                    is_trait_impl: impl_info.is_trait_impl,
                    is_pub: true,
                });
            }
            for method in &impl_info.internal_methods {
                method_occurrences.entry(method.clone()).or_insert_with(Vec::new).push(MethodOccurrence {
                    line: impl_info.line,
                    type_name: impl_info.type_name.clone(),
                    is_trait_impl: impl_info.is_trait_impl,
                    is_pub: false,
                });
            }
            for func in &impl_info.internal_functions {
                method_occurrences.entry(func.clone()).or_insert_with(Vec::new).push(MethodOccurrence {
                    line: impl_info.line,
                    type_name: impl_info.type_name.clone(),
                    is_trait_impl: impl_info.is_trait_impl,
                    is_pub: false,
                });
            }
        }
        
        // Also check module-level pub functions
        for func in &analysis.functions {
            method_occurrences.entry(func.name.clone()).or_insert_with(Vec::new).push(MethodOccurrence {
                line: func.line,
                type_name: "module".to_string(),
                is_trait_impl: false,
                is_pub: true,
            });
        }
        
        let mut duplicates: Vec<(String, Vec<MethodOccurrence>)> = method_occurrences.iter()
            .filter(|(_, occurrences)| occurrences.len() > 1)
            .map(|(name, occurrences)| (name.clone(), occurrences.clone()))
            .collect();
        
        if duplicates.is_empty() {
            println!("{}:{}:\tno duplicate method names", analysis.file.display(), analysis.module_line);
        } else {
            duplicates.sort_by(|a, b| a.0.cmp(&b.0));
            for (method_name, occurrences) in duplicates {
                // Use the first location for the line number
                let line = occurrences[0].line;
                
                // Analyze the duplication pattern
                let mut analysis_notes = Vec::new();
                
                // Check if any trait impl method is short enough to be a default trait method
                for occ in &occurrences {
                    if occ.type_name != "module" && occ.is_trait_impl {
                        // Find the method body for this occurrence
                        if let Some(impl_info) = analysis.impls.iter().find(|i| i.line == occ.line) {
                            if let Some(body) = impl_info.method_bodies.get(&method_name) {
                                if body.len() < 120 {
                                    analysis_notes.push("could be default trait method");
                                    break;
                                }
                            }
                        }
                    }
                }
                
                // Check for stub delegation or code duplication
                if occurrences.len() == 2 {
                    let bodies: Vec<String> = occurrences.iter()
                        .filter_map(|occ| {
                            if occ.type_name == "module" {
                                None
                            } else {
                                analysis.impls.iter()
                                    .find(|i| i.line == occ.line)
                                    .and_then(|impl_info| impl_info.method_bodies.get(&method_name))
                                    .map(|s| s.trim().to_string())
                            }
                        })
                        .collect();
                    
                    if bodies.len() == 2 {
                        let body1 = &bodies[0];
                        let body2 = &bodies[1];
                        
                        // Check for stub delegation patterns
                        let calls_trait = body1.contains("<Self as") || body1.contains("Self::") || body1.contains("self.");
                        let is_very_short = body1.len() < 80 && body2.len() > body1.len() * 2;
                        
                        if calls_trait && is_very_short {
                            analysis_notes.push("stub delegation");
                        } else if body1 == body2 {
                            analysis_notes.push("code duplication");
                        }
                    }
                }
                
                let details: Vec<String> = occurrences.iter().map(|occ| {
                    let visibility = if occ.is_pub { "pub" } else { "internal" };
                    let impl_type = if occ.type_name == "module" {
                        "fn".to_string()
                    } else if occ.is_trait_impl {
                        format!("trait impl {}", occ.type_name)
                    } else {
                        format!("inherent {}", occ.type_name)
                    };
                    format!("{} {}", visibility, impl_type)
                }).collect();
                
                let note_str = if !analysis_notes.is_empty() {
                    format!(" - {}", analysis_notes.join(", "))
                } else {
                    String::new()
                };
                
                println!("{}:{}:\tduplicate method: {} [{}]{} - BUG", 
                    analysis.file.display(), line, method_name, details.join(", "), note_str);
                has_issues = true;
                module_bugs += 1;
                *bug_counts.entry("duplicate method".to_string()).or_insert(0) += 1;
            }
        }
        
        // Show fix recommendations
        if !module_fixes.is_empty() {
            println!("{}:{}:\tRECOMMENDED FIXES:", analysis.file.display(), analysis.module_line);
            for fix in &module_fixes {
                println!("{}:{}:\t  {} -> {}", 
                    analysis.file.display(), fix.line, fix.description, fix.recommendation);
            }
        } else {
            println!("{}:{}:\tno fixes known", analysis.file.display(), analysis.module_line);
        }
        
        // Per-module summary
        println!("{}:{}:\tModule summary: {} BUGs, {} WARNINGs, {} OKs, {} fixes", 
            analysis.file.display(), analysis.module_line,
            format_number(module_bugs), format_number(module_warnings), 
            format_number(module_oks), format_number(module_fixes.len()));
        
        // Aggregate counts
        total_bugs += module_bugs;
        total_warnings += module_warnings;
        total_oks += module_oks;
        total_fixes += module_fixes.len();
        
        if module_bugs == 0 && module_warnings == 0 {
            clean_modules += 1;
        }
        
        if module_fixes.is_empty() {
            modules_with_no_fixes += 1;
        }
        
        println!();
    }
    
    println!("{}", "=".repeat(80));
    println!("SUMMARY:");
    println!("  Total modules analyzed: {}", format_number(all_analyses.len()));
    println!("  Clean modules (no bugs or warnings): {}", format_number(clean_modules));
    println!("  Modules with no known fixes: {}", format_number(modules_with_no_fixes));
    println!("  Total OKs: {}", format_number(total_oks));
    println!("  Total WARNINGs: {}", format_number(total_warnings));
    println!("  Total BUGs: {}", format_number(total_bugs));
    println!("  Total fixes recommended: {}", format_number(total_fixes));
    
    // Pareto analysis
    println!();
    println!("{}", "=".repeat(80));
    println!("PARETO ANALYSIS: BUGS");
    println!("{}", "=".repeat(80));
    
    if !bug_counts.is_empty() {
        let mut bug_vec: Vec<_> = bug_counts.iter().collect();
        bug_vec.sort_by(|a, b| b.1.cmp(a.1));
        
        let mut cumulative = 0;
        for (issue_type, count) in &bug_vec {
            cumulative += **count;
            let percentage = (**count as f64 / total_bugs as f64) * 100.0;
            let cumulative_pct = (cumulative as f64 / total_bugs as f64) * 100.0;
            println!("{:6} ({:5.1}%, cumulative {:5.1}%): {}", 
                format_number(**count), percentage, cumulative_pct, issue_type);
        }
        println!("{}", "-".repeat(80));
        println!("TOTAL BUGS: {}", format_number(total_bugs));
    }
    
    if !warn_counts.is_empty() {
        println!();
        println!("{}", "=".repeat(80));
        println!("PARETO ANALYSIS: WARNINGS");
        println!("{}", "=".repeat(80));
        
        let mut warn_vec: Vec<_> = warn_counts.iter().collect();
        warn_vec.sort_by(|a, b| b.1.cmp(a.1));
        
        let mut cumulative = 0;
        for (issue_type, count) in &warn_vec {
            cumulative += **count;
            let percentage = (**count as f64 / total_warnings as f64) * 100.0;
            let cumulative_pct = (cumulative as f64 / total_warnings as f64) * 100.0;
            println!("{:6} ({:5.1}%, cumulative {:5.1}%): {}", 
                format_number(**count), percentage, cumulative_pct, issue_type);
        }
        println!("{}", "-".repeat(80));
        println!("TOTAL WARNINGS: {}", format_number(total_warnings));
    }
    
    println!();
    println!("Completed in {}ms", start.elapsed().as_millis());
    
    if has_issues {
        std::process::exit(1);
    }
    
    Ok(())
}
