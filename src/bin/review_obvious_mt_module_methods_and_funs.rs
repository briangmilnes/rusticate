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

#[derive(Debug, Clone)]
struct ModuleReport {
    file: PathBuf,
    parallel_methods: Vec<ParallelMethod>,
    calls_mt_modules: Vec<String>,  // Which Mt modules this module imports
}

#[derive(Debug)]
struct TransitiveInfo {
    inherent_parallel: bool,
    transitive_parallel: bool,
    parallel_via: Vec<String>,  // Which modules make this transitively parallel
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

fn find_imported_mt_modules(root: &SyntaxNode, mt_module_names: &[String]) -> Vec<String> {
    let mut imported_modules = Vec::new();
    
    // Find all use statements that import Mt modules
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_stmt) = ast::Use::cast(node) {
                if let Some(use_tree) = use_stmt.use_tree() {
                    // Check all NAME_REF nodes in the use tree for Mt module names
                    for desc in use_tree.syntax().descendants() {
                        if desc.kind() == SyntaxKind::NAME_REF {
                            if let Some(name_ref) = ast::NameRef::cast(desc) {
                                let name = name_ref.text().to_string();
                                if mt_module_names.contains(&name) {
                                    if !imported_modules.contains(&name) {
                                        imported_modules.push(name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    imported_modules
}

fn analyze_file(file_path: &Path, mt_module_names: &[String]) -> Result<ModuleReport> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let parallel_methods = find_parallel_methods(root, &content);
    let calls_mt_modules = find_imported_mt_modules(root, mt_module_names);
    
    Ok(ModuleReport {
        file: file_path.to_path_buf(),
        parallel_methods,
        calls_mt_modules,
    })
}

fn extract_module_name(path: &Path) -> String {
    // Extract module name from path like src/Chap06/DirGraphMtEph.rs -> DirGraphMtEph
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/review_inherent_mt_module_methods_and_funs.log").ok();
    
    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let all_files = find_rust_files(&args.paths);
    
    log!("Entering directory '{}'", base_dir.display());
    println!();
    
    // First pass: collect all Mt module names
    let mut mt_files = Vec::new();
    let mut mt_module_names = Vec::new();
    
    for file_path in &all_files {
        let path_str = file_path.to_string_lossy();
        
        // Check if it's in src/ and has Mt in the filename  
        if path_str.contains("/src/") && path_str.contains("Mt") && path_str.ends_with(".rs") {
            mt_files.push(file_path.clone());
            let module_name = extract_module_name(file_path);
            mt_module_names.push(module_name);
        }
    }
    
    // Second pass: analyze all Mt modules
    let mut mt_modules: Vec<(PathBuf, ModuleReport)> = Vec::new();
    
    for file_path in &mt_files {
        if let Ok(report) = analyze_file(file_path, &mt_module_names) {
            let rel_path = file_path.strip_prefix(&base_dir).unwrap_or(file_path);
            mt_modules.push((rel_path.to_path_buf(), report));
        }
    }
    
    // Sort by file path
    mt_modules.sort_by(|a, b| a.0.cmp(&b.0));
    
    // Compute transitivity via fixed-point iteration
    let mut transitive_info: HashMap<String, TransitiveInfo> = HashMap::new();
    
    // Initialize: modules with inherent parallelism
    for (path, report) in &mt_modules {
        let module_name = extract_module_name(path.as_path());
        transitive_info.insert(module_name.clone(), TransitiveInfo {
            inherent_parallel: !report.parallel_methods.is_empty(),
            transitive_parallel: !report.parallel_methods.is_empty(),
            parallel_via: Vec::new(),
        });
    }
    
    // Fixed-point iteration: propagate transitivity
    let mut changed = true;
    while changed {
        changed = false;
        
        for (path, report) in &mt_modules {
            let module_name = extract_module_name(path.as_path());
            
            // If already inherently parallel, skip
            if transitive_info[&module_name].inherent_parallel {
                continue;
            }
            
            // Check if this module calls any parallel (or transitively parallel) modules
            for called_module in &report.calls_mt_modules {
                if let Some(called_info) = transitive_info.get(called_module) {
                    if called_info.transitive_parallel {
                        let info = transitive_info.get_mut(&module_name).unwrap();
                        if !info.transitive_parallel {
                            info.transitive_parallel = true;
                            info.parallel_via.push(called_module.clone());
                            changed = true;
                        } else if !info.parallel_via.contains(called_module) {
                            info.parallel_via.push(called_module.clone());
                        }
                    }
                }
            }
        }
    }
    
    // Print results
    println!();
    log!("{}", "=".repeat(80));
    log!("Mt MODULES WITH INHERENT PARALLELISM:");
    log!("(Direct parallel operations: spawn, join, par_iter, ParaPair, etc.)");
    log!("{}", "=".repeat(80));
    println!();
    
    let mut total_parallel_methods = 0;
    let mut modules_with_parallelism = 0;
    let mut modules_transitively_parallel = Vec::new();
    let mut modules_not_parallel = Vec::new();
    
    for (file, report) in &mt_modules {
        let module_name = extract_module_name(file.as_path());
        
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
            // Check if transitively parallel
            if let Some(info) = transitive_info.get(&module_name) {
                if info.transitive_parallel {
                    modules_transitively_parallel.push((file.clone(), info.parallel_via.clone()));
                } else {
                    modules_not_parallel.push(file.clone());
                }
            } else {
                modules_not_parallel.push(file.clone());
            }
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("Mt MODULES TRANSITIVELY PARALLEL:");
    log!("(No inherent parallelism, but import/use parallel Mt modules)");
    log!("{}", "=".repeat(80));
    println!();
    
    if modules_transitively_parallel.is_empty() {
        log!("None found");
    } else {
        for (file, via_modules) in &modules_transitively_parallel {
            log!("{}:1:", file.display());
            log!("  Parallel via imports: {}", via_modules.join(", "));
            println!();
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("Mt MODULES NOT PARALLEL:");
    log!("(No direct or transitive parallel operations detected)");
    log!("{}", "=".repeat(80));
    println!();
    
    if modules_not_parallel.is_empty() {
        log!("None found - all Mt modules are parallel!");
    } else {
        for file in &modules_not_parallel {
            log!("{}:1:", file.display());
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Total Mt modules analyzed: {}", mt_modules.len());
    log!("  Modules with inherent parallelism: {}", modules_with_parallelism);
    log!("  Modules with transitive parallelism: {}", modules_transitively_parallel.len());
    log!("  Modules not parallel: {}", modules_not_parallel.len());
    log!("  Total inherent parallel methods/functions: {}", total_parallel_methods);
    log!("{}", "=".repeat(80));
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

