// Copyright (C) Brian G. Milnes 2025

//! Fix: Duplicate Method Call Sites
//! 
//! Fixes call sites in tests/code that used the removed convenience functions.
//! Converts: `method(&receiver, args)` → `receiver.method(args)`
//! 
//! This tool finds call sites that reference methods which have trait + impl
//! but no standalone pub fn (after fix_duplicate_methods removed them).
//! 
//! Binary: rusticate-fix-duplicate-method-call-sites

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasArgList}, SyntaxKind, SourceFile, Edition};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use rusticate::StandardArgs;
use rusticate::args::args::find_rust_files;
use rusticate::duplicate_methods::find_duplicate_methods;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/fix_duplicate_method_call_sites.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn fix_file(file_path: &PathBuf, method_names: &[String], dry_run: bool) -> Result<usize> {
    let source = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&source, Edition::Edition2021);
    
    if !parsed.errors().is_empty() {
        return Ok(0);
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut replacements = Vec::new();
    
    // Find all call expressions
    for node in root.descendants() {
        if node.kind() == SyntaxKind::CALL_EXPR {
            if let Some(call_expr) = ast::CallExpr::cast(node.clone()) {
                // Get the function being called
                if let Some(expr) = call_expr.expr() {
                    // Check if it's a path expression (function name)
                    if let Some(path_expr) = ast::PathExpr::cast(expr.syntax().clone()) {
                        if let Some(path) = path_expr.path() {
                            let fn_name = path.to_string();
                            
                            // Check if this is one of our target methods
                            if method_names.contains(&fn_name) {
                                // Get the argument list
                                if let Some(arg_list) = call_expr.arg_list() {
                                    let args: Vec<_> = arg_list.args().collect();
                                    
                                    // Check if first arg is a reference expression (&something)
                                    if !args.is_empty() {
                                        let first_arg = &args[0];
                                        let first_arg_text = first_arg.to_string();
                                        
                                        // Check if it starts with & (reference)
                                        if first_arg_text.trim().starts_with('&') {
                                            let receiver = first_arg_text.trim().trim_start_matches('&').trim_start_matches("mut ").trim();
                                            
                                            // Build new call: receiver.method(rest_of_args)
                                            let rest_args: Vec<String> = args.iter().skip(1).map(|a| a.to_string()).collect();
                                            let new_call = if rest_args.is_empty() {
                                                format!("{receiver}.{fn_name}()")
                                            } else {
                                                format!("{}.{}({})", receiver, fn_name, rest_args.join(", "))
                                            };
                                            
                                            let start: usize = node.text_range().start().into();
                                            let end: usize = node.text_range().end().into();
                                            
                                            replacements.push((start, end, new_call));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    if replacements.is_empty() {
        return Ok(0);
    }
    
    // Apply replacements in reverse order
    let mut new_source = source.clone();
    replacements.sort_by(|a, b| b.0.cmp(&a.0));
    
    for (start, end, replacement) in &replacements {
        new_source.replace_range(*start..*end, replacement);
    }
    
    let fixed_count = replacements.len();
    
    if !dry_run {
        fs::write(file_path, new_source)?;
        log!("Fixed {} call site(s) in {}", fixed_count, file_path.display());
    } else {
        log!("[DRY RUN] Would fix {} call site(s) in {}", fixed_count, file_path.display());
    }
    
    Ok(fixed_count)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    
    // Check for --dry-run flag
    let dry_run = std::env::args().any(|arg| arg == "--dry-run");
    
    // First, find all duplicate methods in src/ to know which methods to look for
    let src_dir = std::env::current_dir()?.join("src");
    let src_files = if src_dir.exists() {
        find_rust_files(&[src_dir])
    } else {
        Vec::new()
    };
    
    let mut method_names = Vec::new();
    for file_path in &src_files {
        if let Ok(issues) = find_duplicate_methods(file_path) {
            for issue in issues {
                // Check if this issue has trait + impl (which means pub fn was removed)
                let has_trait = issue.locations.iter().any(|l| l.location_type == "trait");
                let has_impl = issue.locations.iter().any(|l| l.location_type == "impl");
                let had_pub_fn = issue.locations.iter().any(|l| l.location_type == "pub fn");
                
                // If it has trait + impl but no pub fn, that pub fn was removed
                // But we also want to catch cases where pub fn still exists but shouldn't
                if has_trait && has_impl
                    && !method_names.contains(&issue.name) {
                        method_names.push(issue.name.clone());
                    }
            }
        }
    }
    
    if method_names.is_empty() {
        log!("No duplicate methods found - nothing to fix");
        return Ok(());
    }
    
    log!("{}", "=".repeat(80));
    log!("FIX DUPLICATE METHOD CALL SITES");
    if dry_run {
        log!("(DRY RUN MODE)");
    }
    log!("{}", "=".repeat(80));
    log!("Target methods: {}", method_names.join(", "));
    log!("");
    
    // Get test/bench files to fix
    let search_dirs = args.get_search_dirs();
    let files = find_rust_files(&search_dirs);
    
    let mut total_fixed = 0;
    let mut files_modified = 0;
    
    for file_path in &files {
        // Skip src/ files, only fix tests and benches
        if file_path.to_string_lossy().contains("/src/") {
            continue;
        }
        
        if let Ok(count) = fix_file(file_path, &method_names, dry_run) {
            if count > 0 {
                total_fixed += count;
                files_modified += 1;
            }
        }
    }
    
    log!("");
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Files modified: {}", files_modified);
    log!("  Total call sites fixed: {}", total_fixed);
    log!("");
    log!("Completed in {}ms", start.elapsed().as_millis());
    
    Ok(())
}

