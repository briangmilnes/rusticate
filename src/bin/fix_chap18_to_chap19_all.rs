//! Fix: Convert Chap18 Eph imports to Chap19 for files in Chap20+
//!
//! Files in Chap20+ should use Chap19 for ephemeral (Eph) types.
//! Persistent (Per) types stay with Chap18.
//! 
//! This tool converts Chap18 imports to Chap19 ONLY for:
//! - *Eph types (ArraySeqMtEph, ArraySeqStEph, etc.)
//! 
//! Leaves unchanged:
//! - *Per types (stay with Chap18)
//! - Base types without Eph/Per suffix (stay with Chap18)
//!
//! Uses AST parsing - NO STRING HACKING.
//!
//! Binary: fix-chap18-to-chap19-all

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasName}, SyntaxKind, SourceFile, Edition};
use rusticate::{find_rust_files, StandardArgs};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn is_chap20_or_higher(file_path: &Path) -> bool {
    if let Some(parent) = file_path.parent() {
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

fn has_eph_in_use_path(use_node: &ast::Use) -> bool {
    // Check if any NAME_REF in the path ends with "Eph"
    for node in use_node.syntax().descendants() {
        if node.kind() == SyntaxKind::NAME_REF {
            if let Some(name_ref) = ast::NameRef::cast(node) {
                let text = name_ref.text();
                if text.ends_with("Eph") {
                    return true;
                }
            }
        }
    }
    false
}

fn convert_chap18_to_chap19(content: &str, file_path: &Path) -> Option<String> {
    // Only process files in Chap20+
    if !is_chap20_or_higher(file_path) {
        return None;
    }
    
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut replacements = Vec::new();
    
    // Find all NAME_REF nodes that are "Chap18" in the USE tree
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_stmt) = ast::Use::cast(node.clone()) {
                // Only convert if this USE statement imports an Eph type
                if has_eph_in_use_path(&use_stmt) {
                    // Within this USE statement, find all NAME_REF nodes
                    for use_descendant in use_stmt.syntax().descendants() {
                        if use_descendant.kind() == SyntaxKind::NAME_REF {
                            if let Some(name_ref) = ast::NameRef::cast(use_descendant.clone()) {
                                if name_ref.text() == "Chap18" {
                                    // Replace this specific NAME_REF with Chap19
                                    replacements.push((name_ref.syntax().text_range(), "Chap19".to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    if replacements.is_empty() {
        return None;
    }
    
    // Sort replacements in reverse order to maintain valid offsets
    replacements.sort_by(|a, b| b.0.start().cmp(&a.0.start()));
    
    // Apply replacements
    let mut result = content.to_string();
    for (range, new_text) in replacements {
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        result.replace_range(start..end, &new_text);
    }
    
    Some(result)
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/fix_chap18_to_chap19_all.log").ok();
    
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
    let all_files = find_rust_files(&args.paths);
    
    log!("Entering directory '{}'", std::env::current_dir()?.display());
    println!();
    
    let mut fixed_count = 0;
    let mut files_fixed = Vec::new();
    
    for file_path in &all_files {
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        if let Some(new_content) = convert_chap18_to_chap19(&content, file_path) {
            // Write the fixed content
            if let Err(e) = fs::write(file_path, new_content) {
                eprintln!("Error writing {}: {}", file_path.display(), e);
                continue;
            }
            
            let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
            log!("{}:1: Converted Chap18 -> Chap19", rel_path.display());
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

