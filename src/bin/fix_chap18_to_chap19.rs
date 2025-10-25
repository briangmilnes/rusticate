//! Fix: Chap18 to Chap19 imports in Mt files
//!
//! Changes Mt files in Chap20+ that import from Chap18 to use Chap19 instead.
//! Uses AST parsing to properly rewrite use statements.
//!
//! Binary: rusticate-fix-chap18-to-chap19

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

fn is_mt_file(path: &Path) -> bool {
    let path_str = path.to_str().unwrap_or("");
    path_str.contains("Mt") && !path_str.contains("/tests/") && !path_str.contains("/benches/")
}

fn should_fix_file(path: &Path) -> bool {
    // Only fix Mt files in Chap20+
    if !is_mt_file(path) {
        return false;
    }
    
    if let Some(parent) = path.parent() {
        if let Some(dir_name) = parent.file_name().and_then(|n| n.to_str()) {
            if dir_name.starts_with("Chap") {
                if let Ok(chap_num) = dir_name[4..].parse::<u32>() {
                    return chap_num >= 20;
                }
            }
        }
    }
    false
}

fn fix_chap18_imports(content: &str) -> Option<String> {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut changes: Vec<(TextRange, String)> = Vec::new();
    
    // Find all USE statements that import from Chap18
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                let use_text = use_item.to_string();
                
                if use_text.contains("Chap18::") {
                    let range = use_item.syntax().text_range();
                    let new_text = use_text.replace("Chap18::", "Chap19::");
                    changes.push((range, new_text));
                }
            }
        }
    }
    
    // Find all PATH nodes that reference Chap18 (e.g., Chap18::ArraySeqMtEph::...)
    // Also check NAME_REF nodes for macro content
    for node in root.descendants() {
        if node.kind() == SyntaxKind::PATH || node.kind() == SyntaxKind::NAME_REF {
            let path_text = node.to_string();
            
            // Skip if already processed (child of a PATH node we'll process)
            if node.kind() == SyntaxKind::NAME_REF {
                // Only process NAME_REF if it's exactly "Chap18"
                if path_text == "Chap18" {
                    let range = node.text_range();
                    let new_text = "Chap19".to_string();
                    changes.push((range, new_text));
                }
            } else {
                // For PATH nodes, check if contains Chap18::
                if path_text.contains("::Chap18::") || path_text.starts_with("Chap18::") {
                    let range = node.text_range();
                    let new_text = path_text.replace("Chap18::", "Chap19::");
                    changes.push((range, new_text));
                }
            }
        }
    }
    
    // Check for IDENT tokens containing "Chap18" in macro bodies
    for token in root.descendants_with_tokens() {
        if let Some(token_node) = token.as_token() {
            let token_text = token_node.text();
            if token_text == "Chap18" {
                let range = token_node.text_range();
                changes.push((range, "Chap19".to_string()));
            }
        }
    }
    
    // Find impl blocks and add + 'static to type parameters that need it
    for node in root.descendants() {
        if node.kind() == SyntaxKind::GENERIC_PARAM_LIST {
            let params_text = node.to_string();
            
            // Check if we have type params with trait bounds but no 'static
            if params_text.contains("StTInMtT") && !params_text.contains("'static") {
                let range = node.text_range();
                
                // Add + 'static to each type parameter
                let mut new_params = params_text.clone();
                
                // Replace patterns like "T: TraitBound" with "T: TraitBound + 'static"
                // Handle multiple patterns
                new_params = new_params.replace(": StTInMtT>", ": StTInMtT + 'static>");
                new_params = new_params.replace(": StTInMtT,", ": StTInMtT + 'static,");
                new_params = new_params.replace(": StTInMtT + ", ": StTInMtT + 'static + ");
                
                // Handle Ord and other bounds
                if new_params.contains(" Ord") && !new_params.contains("Ord + 'static") {
                    new_params = new_params.replace(" + Ord>", " + Ord + 'static>");
                    new_params = new_params.replace(" + Ord,", " + Ord + 'static,");
                    new_params = new_params.replace(" + Ord + ", " + Ord + 'static + ");
                }
                
                if new_params != params_text {
                    changes.push((range, new_params));
                }
            }
        }
    }
    
    if changes.is_empty() {
        return None;
    }
    
    // Apply changes in reverse order to maintain valid offsets
    changes.sort_by(|a, b| b.0.start().cmp(&a.0.start()));
    
    let mut result = content.to_string();
    for (range, replacement) in changes {
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        result.replace_range(start..end, &replacement);
    }
    
    Some(result)
}

fn main() -> Result<()> {
    
    // Setup logging to analyses/fix_chap18_to_chap19.log
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/fix_chap18_to_chap19.log").ok();
    
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
    
    log!("Entering directory '{}'", std::env::current_dir()?.display());
    println!();
    
    let files = find_rust_files(&args.paths);
    let mut fixed_count = 0;
    let mut files_fixed = Vec::new();
    
    for file_path in &files {
        if !should_fix_file(file_path) {
            continue;
        }
        
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        // Debug: check if file imports Chap18 using AST
        let parsed = SourceFile::parse(&content, Edition::Edition2021);
        let tree = parsed.tree();
        let root = tree.syntax();
        let mut has_chap18 = false;
        for node in root.descendants() {
            if node.kind() == SyntaxKind::USE {
                if let Some(use_item) = ast::Use::cast(node.clone()) {
                    let use_text = use_item.to_string();
                    if use_text.contains("Chap18::") {
                        has_chap18 = true;
                        break;
                    }
                }
            }
        }
        if has_chap18 {
            let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
            eprintln!("DEBUG: {} imports Chap18", rel_path.display());
        }
        
        if let Some(new_content) = fix_chap18_imports(&content) {
            // Write the fixed content
            if let Err(e) = fs::write(file_path, new_content) {
                eprintln!("Error writing {}: {}", file_path.display(), e);
                continue;
            }
            
            let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
            log!("{}:1: Changed Chap18 imports to Chap19", rel_path.display());
            files_fixed.push(rel_path.display().to_string());
            fixed_count += 1;
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Files fixed: {}", fixed_count);
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

