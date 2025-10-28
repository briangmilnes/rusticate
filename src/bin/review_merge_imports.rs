use anyhow::Result;
use ra_ap_syntax::{ast, AstNode, SourceFile, SyntaxKind};
use rusticate::{find_rust_files, StandardArgs};
use std::fs;
use std::path::Path;

macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_merge_imports.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

fn get_line_number(offset: usize, content: &str) -> usize {
    content[..offset].lines().count()
}

/// Extract the base path from a use statement (everything except the final item)
/// e.g., "std::fmt::Display" -> "std::fmt"
///       "crate::module::Type" -> "crate::module"
fn extract_base_path(use_stmt: &ast::Use) -> Option<String> {
    if let Some(use_tree) = use_stmt.use_tree() {
        if let Some(path) = use_tree.path() {
            // Get all segments except the last one
            let segments: Vec<_> = path.segments().collect();
            if segments.len() >= 2 {
                // Reconstruct path without the last segment
                let base_segments: Vec<_> = segments.iter().take(segments.len() - 1).collect();
                let base_path = base_segments.iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                return Some(base_path);
            }
        }
    }
    None
}

/// Check if a use statement has an alias (as clause)
fn has_alias(use_stmt: &ast::Use) -> bool {
    if let Some(use_tree) = use_stmt.use_tree() {
        if use_tree.rename().is_some() {
            return true;
        }
    }
    false
}

/// Extract the final item from a use statement
/// e.g., "std::fmt::Display" -> "Display"
fn extract_final_item(use_stmt: &ast::Use) -> Option<String> {
    if let Some(use_tree) = use_stmt.use_tree() {
        if let Some(path) = use_tree.path() {
            if let Some(segment) = path.segments().last() {
                return Some(segment.to_string());
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
struct UseStatementInfo {
    line: usize,
    base_path: String,
    item: String,
    full_text: String,
}

fn is_glob_import(use_stmt: &ast::Use) -> bool {
    if let Some(use_tree) = use_stmt.use_tree() {
        use_tree.syntax().descendants_with_tokens()
            .any(|n| n.kind() == SyntaxKind::STAR)
    } else {
        false
    }
}

fn is_grouped_import(use_stmt: &ast::Use) -> bool {
    if let Some(use_tree) = use_stmt.use_tree() {
        use_tree.syntax().descendants()
            .any(|n| n.kind() == SyntaxKind::USE_TREE_LIST)
    } else {
        false
    }
}

fn analyze_file(file_path: &Path) -> Result<Vec<Vec<UseStatementInfo>>> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, ra_ap_syntax::Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();

    let mut use_statements: Vec<UseStatementInfo> = Vec::new();

    // Collect all use statements with their info
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_stmt) = ast::Use::cast(node.clone()) {
                // Skip glob imports, grouped imports, and aliased imports
                if is_glob_import(&use_stmt) || is_grouped_import(&use_stmt) {
                    continue;
                }
                
                // Skip imports with aliases (e.g., "use std::fmt::Result as FmtResult")
                if has_alias(&use_stmt) {
                    continue;
                }

                let use_text = use_stmt.to_string();
                if let (Some(base_path), Some(item)) = (extract_base_path(&use_stmt), extract_final_item(&use_stmt)) {
                    let line = get_line_number(node.text_range().start().into(), &content);
                    use_statements.push(UseStatementInfo {
                        line,
                        base_path,
                        item,
                        full_text: use_text.trim().to_string(),
                    });
                }
            }
        }
    }

    // Group consecutive use statements by base path
    let mut groupable: Vec<Vec<UseStatementInfo>> = Vec::new();
    let mut current_group: Vec<UseStatementInfo> = Vec::new();
    let mut last_base_path: Option<String> = None;
    let mut last_line: Option<usize> = None;

    for use_info in use_statements {
        let is_consecutive = if let Some(prev_line) = last_line {
            use_info.line <= prev_line + 1
        } else {
            true
        };

        if Some(&use_info.base_path) == last_base_path.as_ref() && is_consecutive {
            // Same base path and consecutive lines - add to current group
            current_group.push(use_info.clone());
        } else {
            // Different base path or non-consecutive - start new group
            if current_group.len() >= 2 {
                groupable.push(current_group.clone());
            }
            current_group = vec![use_info.clone()];
            last_base_path = Some(use_info.base_path.clone());
        }
        last_line = Some(use_info.line);
    }

    // Don't forget the last group
    if current_group.len() >= 2 {
        groupable.push(current_group);
    }

    Ok(groupable)
}

fn review_file(file_path: &Path) -> Result<usize> {
    let groups = analyze_file(file_path)?;
    
    if groups.is_empty() {
        return Ok(0);
    }

    log!("\n{}:", file_path.display());
    let mut total_groupable = 0;

    for group in groups {
        if group.len() < 2 {
            continue;
        }

        let base_path = &group[0].base_path;
        let items: Vec<_> = group.iter().map(|u| u.item.as_str()).collect();
        
        log!("  Line {}: {} imports from {}", 
            group[0].line, 
            group.len(),
            base_path
        );
        for use_info in &group {
            log!("    Line {}: {}", use_info.line, use_info.full_text);
        }
        log!("  -> Suggested: use {}::{{{}}};", base_path, items.join(", "));
        
        total_groupable += group.len();
    }

    Ok(total_groupable)
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let _ = fs::remove_file("analyses/review_merge_imports.log");

    let args = StandardArgs::parse()?;
    let files = find_rust_files(&args.paths);

    log!("Reviewing {} files for mergeable imports...\n", files.len());
    log!("{}", "=".repeat(80));

    let mut total_files_with_mergeable = 0;
    let mut total_mergeable_imports = 0;

    for file in &files {
        let count = review_file(file)?;
        if count > 0 {
            total_files_with_mergeable += 1;
            total_mergeable_imports += count;
        }
    }

    log!("\n{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Files analyzed: {}", files.len());
    log!("  Files with mergeable imports: {}", total_files_with_mergeable);
    log!("  Total imports that can be merged: {}", total_mergeable_imports);
    log!("{}", "=".repeat(80));

    Ok(())
}

