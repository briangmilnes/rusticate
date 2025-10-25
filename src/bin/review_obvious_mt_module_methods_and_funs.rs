//! Review: Obvious Mt Module Methods and Functions
//!
//! Analyzes Mt modules to find which methods/functions contain parallel operations.
//! This helps determine transitive parallelism - Mt modules that call other Mt 
//! modules' parallel methods are themselves parallel.
//!
//! Uses AST parsing - NO STRING HACKING.
//!
//! Binary: review-obvious-mt-module-methods-and-funs

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasName}, SyntaxKind, SourceFile, Edition, SyntaxNode};
use rusticate::{find_rust_files, StandardArgs};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

macro_rules! log {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        println!("{}", msg);
    }};
}

#[derive(Debug, Clone)]
struct ParallelMethod {
    name: String,
    line: usize,
}

#[derive(Debug)]
struct ModuleReport {
    file: PathBuf,
    parallel_methods: Vec<ParallelMethod>,
}

fn get_line_number(node: &SyntaxNode, content: &str) -> usize {
    let offset: usize = node.text_range().start().into();
    content[..offset].lines().count()
}

fn has_parallel_operation(node: &SyntaxNode) -> bool {
    // Check for parallel operations in this node
    for descendant in node.descendants() {
        match descendant.kind() {
            SyntaxKind::METHOD_CALL_EXPR => {
                if let Some(method_call) = ast::MethodCallExpr::cast(descendant) {
                    if let Some(name_ref) = method_call.name_ref() {
                        let method_name = name_ref.text();
                        if method_name == "spawn" 
                            || method_name == "join" 
                            || method_name == "par_iter"
                            || method_name == "par_chunks"
                            || method_name == "par_bridge" {
                            return true;
                        }
                    }
                }
            }
            SyntaxKind::CALL_EXPR => {
                if let Some(call_expr) = ast::CallExpr::cast(descendant) {
                    if let Some(expr) = call_expr.expr() {
                        if let ast::Expr::PathExpr(path_expr) = expr {
                            if let Some(path) = path_expr.path() {
                                if let Some(segment) = path.segment() {
                                    if let Some(name) = segment.name_ref() {
                                        let fn_name = name.text();
                                        if fn_name == "spawn" 
                                            || fn_name == "join"
                                            || fn_name == "ParaPair" {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            SyntaxKind::MACRO_CALL => {
                // Check for ParaPair! macro
                if let Some(macro_call) = ast::MacroCall::cast(descendant) {
                    if let Some(path) = macro_call.path() {
                        if let Some(segment) = path.segment() {
                            if let Some(name) = segment.name_ref() {
                                if name.text() == "ParaPair" {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn find_parallel_methods(root: &SyntaxNode, content: &str) -> Vec<ParallelMethod> {
    let mut methods = Vec::new();
    
    // Find all function definitions
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FN {
            if let Some(fn_def) = ast::Fn::cast(node.clone()) {
                if has_parallel_operation(&node) {
                    if let Some(name) = fn_def.name() {
                        methods.push(ParallelMethod {
                            name: name.text().to_string(),
                            line: get_line_number(&node, content),
                        });
                    }
                }
            }
        }
    }
    
    methods
}

fn analyze_file(file_path: &Path) -> Result<ModuleReport> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let parallel_methods = find_parallel_methods(root, &content);
    
    Ok(ModuleReport {
        file: file_path.to_path_buf(),
        parallel_methods,
    })
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/review_obvious_mt_module_methods_and_funs.log").ok();
    
    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let all_files = find_rust_files(&args.paths);
    
    log!("Entering directory '{}'", base_dir.display());
    println!();
    
    let mut mt_modules: Vec<(PathBuf, ModuleReport)> = Vec::new();
    
    // Filter for Mt modules in src/
    for file_path in &all_files {
        let path_str = file_path.to_string_lossy();
        
        // Check if it's in src/ and has Mt in the filename
        if path_str.contains("/src/") && path_str.contains("Mt") && path_str.ends_with(".rs") {
            if let Ok(report) = analyze_file(file_path) {
                let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
                mt_modules.push((rel_path.to_path_buf(), report));
            }
        }
    }
    
    // Sort by file path
    mt_modules.sort_by(|a, b| a.0.cmp(&b.0));
    
    // Print results
    println!();
    log!("{}", "=".repeat(80));
    log!("Mt MODULES WITH PARALLEL METHODS/FUNCTIONS:");
    log!("{}", "=".repeat(80));
    println!();
    
    let mut total_parallel_methods = 0;
    let mut modules_with_parallelism = 0;
    let mut modules_without_parallelism = Vec::new();
    
    for (file, report) in &mt_modules {
        if !report.parallel_methods.is_empty() {
            log!("{}:1:", file.display());
            log!("  Parallel methods/functions: {}", report.parallel_methods.len());
            for method in &report.parallel_methods {
                log!("    Line {}: {}", method.line, method.name);
            }
            println!();
            total_parallel_methods += report.parallel_methods.len();
            modules_with_parallelism += 1;
        } else {
            modules_without_parallelism.push(file.clone());
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("Mt MODULES WITHOUT DIRECT PARALLEL OPERATIONS:");
    log!("(May be transitively parallel by calling other Mt modules)");
    log!("{}", "=".repeat(80));
    println!();
    
    if modules_without_parallelism.is_empty() {
        log!("None found - all Mt modules have direct parallelism!");
    } else {
        for file in &modules_without_parallelism {
            log!("{}:1:", file.display());
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Total Mt modules analyzed: {}", mt_modules.len());
    log!("  Modules with direct parallelism: {}", modules_with_parallelism);
    log!("  Modules without direct parallelism: {}", modules_without_parallelism.len());
    log!("  Total parallel methods/functions found: {}", total_parallel_methods);
    log!("{}", "=".repeat(80));
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

