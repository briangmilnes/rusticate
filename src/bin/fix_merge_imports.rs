use anyhow::Result;
use ra_ap_syntax::{ast, AstNode, Edition, SourceFile, SyntaxKind, TextRange};
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
            .open("analyses/fix_merge_imports.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

fn get_line_number(offset: usize, content: &str) -> usize {
    content[..offset].lines().count()
}

/// Extract the base path from a use statement (everything except the final item)
fn extract_base_path(use_stmt: &ast::Use) -> Option<String> {
    if let Some(use_tree) = use_stmt.use_tree() {
        if let Some(path) = use_tree.path() {
            let segments: Vec<_> = path.segments().collect();
            if segments.len() >= 2 {
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
    range: TextRange,
    base_path: String,
    item: String,
}

/// Get the indentation of the line containing the given offset
fn get_indentation(content: &str, offset: usize) -> String {
    let line_start = content[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line = &content[line_start..];
    let indent_end = line.find(|c: char| !c.is_whitespace()).unwrap_or(0);
    line[..indent_end].to_string()
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

fn fix_file(file_path: &Path, dry_run: bool) -> Result<usize> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
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

                if let (Some(base_path), Some(item)) = (extract_base_path(&use_stmt), extract_final_item(&use_stmt)) {
                    let line = get_line_number(node.text_range().start().into(), &content);
                    use_statements.push(UseStatementInfo {
                        line,
                        range: node.text_range(),
                        base_path,
                        item,
                    });
                }
            }
        }
    }

    // Group consecutive use statements by base path
    let mut groups: Vec<Vec<UseStatementInfo>> = Vec::new();
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
            current_group.push(use_info.clone());
        } else {
            if current_group.len() >= 2 {
                groups.push(current_group.clone());
            }
            current_group = vec![use_info.clone()];
            last_base_path = Some(use_info.base_path.clone());
        }
        last_line = Some(use_info.line);
    }

    if current_group.len() >= 2 {
        groups.push(current_group);
    }

    if groups.is_empty() {
        return Ok(0);
    }

    // Create replacements for each group
    let mut replacements: Vec<(TextRange, String)> = Vec::new();
    let mut total_merged = 0;

    for group in groups {
        if group.len() < 2 {
            continue;
        }

        let base_path = &group[0].base_path;
        let items: Vec<_> = group.iter().map(|u| u.item.clone()).collect();
        
        // Get indentation from the first use statement
        let first_offset: usize = group[0].range.start().into();
        let indent = get_indentation(&content, first_offset);

        // Create merged import
        let merged = format!("{}use {}::{{{}}};", indent, base_path, items.join(", "));

        // Calculate range spanning all imports in the group, including leading whitespace
        let first_line_start = content[..first_offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let start = ra_ap_syntax::TextSize::from(first_line_start as u32);
        let end = group[group.len() - 1].range.end();
        let full_range = TextRange::new(start, end);

        replacements.push((full_range, merged));
        total_merged += group.len();

        log!("  Line {}: Merging {} imports from {}", 
            group[0].line, 
            group.len(), 
            base_path
        );
    }

    if dry_run {
        log!("  [DRY RUN] Would merge {} imports", total_merged);
        return Ok(total_merged);
    }

    // Apply replacements in reverse order to avoid offset issues
    replacements.sort_by_key(|(range, _)| range.start());
    replacements.reverse();

    let mut new_content = content.clone();
    for (range, replacement) in replacements {
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        new_content.replace_range(start..end, &replacement);
    }

    fs::write(file_path, new_content)?;
    log!("  ✓ Merged {} imports", total_merged);

    Ok(total_merged)
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let _ = fs::remove_file("analyses/fix_merge_imports.log");

    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    let dry_run = std::env::args().any(|arg| arg == "--dry-run");
    
    let files = find_rust_files(&args.paths);

    log!("Processing {} files to merge imports...\n", files.len());
    if dry_run {
        log!("[DRY RUN MODE - No files will be modified]");
    }
    log!("{}", "=".repeat(80));

    let mut total_files_modified = 0;
    let mut total_imports_merged = 0;

    for file in &files {
        let count = fix_file(file, dry_run)?;
        if count > 0 {
            log!("{}:", file.display());
            total_files_modified += 1;
            total_imports_merged += count;
        }
    }

    log!("\n{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Files processed: {}", files.len());
    log!("  Files modified: {}", total_files_modified);
    log!("  Total imports merged: {}", total_imports_merged);
    log!("  Completed in {}ms", start_time.elapsed().as_millis());
    log!("{}", "=".repeat(80));

    Ok(())
}

