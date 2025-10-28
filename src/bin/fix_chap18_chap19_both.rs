//! Fix: Remove Chap18 imports when file also imports Chap19
//!
//! Files that import both Chap18 and Chap19 create trait ambiguity.
//! This tool removes Chap18 imports when Chap19 is also imported,
//! since Chap19 re-exports everything needed from Chap18.
//!
//! Uses AST parsing - NO STRING HACKING.
//!
//! Binary: rusticate-fix-chap18-chap19-both

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition, TextRange};
use rusticate::{find_rust_files, StandardArgs};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn extract_binary_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn path_starts_with_chapter(use_item: &ast::Use, chap: &str) -> bool {
    if let Some(use_tree) = use_item.use_tree() {
        if let Some(path) = use_tree.path() {
            if let Some(first_segment) = path.segments().next() {
                return first_segment.to_string() == chap;
            }
        }
    }
    false
}

fn has_chap_import(content: &str, chap: &str) -> bool {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                if path_starts_with_chapter(&use_item, chap) {
                    return true;
                }
            }
        }
    }
    
    false
}

fn path_contains_name(use_item: &ast::Use, name: &str) -> bool {
    if let Some(use_tree) = use_item.use_tree() {
        // Check the path segments
        if let Some(path) = use_tree.path() {
            for segment in path.segments() {
                if segment.to_string() == name {
                    return true;
                }
            }
        }
        // Also check the use tree list if it's a grouped import
        for child in use_tree.syntax().descendants() {
            if child.kind() == SyntaxKind::NAME_REF
                && child.text() == name {
                    return true;
                }
        }
    }
    false
}

fn has_chap18_redefinable_trait_import(content: &str) -> bool {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                if path_starts_with_chapter(&use_item, "Chap18") && 
                   path_contains_name(&use_item, "RedefinableTrait") {
                    return true;
                }
            }
        }
    }
    
    false
}

fn extract_module_name_after_chapter(use_item: &ast::Use, chap: &str) -> Option<String> {
    if let Some(use_tree) = use_item.use_tree() {
        if let Some(path) = use_tree.path() {
            let segments: Vec<_> = path.segments().collect();
            // Look for the chapter segment and return the next one
            for (i, segment) in segments.iter().enumerate() {
                if segment.to_string() == chap {
                    if let Some(next_segment) = segments.get(i + 1) {
                        return Some(next_segment.to_string());
                    }
                }
            }
        }
    }
    None
}

fn imports_different_modules(content: &str) -> bool {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut chap18_modules = Vec::new();
    let mut chap19_modules = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                // Extract module name after Chap18:: or Chap19::
                if let Some(module) = extract_module_name_after_chapter(&use_item, "Chap18") {
                    chap18_modules.push(module);
                }
                
                if let Some(module) = extract_module_name_after_chapter(&use_item, "Chap19") {
                    chap19_modules.push(module);
                }
            }
        }
    }
    
    // Check if any Chap18 module is NOT in Chap19 modules
    for chap18_mod in &chap18_modules {
        if !chap19_modules.contains(chap18_mod) {
            return true;
        }
    }
    
    false
}

fn extract_type_from_path_type(node: &ra_ap_syntax::SyntaxNode) -> Option<String> {
    // PATH_TYPE should have a PATH child with the type name
    for child in node.children() {
        if child.kind() == SyntaxKind::PATH {
            if let Some(path) = ast::Path::cast(child) {
                // Get all segments and reconstruct the type path
                let mut segments = Vec::new();
                let mut current = Some(path);
                while let Some(p) = current {
                    if let Some(segment) = p.segment() {
                        if let Some(name) = segment.name_ref() {
                            let mut seg_text = name.text().to_string();
                            // Include generic args if present
                            for seg_child in segment.syntax().children() {
                                if seg_child.kind() == SyntaxKind::GENERIC_ARG_LIST {
                                    seg_text.push_str(&seg_child.to_string());
                                    break;
                                }
                            }
                            segments.push(seg_text);
                        }
                    }
                    current = p.qualifier();
                }
                segments.reverse();
                return Some(segments.join("::"));
            }
        }
    }
    None
}

fn simplify_ufcs_call(node: &ra_ap_syntax::SyntaxNode) -> Option<String> {
    // Use AST to parse UFCS: <Type as Trait>::function
    // Return: Type::function
    
    
    
    // Must be a PATH_EXPR node
    if node.kind() != SyntaxKind::PATH_EXPR {
        return None;
    }
    
    let path_expr = ast::PathExpr::cast(node.clone())?;
    let path = path_expr.path()?;
    
    // Get the qualifier - for UFCS this should contain the <Type as Trait> part
    let qualifier = path.qualifier()?;
    
    // The qualifier syntax should have a PATH_TYPE node for the <Type as Trait> part
    let qualifier_syntax = qualifier.syntax();
    
    // Look for PATH_TYPE in the qualifier's children or descendants
    let mut type_name = None;
    for child in qualifier_syntax.children() {
        if child.kind() == SyntaxKind::PATH_TYPE {
            type_name = extract_type_from_path_type(&child);
            break;
        }
    }
    
    if type_name.is_none() {
        for desc in qualifier_syntax.descendants() {
            if desc.kind() == SyntaxKind::PATH_TYPE {
                type_name = extract_type_from_path_type(&desc);
                break;
            }
        }
    }
    
    let type_name = type_name?;
    
    // Get the final segment (the function/method name)
    let segment = path.segment()?;
    let name_ref = segment.name_ref()?;
    let function_name = name_ref.text();
    
    // Check if there are generic arguments on the function
    let mut generic_args = String::new();
    for seg_child in segment.syntax().children() {
        if seg_child.kind() == SyntaxKind::GENERIC_ARG_LIST {
            generic_args = seg_child.to_string();
            break;
        }
    }
    
    Some(format!("{type_name}::{function_name}{generic_args}"))
}

fn remove_chap18_imports_and_simplify_ufcs(content: &str) -> Option<String> {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Check if file has any Chap18/Chap19 imports
    let has_chap18 = has_chap_import(content, "Chap18");
    let has_chap19 = has_chap_import(content, "Chap19");
    
    // Determine if we should remove Chap18 imports
    // (only if both Chap18 and Chap19 are imported and it's safe to do so)
    let should_remove_chap18 = has_chap18 && has_chap19 
        && !has_chap18_redefinable_trait_import(content) 
        && !imports_different_modules(content);
    
    // Collect all transformations
    #[derive(Debug)]
    struct Replacement {
        range: TextRange,
        new_text: String,
    }
    let mut replacements: Vec<Replacement> = Vec::new();
    
    // Collect Chap18 USE statements to delete
    if should_remove_chap18 {
        for node in root.descendants() {
            if node.kind() == SyntaxKind::USE {
                if let Some(use_item) = ast::Use::cast(node.clone()) {
                    if path_starts_with_chapter(&use_item, "Chap18") {
                        replacements.push(Replacement {
                            range: use_item.syntax().text_range(),
                            new_text: String::new(),  // Delete
                        });
                    }
                }
            }
        }
    }
    
    // NOTE: We don't simplify UFCS calls here because not all UFCS calls can be safely
    // simplified. Some methods are trait-only and require UFCS syntax. With the stubbing
    // architecture, only methods that are re-declared in Chap19's trait can use simple syntax.
    // Blindly simplifying all UFCS calls breaks compilation.
    
    if replacements.is_empty() {
        return None;
    }
    
    // Sort ranges in reverse order to maintain valid offsets
    replacements.sort_by(|a, b| b.range.start().cmp(&a.range.start()));
    
    // Apply replacements
    let mut result = content.to_string();
    for repl in replacements {
        let start: usize = repl.range.start().into();
        let end: usize = repl.range.end().into();
        
        if repl.new_text.is_empty() {
            // Deletion - also remove the newline after the use statement if present
            let end_with_newline = if end < result.len() && result.as_bytes()[end] == b'\n' {
                end + 1
            } else {
                end
            };
            result.replace_range(start..end_with_newline, "");
        } else {
            // Replacement
            result.replace_range(start..end, &repl.new_text);
        }
    }
    
    Some(result)
}

fn main() -> Result<()> {
    
    // Setup logging to analyses/fix_chap18_chap19_both.log
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/fix_chap18_chap19_both.log").ok();
    
    #[allow(unused_macros)]
    macro_rules! log {
        ($($arg:tt)*) => {{
            let msg = format!($($arg)*);
            println!("{}", msg);
            if let Some(ref mut f) = _log_file {
                use std::io::Write;
                let _ = writeln!(f, "{}", msg);
            }
        }};
    }
    
    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let all_files: Vec<PathBuf> = find_rust_files(&args.paths)
        .into_iter()
        // Skip Chap18 and Chap19 implementation directories
        .filter(|path| {
            !path.to_str().is_some_and(|s| s.contains("/Chap18/") || s.contains("/Chap19/"))
        })
        .collect();
    
    log!("Entering directory '{}'", std::env::current_dir()?.display());
    println!();
    
    let mut fixed_count = 0;
    let mut files_fixed = Vec::new();
    
    for file_path in &all_files {
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        if let Some(new_content) = remove_chap18_imports_and_simplify_ufcs(&content) {
            // Write the fixed content
            if let Err(e) = fs::write(file_path, new_content) {
                eprintln!("Error writing {}: {}", file_path.display(), e);
                continue;
            }
            
            let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
            log!("{}:1: Fixed (removed Chap18 imports and/or simplified UFCS calls)", rel_path.display());
            files_fixed.push(rel_path.display().to_string());
            fixed_count += 1;
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Files fixed: {}", fixed_count);
    
    if fixed_count > 0 {
        println!();
        log!("Files modified:");
        for file in &files_fixed {
            log!("  {}", file);
        }
    }
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

