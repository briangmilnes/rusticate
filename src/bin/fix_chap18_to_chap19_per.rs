//! Fix: Convert Chap18 ArraySeqStPer to Chap19 for specific files
//!
//! Migrates ArraySeqStPer imports from Chap18 to Chap19 for files that:
//! - Are in Chap20+ 
//! - Use Chap18::ArraySeqStPer
//! - Should use Chap19::ArraySeqStPer instead
//!
//! Note: Does NOT migrate ArraySeqMtPer (no Chap19 equivalent exists)
//!
//! Uses AST parsing - NO STRING HACKING.
//!
//! Binary: fix-chap18-to-chap19-per

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition, TextRange};
use rusticate::{find_rust_files, StandardArgs};
use std::fs;
use std::path::Path;
use std::time::Instant;

macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/fix_chap18_to_chap19_per.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

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

fn has_array_seq_st_per_in_use_path(use_node: &ast::Use) -> bool {
    // Check if the use path contains "ArraySeqStPer"
    for node in use_node.syntax().descendants() {
        if node.kind() == SyntaxKind::NAME_REF {
            if let Some(name_ref) = ast::NameRef::cast(node) {
                let text = name_ref.text();
                if text == "ArraySeqStPer" {
                    return true;
                }
            }
        }
    }
    false
}

fn uses_redefinable_trait(root: &ra_ap_syntax::SyntaxNode) -> bool {
    // Check if the file uses ArraySeqStPerRedefinableTrait via AST
    for node in root.descendants() {
        if node.kind() == SyntaxKind::NAME_REF {
            if let Some(name_ref) = ast::NameRef::cast(node) {
                if name_ref.text() == "ArraySeqStPerRedefinableTrait" {
                    return true;
                }
            }
        }
    }
    false
}

fn convert_chap18_to_chap19_per(content: &str, file_path: &Path) -> Option<String> {
    // Only process files in Chap20+
    if !is_chap20_or_higher(file_path) {
        return None;
    }
    
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Skip files that use RedefinableTrait (not available in Chap19)
    if uses_redefinable_trait(root) {
        return None;
    }
    
    let mut replacements: Vec<(TextRange, String)> = Vec::new();
    
    // Find all USE statements that import ArraySeqStPer from Chap18
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_stmt) = ast::Use::cast(node.clone()) {
                // Only convert if this USE statement imports ArraySeqStPer
                if has_array_seq_st_per_in_use_path(&use_stmt) {
                    // Within this USE statement, find all NAME_REF nodes that are "Chap18"
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
    
    // Apply replacements in reverse order to maintain correct offsets
    replacements.sort_by_key(|(range, _)| range.start());
    replacements.reverse();
    
    let mut new_content = content.to_string();
    for (range, replacement) in replacements {
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        new_content.replace_range(start..end, &replacement);
    }
    
    Some(new_content)
}

fn fix_file(file_path: &Path, dry_run: bool) -> Result<bool> {
    let content = fs::read_to_string(file_path)?;
    
    if let Some(new_content) = convert_chap18_to_chap19_per(&content, file_path) {
        if dry_run {
            log!("  [DRY RUN] Would convert Chap18 -> Chap19 for ArraySeqStPer");
            return Ok(true);
        }
        
        fs::write(file_path, new_content)?;
        log!("  ✓ Converted Chap18 -> Chap19 for ArraySeqStPer");
        return Ok(true);
    }
    
    Ok(false)
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let _ = fs::remove_file("analyses/fix_chap18_to_chap19_per.log");
    
    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    let dry_run = std::env::args().any(|arg| arg == "--dry-run");
    
    let files = find_rust_files(&args.paths);
    
    log!("Processing {} files to convert Chap18::ArraySeqStPer -> Chap19::ArraySeqStPer...\n", files.len());
    if dry_run {
        log!("[DRY RUN MODE - No files will be modified]");
    }
    log!("{}", "=".repeat(80));
    
    let mut total_files_modified = 0;
    
    for file in &files {
        let modified = fix_file(file, dry_run)?;
        if modified {
            log!("{}:", file.display());
            total_files_modified += 1;
        }
    }
    
    log!("\n{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Files processed: {}", files.len());
    log!("  Files modified: {}", total_files_modified);
    log!("  Completed in {}ms", start_time.elapsed().as_millis());
    log!("{}", "=".repeat(80));
    
    Ok(())
}

