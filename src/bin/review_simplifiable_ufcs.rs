//! Review: Find UFCS calls that could be simplified
//!
//! With the stubbing architecture in Chap19, methods that are re-declared
//! can be called as Type::method() instead of <Type as Trait>::method().
//! This tool identifies such calls for simplification.
//!
//! Uses AST parsing - NO STRING HACKING.
//!
//! Binary: rusticate-review-simplifiable-ufcs

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition};
use rusticate::{find_rust_files, StandardArgs};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug)]
struct UfcsCall {
    file: PathBuf,
    line: usize,
    type_name: String,
    trait_name: String,
    method: String,
    is_chap19: bool,
}

fn parse_ufcs_from_text(text: &str) -> Option<(String, String, String)> {
    // Parse "<Type as Trait>::method" from text
    // Find the pattern: <...> as ...>::method
    
    let start = text.find('<')?;
    let as_pos = text.find(" as ")?;
    
    if as_pos <= start {
        return None;
    }
    
    // Extract type name (between < and " as ")
    let type_part = &text[start + 1..as_pos];
    let type_name = type_part.trim().to_string();
    
    // Find the end of the trait (look for >::)
    let after_as = &text[as_pos + 4..];
    let trait_end = after_as.find(">::")?;
    let trait_name = after_as[..trait_end].trim().to_string();
    
    // Extract method name (after >::)
    let after_method_start = &after_as[trait_end + 3..];
    // Method name goes until ( or <
    let method_end = after_method_start
        .find('(')
        .or_else(|| after_method_start.find('<'))
        .unwrap_or(after_method_start.len());
    let method = after_method_start[..method_end].trim().to_string();
    
    Some((type_name, trait_name, method))
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

fn is_ufcs_pattern(node: &ra_ap_syntax::SyntaxNode) -> bool {
    // Check if this is a PATH_EXPR with a PATH that contains an AS_KW (UFCS pattern)
    if node.kind() == SyntaxKind::PATH_EXPR {
        for descendant in node.descendants_with_tokens() {
            if descendant.kind() == SyntaxKind::AS_KW {
                return true;
            }
        }
    }
    false
}

fn find_ufcs_calls(content: &str, file_path: &Path) -> Vec<UfcsCall> {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut calls = Vec::new();
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    
    // Determine if this file uses Chap19 using AST
    let mut is_chap19 = false;
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                if path_starts_with_chapter(&use_item, "Chap19") {
                    is_chap19 = true;
                    break;
                }
            }
        }
    }
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::CALL_EXPR {
            if let Some(callee) = node.first_child() {
                // Check for UFCS pattern using AST
                if is_ufcs_pattern(&callee) {
                    let callee_text = callee.to_string();
                    
                    if let Some((type_name, trait_name, method)) = parse_ufcs_from_text(&callee_text) {
                        if !type_name.starts_with("Self") && !type_name.is_empty() && !trait_name.is_empty() {
                            let offset = node.text_range().start();
                            let line = content[..usize::from(offset)].lines().count();
                            
                            let key = (line, type_name.clone(), method.clone());
                            if !seen.contains(&key) {
                                seen.insert(key);
                                
                                calls.push(UfcsCall {
                                    file: file_path.to_path_buf(),
                                    line,
                                    type_name,
                                    trait_name,
                                    method,
                                    is_chap19,
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

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/review_simplifiable_ufcs.log").ok();
    
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
    let mut all_calls: Vec<UfcsCall> = Vec::new();
    
    for file_path in &files {
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        let calls = find_ufcs_calls(&content, file_path);
        all_calls.extend(calls);
    }
    
    // Group by file
    let mut by_file: HashMap<PathBuf, Vec<&UfcsCall>> = HashMap::new();
    for call in &all_calls {
        by_file.entry(call.file.clone()).or_default().push(call);
    }
    
    // Sort files by path
    let mut file_paths: Vec<_> = by_file.keys().collect();
    file_paths.sort();
    
    log!("UFCS Calls That Could Potentially Be Simplified");
    log!("(These use <Type as Trait>::method syntax)");
    println!();
    
    for file_path in &file_paths {
        let calls = by_file.get(*file_path).unwrap();
        let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
        
        log!("{}:1:", rel_path.display());
        log!("  {} UFCS call{}", calls.len(), if calls.len() == 1 { "" } else { "s" });
        
        for call in calls {
            log!("    Line {}: <{} as {}>::{}", 
                call.line, call.type_name, call.trait_name, call.method);
            if call.is_chap19 {
                log!("      (File uses Chap19 - may be simplifiable)");
            }
        }
        println!();
    }
    
    let chap19_calls = all_calls.iter().filter(|c| c.is_chap19).count();
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Total UFCS calls: {}", all_calls.len());
    log!("  Files with UFCS calls: {}", by_file.len());
    log!("  UFCS calls in Chap19 files: {} (potentially simplifiable)", chap19_calls);
    log!("  UFCS calls in Chap18 files: {}", all_calls.len() - chap19_calls);
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

