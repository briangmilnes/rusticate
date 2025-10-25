//! Review: Chap18/Chap19 UFCS calls that can be simplified
//!
//! Identifies UFCS calls like:
//!   <ArraySeqMtEphS<_> as ArraySeqMtEphTrait<_>>::reduce(...)
//! that can be simplified to:
//!   ArraySeqMtEphS::reduce(...)
//!
//! This is possible when using Chap19 which only exposes BaseTrait.
//!
//! Binary: rusticate-review-chap18-chap19

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition};
use rusticate::{find_rust_files, StandardArgs};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn extract_binary_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

#[derive(Debug, Clone)]
struct UfcsCall {
    line: usize,
    type_name: String,
    trait_name: String,
    method: String,
    full_text: String,
}

#[derive(Debug)]
struct FileReport {
    file: String,
    chap18_eph_imports: usize,  // Eph types from Chap18 (should use Chap19)
    chap18_per_imports: usize,  // Per types from Chap18 (correct)
    chap19_imports: usize,
    ufcs_calls: Vec<UfcsCall>,
    is_mt: bool,
}

fn is_mt_file(path: &Path) -> bool {
    let path_str = path.to_str().unwrap_or("");
    path_str.contains("Mt") && !path_str.contains("/tests/") && !path_str.contains("/benches/")
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

fn has_per_in_use_path(use_node: &ast::Use) -> bool {
    // Check if any NAME_REF in the path ends with "Per"
    for node in use_node.syntax().descendants() {
        if node.kind() == SyntaxKind::NAME_REF {
            if let Some(name_ref) = ast::NameRef::cast(node) {
                let text = name_ref.text();
                if text.ends_with("Per") {
                    return true;
                }
            }
        }
    }
    false
}

fn find_chap18_imports(content: &str) -> (usize, usize) {
    // Returns (eph_count, per_count)
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut eph_count = 0;
    let mut per_count = 0;
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                let use_text = use_item.to_string();
                if use_text.contains("Chap18::") {
                    if has_eph_in_use_path(&use_item) {
                        eph_count += 1;
                    } else if has_per_in_use_path(&use_item) {
                        per_count += 1;
                    }
                    // Note: Base types (no Eph/Per) are not counted
                }
            }
        }
    }
    
    (eph_count, per_count)
}

fn find_chap19_imports(content: &str) -> usize {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut count = 0;
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                let use_text = use_item.to_string();
                if use_text.contains("Chap19::") {
                    count += 1;
                }
            }
        }
    }
    
    count
}

fn find_ufcs_calls(content: &str, file_path: &Path) -> Vec<UfcsCall> {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut calls = Vec::new();
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    
    // Find all CALL_EXPR nodes and check their function paths
    for node in root.descendants() {
        if node.kind() == SyntaxKind::CALL_EXPR {
            // Get the function being called (first child should be the path/expr)
            if let Some(callee) = node.first_child() {
                let callee_text = callee.to_string();
                
                // Check if this is a UFCS pattern: contains "<", " as ", and "::"
                if callee_text.contains('<') && callee_text.contains(" as ") && callee_text.contains(">::") {
                    // Parse the UFCS call to extract type, trait, and method
                    if let Some((type_name, trait_name, method)) = parse_ufcs_from_text(&callee_text) {
                        // Skip "Self" patterns
                        if !type_name.starts_with("Self") && !type_name.is_empty() && !trait_name.is_empty() {
                            // Calculate line number
                            let offset = node.text_range().start();
                            let line = content[..usize::from(offset)].lines().count();
                            
                            // Deduplicate
                            let key = (line, type_name.clone(), method.clone());
                            if !seen.contains(&key) {
                                seen.insert(key);
                                
                                // Create display snippet
                                let display_text = if callee_text.len() > 60 {
                                    format!("{}...", &callee_text[..60])
                                } else {
                                    callee_text.clone()
                                };
                                
                                calls.push(UfcsCall {
                                    line,
                                    type_name,
                                    trait_name,
                                    method,
                                    full_text: display_text,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Also check macro bodies - they contain token streams that aren't fully parsed as CALL_EXPR
    // Look for MACRO_RULES nodes and scan their token trees
    for node in root.descendants() {
        if node.kind() == SyntaxKind::MACRO_RULES {
            let macro_text = node.to_string();
            let macro_offset = node.text_range().start();
            
            // Scan the macro body for UFCS patterns
            for (line_idx, line_text) in macro_text.lines().enumerate() {
                if line_text.contains('<') && line_text.contains(" as ") && line_text.contains(">::") {
                    // Find UFCS patterns in the line
                    if let Some((type_name, trait_name, method)) = extract_ufcs_from_line(line_text) {
                        // Skip "Self" patterns
                        if !type_name.starts_with("Self") && !type_name.is_empty() && !trait_name.is_empty() {
                            // Calculate line number (approximate - use macro start + line index)
                            let line_num = content[..usize::from(macro_offset)].lines().count() + line_idx;
                            
                            // Deduplicate
                            let key = (line_num, type_name.clone(), method.clone());
                            if !seen.contains(&key) {
                                seen.insert(key);
                                
                                let display_text = if line_text.len() > 60 {
                                    format!("{}...", &line_text[..60])
                                } else {
                                    line_text.trim().to_string()
                                };
                                
                                calls.push(UfcsCall {
                                    line: line_num,
                                    type_name,
                                    trait_name,
                                    method,
                                    full_text: display_text,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    
    calls
}

fn extract_ufcs_from_line(line: &str) -> Option<(String, String, String)> {
    // Find the first UFCS pattern in the line
    let start = line.find('<')?;
    let mut depth = 0;
    let mut as_pos = None;
    let mut end_pos = None;
    
    for (i, ch) in line[start..].char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    end_pos = Some(start + i);
                    break;
                }
            }
            ' ' if depth >= 1 && line[start + i..].starts_with(" as ") => {
                if as_pos.is_none() {
                    as_pos = Some(start + i);
                }
            }
            _ => {}
        }
    }
    
    let as_pos = as_pos?;
    let end_pos = end_pos?;
    
    // Check if followed by >::
    if !line[end_pos + 1..].starts_with("::") {
        return None;
    }
    
    // Extract type and trait
    let type_str = line[start + 1..as_pos].trim();
    let trait_str = line[as_pos + 4..end_pos].trim();
    
    // Extract method name
    let after_colons = &line[end_pos + 3..];
    let method_end = after_colons.find('(').unwrap_or(after_colons.len());
    let method = after_colons[..method_end].trim();
    
    Some((type_str.to_string(), trait_str.to_string(), method.to_string()))
}

fn parse_ufcs_from_text(text: &str) -> Option<(String, String, String)> {
    // Parse: <Type as Trait>::method from the callee text
    // Find the < and matching >
    let start = text.find('<')?;
    let mut depth = 0;
    let mut as_pos = None;
    let mut end_pos = None;
    
    for (i, ch) in text[start..].char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    end_pos = Some(start + i);
                    break;
                }
            }
            ' ' if depth >= 1 && text[start + i..].starts_with(" as ") => {
                if as_pos.is_none() {
                    as_pos = Some(start + i);
                }
            }
            _ => {}
        }
    }
    
    let as_pos = as_pos?;
    let end_pos = end_pos?;
    
    // Extract type and trait
    let type_str = text[start + 1..as_pos].trim();
    let trait_str = text[as_pos + 4..end_pos].trim();
    
    // Extract method name after >::
    let after_bracket = &text[end_pos + 1..];
    if let Some(method_start) = after_bracket.find("::") {
        let method_part = &after_bracket[method_start + 2..];
        // Method name ends at ( or whitespace
        let method_end = method_part.find('(').unwrap_or(method_part.len());
        let method = method_part[..method_end].trim();
        
        return Some((type_str.to_string(), trait_str.to_string(), method.to_string()));
    }
    
    None
}


fn main() -> Result<()> {
    
    // Setup logging to analyses/review_chap18_chap19.log
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/review_chap18_chap19.log").ok();
    
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
    
    let files: Vec<PathBuf> = find_rust_files(&args.paths)
        .into_iter()
        // Skip Chap18 and Chap19 implementation directories
        .filter(|path| {
            !path.to_str().map_or(false, |s| s.contains("/Chap18/") || s.contains("/Chap19/"))
        })
        .collect();
    let mut reports: Vec<FileReport> = Vec::new();
    
    for file_path in &files {
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        let (chap18_eph, chap18_per) = find_chap18_imports(&content);
        let chap19_imports = find_chap19_imports(&content);
        let ufcs_calls = find_ufcs_calls(&content, file_path);
        
        // Only report files with imports or UFCS calls
        if chap18_eph > 0 || chap18_per > 0 || chap19_imports > 0 || !ufcs_calls.is_empty() {
            let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
            let is_mt = is_mt_file(file_path);
            
            reports.push(FileReport {
                file: rel_path.display().to_string(),
                chap18_eph_imports: chap18_eph,
                chap18_per_imports: chap18_per,
                chap19_imports,
                ufcs_calls,
                is_mt,
            });
        }
    }
    
    // Sort by filename
    reports.sort_by(|a, b| a.file.cmp(&b.file));
    
    // Report files importing both Chap18 and Chap19 (ERROR!)
    log!("{}", "=".repeat(80));
    log!("FILES IMPORTING BOTH Chap18 AND Chap19 (WRONG - CAUSES AMBIGUITY):");
    log!("{}", "=".repeat(80));
    println!();
    
    let mut both_count = 0;
    for report in &reports {
        let has_chap18 = report.chap18_eph_imports > 0 || report.chap18_per_imports > 0;
        if has_chap18 && report.chap19_imports > 0 {
            log!("{}:1:", report.file);
            log!("  Chap18 Eph imports: {}, Chap18 Per imports: {}, Chap19 imports: {}", 
                 report.chap18_eph_imports, report.chap18_per_imports, report.chap19_imports);
            if report.is_mt {
                log!("  Type: Mt file");
            } else {
                log!("  Type: St file");
            }
            println!();
            both_count += 1;
        }
    }
    
    if both_count == 0 {
        log!("None (good!)");
        println!();
    }
    
    // Report files with UFCS calls
    println!();
    log!("{}", "=".repeat(80));
    log!("Files with UFCS calls that can be simplified:");
    log!("{}", "=".repeat(80));
    println!();
    
    let mut total_ufcs = 0;
    let mut files_with_ufcs = 0;
    
    for report in &reports {
        if !report.ufcs_calls.is_empty() {
            log!("{}:1:", report.file);
            log!("  Chap18 Eph imports: {}, Chap18 Per imports: {}, Chap19 imports: {}", 
                 report.chap18_eph_imports, report.chap18_per_imports, report.chap19_imports);
            log!("  UFCS calls: {}", report.ufcs_calls.len());
            
            for call in &report.ufcs_calls {
                log!("    Line {}: {} -> {}::{}", 
                     call.line, 
                     if call.full_text.len() > 60 {
                         format!("{}...", &call.full_text[..60])
                     } else {
                         call.full_text.clone()
                     },
                     call.type_name,
                     call.method);
            }
            
            println!();
            total_ufcs += report.ufcs_calls.len();
            files_with_ufcs += 1;
        }
    }
    
    // Summary
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Files analyzed: {}", reports.len());
    log!("  Files with UFCS calls: {}", files_with_ufcs);
    log!("  Total UFCS calls: {}", total_ufcs);
    
    // Count by import pattern
    let mut chap18_eph_count = 0;
    let mut chap18_eph_mt = 0;
    let mut chap18_eph_st = 0;
    let mut chap18_per_count = 0;
    let mut chap18_per_mt = 0;
    let mut chap18_per_st = 0;
    let mut chap19_count = 0;
    let mut chap19_mt = 0;
    let mut chap19_st = 0;
    let mut both = 0;
    let mut both_mt = 0;
    let mut both_st = 0;
    
    for report in &reports {
        // Count Chap18 Eph imports (should move to Chap19)
        if report.chap18_eph_imports > 0 {
            chap18_eph_count += 1;
            if report.is_mt {
                chap18_eph_mt += 1;
            } else {
                chap18_eph_st += 1;
            }
        }
        
        // Count Chap18 Per imports (correct)
        if report.chap18_per_imports > 0 {
            chap18_per_count += 1;
            if report.is_mt {
                chap18_per_mt += 1;
            } else {
                chap18_per_st += 1;
            }
        }
        
        // Count Chap19 imports
        if report.chap19_imports > 0 {
            chap19_count += 1;
            if report.is_mt {
                chap19_mt += 1;
            } else {
                chap19_st += 1;
            }
        }
        
        // Count files with both
        let has_chap18 = report.chap18_eph_imports > 0 || report.chap18_per_imports > 0;
        if has_chap18 && report.chap19_imports > 0 {
            both += 1;
            if report.is_mt {
                both_mt += 1;
            } else {
                both_st += 1;
            }
        }
    }
    
    log!("  Files with Chap18 Eph imports (should use Chap19): {} (Mt: {}, St: {})", 
         chap18_eph_count, chap18_eph_mt, chap18_eph_st);
    log!("  Files with Chap18 Per imports (correct): {} (Mt: {}, St: {})", 
         chap18_per_count, chap18_per_mt, chap18_per_st);
    log!("  Files with Chap19 imports: {} (Mt: {}, St: {})", 
         chap19_count, chap19_mt, chap19_st);
    log!("  Files importing both Chap18 and Chap19: {} (Mt: {}, St: {})", both, both_mt, both_st);
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

