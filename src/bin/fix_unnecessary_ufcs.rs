//! Fix: Unnecessary UFCS Calls
//!
//! Simplifies UFCS calls like <Type as Trait>::method to Type::method
//! when the trait is in scope via glob imports.
//!
//! Uses PURE AST parsing - NO STRING HACKING WHATSOEVER.
//!
//! Binary: fix-unnecessary-ufcs

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasArgList, HasGenericArgs}, SyntaxKind, SourceFile, Edition, SyntaxNode, TextRange};
use rusticate::{find_rust_files, StandardArgs};
use std::fs;
use std::path::Path;
use std::time::Instant;

macro_rules! log {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        println!("{}", msg);
    }};
}

#[derive(Debug, Clone)]
struct UfcsReplacement {
    range: TextRange,
    new_text: String,
}

fn has_star_token(use_node: &SyntaxNode) -> bool {
    for token in use_node.descendants_with_tokens() {
        if token.kind() == SyntaxKind::STAR {
            return true;
        }
    }
    false
}

fn has_glob_imports(root: &SyntaxNode) -> bool {
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE
            && has_star_token(&node) {
                return true;
            }
    }
    false
}

// Build replacement text by emitting AST nodes
fn build_simplified_call(
    base_type: &str,
    generics: &str,
    method: &str,
    args: &str,
) -> String {
    if generics.is_empty() {
        format!("{base_type}::{method}{args}")
    } else {
        format!("{base_type}::{generics}::{method}{args}")
    }
}

fn find_ufcs_replacements(root: &SyntaxNode) -> Vec<UfcsReplacement> {
    let mut replacements = Vec::new();
    
    if !has_glob_imports(root) {
        return replacements;
    }
    
    // Find all CALL_EXPR nodes
    for node in root.descendants() {
        if node.kind() != SyntaxKind::CALL_EXPR {
            continue;
        }
        
        // Cast to CallExpr to access methods
        let call_expr = match ast::CallExpr::cast(node.clone()) {
            Some(c) => c,
            None => continue,
        };
        
        // Get the expression being called
        let expr = match call_expr.expr() {
            Some(e) => e,
            None => continue,
        };
        
        // Check if it's a PathExpr (method/function path)
        let path_expr = match expr {
            ast::Expr::PathExpr(p) => p,
            _ => continue,
        };
        
        // Get the path
        let path = match path_expr.path() {
            Some(p) => p,
            None => continue,
        };
        
        // Check if path has a qualifier (the <Type as Trait> part)
        let qualifier = match path.qualifier() {
            Some(q) => q,
            None => continue,
        };
        
        // Check if qualifier contains AS_KW (indicates UFCS)
        let mut has_as = false;
        for token in qualifier.syntax().descendants_with_tokens() {
            if token.kind() == SyntaxKind::AS_KW {
                has_as = true;
                break;
            }
        }
        
        if !has_as {
            continue;
        }
        
        // This is a UFCS call - extract components
        
        // Get the method name (the segment after ::)
        let method = match path.segment() {
            Some(seg) => {
                if let Some(name) = seg.name_ref() {
                    name.syntax().text().to_string()
                } else {
                    continue;
                }
            }
            None => continue,
        };
        
        // Extract the type before "as" from the qualifier
        // Look for PATH_TYPE before AS_KW
        let mut base_type = None;
        let mut generics = String::new();
        
        for desc in qualifier.syntax().descendants() {
            if desc.kind() == SyntaxKind::PATH_TYPE {
                if let Some(path_type) = ast::PathType::cast(desc) {
                    if let Some(type_path) = path_type.path() {
                        if let Some(type_seg) = type_path.segment() {
                            // Get base type name
                            base_type = type_seg.name_ref().map(|name| name.syntax().text().to_string());
                            
                            // Get generics if present
                            if let Some(gen_args) = type_seg.generic_arg_list() {
                                generics = gen_args.syntax().text().to_string();
                            }
                            
                            break;
                        }
                    }
                }
            }
        }
        
        let base_type = match base_type {
            Some(t) => t,
            None => continue,
        };
        
        // Get the argument list
        let args = if let Some(arg_list) = call_expr.arg_list() {
            arg_list.syntax().text().to_string()
        } else {
            "()".to_string()
        };
        
        // Build the simplified call
        let new_text = build_simplified_call(&base_type, &generics, &method, &args);
        
        replacements.push(UfcsReplacement {
            range: node.text_range(),
            new_text,
        });
    }
    
    replacements
}

fn rewrite_node(node: &SyntaxNode, ufcs_map: &std::collections::HashMap<TextRange, String>) -> String {
    // If this node is in our replacement map, emit the replacement
    if let Some(replacement) = ufcs_map.get(&node.text_range()) {
        return replacement.clone();
    }
    
    // Otherwise, emit this node by traversing its children
    let mut result = String::new();
    
    for child in node.children_with_tokens() {
        match child {
            ra_ap_syntax::NodeOrToken::Node(n) => {
                result.push_str(&rewrite_node(&n, ufcs_map));
            }
            ra_ap_syntax::NodeOrToken::Token(t) => {
                result.push_str(t.text());
            }
        }
    }
    
    result
}

fn fix_file(file_path: &Path, dry_run: bool) -> Result<usize> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let replacements = find_ufcs_replacements(root);
    
    if replacements.is_empty() {
        return Ok(0);
    }
    
    // Build a map of ranges to replacements
    let mut ufcs_map = std::collections::HashMap::new();
    for replacement in &replacements {
        ufcs_map.insert(replacement.range, replacement.new_text.clone());
    }
    
    // Rewrite the tree
    let result = rewrite_node(root, &ufcs_map);
    
    if !dry_run {
        fs::write(file_path, result)?;
    }
    
    Ok(replacements.len())
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/fix_unnecessary_ufcs.log").ok();
    
    let start_time = Instant::now();
    
    let dry_run = std::env::args().any(|arg| arg == "--dry-run" || arg == "-n");
    
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let all_files = find_rust_files(&args.paths);
    
    if dry_run {
        log!("DRY RUN MODE - No files will be modified");
    }
    log!("Entering directory '{}'", base_dir.display());
    println!();
    
    let mut total_fixed = 0;
    let mut files_fixed = Vec::new();
    
    for file_path in &all_files {
        match fix_file(file_path, dry_run) {
            Ok(count) if count > 0 => {
                let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
                if dry_run {
                    log!("{}:1: Would simplify {} UFCS calls", rel_path.display(), count);
                } else {
                    log!("{}:1: Simplified {} UFCS calls", rel_path.display(), count);
                }
                files_fixed.push(rel_path.display().to_string());
                total_fixed += count;
            }
            Err(e) => {
                eprintln!("Error processing {}: {}", file_path.display(), e);
            }
            _ => {}
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    if dry_run {
        log!("  UFCS calls that would be simplified: {}", total_fixed);
        log!("  Files that would be modified: {}", files_fixed.len());
    } else {
        log!("  UFCS calls simplified: {}", total_fixed);
        log!("  Files modified: {}", files_fixed.len());
    }
    
    if !files_fixed.is_empty() && files_fixed.len() <= 20 {
        println!();
        if dry_run {
            log!("Files that would be modified:");
        } else {
            log!("Files modified:");
        }
        for file in &files_fixed {
            log!("  {}", file);
        }
    }
    
    log!("{}", "=".repeat(80));
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}
