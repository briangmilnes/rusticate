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
struct MethodCallInfo {
    method_name: String,
    line: usize,
    calls_parallel_methods: Vec<ParallelCall>,  // Which parallel methods this method calls
}

#[derive(Debug, Clone)]
struct ParallelCall {
    called_module: String,
    called_method: String,
    call_line: usize,
}

#[derive(Debug, Clone)]
struct ModuleReport {
    file: PathBuf,
    module_name: String,
    inherent_parallel_methods: Vec<ParallelMethod>,
    transitive_parallel_methods: Vec<MethodCallInfo>,
    all_methods: Vec<String>,  // All method names in this module
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

fn find_all_methods(root: &SyntaxNode) -> Vec<String> {
    let mut methods = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FN {
            if let Some(fn_def) = ast::Fn::cast(node) {
                if let Some(name) = fn_def.name() {
                    methods.push(name.text().to_string());
                }
            }
        }
    }
    
    methods
}

fn find_glob_imported_mt_modules(root: &SyntaxNode, mt_module_names: &[String]) -> Vec<String> {
    let mut imported = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_stmt) = ast::Use::cast(node) {
                if let Some(use_tree) = use_stmt.use_tree() {
                    // Check for glob import (*) - must use descendants_with_tokens() to find STAR token
                    let has_glob = use_tree.syntax().descendants_with_tokens().any(|d| d.kind() == SyntaxKind::STAR);
                    
                    if has_glob {
                        // Find Mt module names in the use path
                        for desc in use_tree.syntax().descendants() {
                            if desc.kind() == SyntaxKind::NAME_REF {
                                if let Some(name_ref) = ast::NameRef::cast(desc) {
                                    let name = name_ref.text().to_string();
                                    // Check if this name matches any mt_module_name (either exact match or ends with "/name")
                                    for mt_module in mt_module_names {
                                        if mt_module == &name || mt_module.ends_with(&format!("/{}", name)) {
                                            if !imported.contains(&name) {
                                                imported.push(name.clone());
                                            }
                                            break;
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
    
    imported
}

fn find_method_calls_in_function(
    fn_node: &SyntaxNode,
    content: &str,
    glob_imported_modules: &[String],
    parallel_methods_map: &HashMap<String, Vec<String>>,
) -> Vec<ParallelCall> {
    let mut calls = Vec::new();
    
    let debug = false; // Set to true to enable debug output
    
    // Find all calls within this function
    for node in fn_node.descendants() {
        match node.kind() {
            SyntaxKind::CALL_EXPR => {
                let call_line = get_line_number(&node, content);
                if let Some(call_expr) = ast::CallExpr::cast(node) {
                    if let Some(expr) = call_expr.expr() {
                        if let ast::Expr::PathExpr(path_expr) = expr {
                            if let Some(path) = path_expr.path() {
                                let segments: Vec<_> = path.segments().collect();
                                
                                // Case 1: Explicit Type::method() or Module::method() call
                                // Check if the last segment is a method that's parallel in any glob-imported module
                                if segments.len() >= 2 {
                                    if let Some(last_segment) = segments.last() {
                                        if let Some(method_name_ref) = last_segment.name_ref() {
                                            let method_name = method_name_ref.text().to_string();
                                            
                                            // Get the type/module name (second-to-last segment)
                                            if let Some(type_segment) = segments.get(segments.len() - 2) {
                                                if let Some(type_name_ref) = type_segment.name_ref() {
                                                    let type_name = type_name_ref.text().to_string();
                                                    
                                                    // Check both the type name and type name without 'S' suffix (for type aliases like ArraySeqMtEphS)
                                                    let candidates = if type_name.ends_with('S') && type_name.len() > 1 {
                                                        vec![type_name.clone(), type_name[..type_name.len()-1].to_string()]
                                                    } else {
                                                        vec![type_name.clone()]
                                                    };
                                                    
                                                    if debug {
                                                        eprintln!("DEBUG: Call {}::{} at line {}", type_name, method_name, call_line);
                                                        eprintln!("  Candidates: {:?}", candidates);
                                                        eprintln!("  Glob imports: {:?}", glob_imported_modules);
                                                    }
                                                    
                                                    // Check if this method is parallel in any glob-imported module
                                                    for candidate in &candidates {
                                                        for module_name in glob_imported_modules {
                                                            if candidate == module_name {
                                                                if debug {
                                                                    eprintln!("  MATCH: {} == {}", candidate, module_name);
                                                                }
                                                                // Try to find the parallel methods in the map, checking all chapter variants
                                                                let mut found = false;
                                                                for (map_key, parallel_methods) in parallel_methods_map {
                                                                    // Check if map_key ends with "/module_name" (e.g., "Chap19/ArraySeqMtEph" ends with "/ArraySeqMtEph")
                                                                    if map_key.ends_with(&format!("/{}", module_name)) || map_key == module_name {
                                                                        if debug {
                                                                            eprintln!("    Parallel methods in {}: {:?}", map_key, parallel_methods);
                                                                        }
                                                                        if parallel_methods.contains(&method_name) {
                                                                            calls.push(ParallelCall {
                                                                                called_module: map_key.clone(),
                                                                                called_method: method_name.clone(),
                                                                                call_line,
                                                                            });
                                                                            found = true;
                                                                            break;
                                                                        }
                                                                    }
                                                                }
                                                                if found {
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                // Case 2: Direct method() call from glob import
                                if segments.len() == 1 {
                                    if let Some(segment) = segments.first() {
                                        if let Some(name_ref) = segment.name_ref() {
                                            let method_name = name_ref.text().to_string();
                                            // Check if this method is parallel in any glob-imported module
                                            for module_name in glob_imported_modules {
                                                if let Some(parallel_methods) = parallel_methods_map.get(module_name) {
                                                    if parallel_methods.contains(&method_name) {
                                                        calls.push(ParallelCall {
                                                            called_module: module_name.clone(),
                                                            called_method: method_name.clone(),
                                                            call_line,
                                                        });
                                                        break;  // Only count once even if multiple modules have it
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
            }
            _ => {}
        }
    }
    
    calls
}

fn analyze_file(
    file_path: &Path,
    module_name: String,
    mt_module_names: &[String],
    parallel_methods_map: &HashMap<String, Vec<String>>,
) -> Result<ModuleReport> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let inherent_parallel_methods = find_parallel_methods(root, &content);
    let all_methods = find_all_methods(root);
    let glob_imported_modules = find_glob_imported_mt_modules(root, mt_module_names);
    
    // For each method, find calls to parallel methods in other Mt modules
    let mut transitive_parallel_methods = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FN {
            if let Some(fn_def) = ast::Fn::cast(node.clone()) {
                if let Some(name) = fn_def.name() {
                    let method_name = name.text().to_string();
                    
                    // Skip if this method is already inherently parallel
                    if inherent_parallel_methods.iter().any(|m| m.name == method_name) {
                        continue;
                    }
                    
                    // Find calls to parallel methods within this method
                    let calls = find_method_calls_in_function(&node, &content, &glob_imported_modules, parallel_methods_map);
                    
                    if !calls.is_empty() {
                        transitive_parallel_methods.push(MethodCallInfo {
                            method_name,
                            line: get_line_number(&node, &content),
                            calls_parallel_methods: calls,
                        });
                    }
                }
            }
        }
    }
    
    Ok(ModuleReport {
        file: file_path.to_path_buf(),
        module_name,
        inherent_parallel_methods,
        transitive_parallel_methods,
        all_methods,
    })
}

fn extract_module_name(path: &Path) -> String {
    // Extract module name from path like src/Chap06/DirGraphMtEph.rs -> Chap06/DirGraphMtEph
    // to handle duplicate names in different chapters (e.g., Chap18/ArraySeqMtEph vs Chap19/ArraySeqMtEph)
    if let Some(parent) = path.parent() {
        if let Some(parent_name) = parent.file_name().and_then(|s| s.to_str()) {
            if parent_name.starts_with("Chap") {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    return format!("{}/{}", parent_name, file_stem);
                }
            }
        }
    }
    // Fallback to just the filename
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/review_inherent_and_transitive_mt.log").ok();
    
    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let all_files = find_rust_files(&args.paths);
    
    log!("Entering directory '{}'", base_dir.display());
    println!();
    
    // First pass: collect all Mt module names and build parallel methods map
    let mut mt_files = Vec::new();
    let mut mt_module_names = Vec::new();
    let mut parallel_methods_map: HashMap<String, Vec<String>> = HashMap::new();
    
    for file_path in &all_files {
        let path_str = file_path.to_string_lossy();
        
        // Check if it's in src/ and has Mt in the filename  
        if path_str.contains("/src/") && path_str.contains("Mt") && path_str.ends_with(".rs") {
            mt_files.push(file_path.clone());
            let module_name = extract_module_name(file_path);
            mt_module_names.push(module_name.clone());
            
            // Quick parse to get inherent parallel methods
            if let Ok(content) = fs::read_to_string(file_path) {
                let parsed = SourceFile::parse(&content, Edition::Edition2021);
                let tree = parsed.tree();
                let root = tree.syntax();
                let methods = find_parallel_methods(root, &content);
                
                if !methods.is_empty() {
                    let method_names: Vec<String> = methods.iter().map(|m| m.name.clone()).collect();
                    parallel_methods_map.insert(module_name, method_names);
                }
            }
        }
    }
    
    // Second pass: analyze all Mt modules with the parallel methods map
    let mut mt_modules: Vec<ModuleReport> = Vec::new();
    
    for file_path in &mt_files {
        let module_name = extract_module_name(file_path);
        if let Ok(report) = analyze_file(file_path, module_name, &mt_module_names, &parallel_methods_map) {
            mt_modules.push(report);
        }
    }
    
    // Sort by file path
    mt_modules.sort_by(|a, b| a.file.cmp(&b.file));
    
    // Print results
    println!();
    log!("{}", "=".repeat(80));
    log!("Mt MODULES WITH INHERENT PARALLELISM:");
    log!("(Methods with direct parallel operations: spawn, join, par_iter, ParaPair, etc.)");
    log!("{}", "=".repeat(80));
    println!();
    
    let mut total_inherent_parallel_methods = 0;
    let mut total_transitive_parallel_methods = 0;
    let mut modules_with_inherent = 0;
    let mut modules_with_transitive_only = 0;
    let mut modules_not_parallel = 0;
    
    for report in &mt_modules {
        let rel_path = report.file.strip_prefix(&base_dir).unwrap_or(&report.file);
        
        if !report.inherent_parallel_methods.is_empty() {
            log!("{}:1:", rel_path.display());
            log!("  Inherent parallel methods: {}", report.inherent_parallel_methods.len());
            for method in &report.inherent_parallel_methods {
                log!("    Line {}: {}", method.line, method.name);
            }
            println!();
            total_inherent_parallel_methods += report.inherent_parallel_methods.len();
            modules_with_inherent += 1;
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("Mt MODULES WITH TRANSITIVE PARALLELISM:");
    log!("(Methods that call parallel methods in other Mt modules)");
    log!("{}", "=".repeat(80));
    println!();
    
    for report in &mt_modules {
        let rel_path = report.file.strip_prefix(&base_dir).unwrap_or(&report.file);
        
        if !report.transitive_parallel_methods.is_empty() {
            log!("{}:1:", rel_path.display());
            log!("  Transitive parallel methods: {}", report.transitive_parallel_methods.len());
            for method_info in &report.transitive_parallel_methods {
                log!("    Line {}: {} calls:", method_info.line, method_info.method_name);
                for call in &method_info.calls_parallel_methods {
                    log!("      Line {}: {}::{}", call.call_line, call.called_module, call.called_method);
                }
            }
            println!();
            total_transitive_parallel_methods += report.transitive_parallel_methods.len();
            if report.inherent_parallel_methods.is_empty() {
                modules_with_transitive_only += 1;
            }
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("Mt MODULES NOT PARALLEL:");
    log!("(No inherent or transitive parallel methods detected)");
    log!("{}", "=".repeat(80));
    println!();
    
    for report in &mt_modules {
        let rel_path = report.file.strip_prefix(&base_dir).unwrap_or(&report.file);
        
        if report.inherent_parallel_methods.is_empty() && report.transitive_parallel_methods.is_empty() {
            log!("{}:1:", rel_path.display());
            modules_not_parallel += 1;
        }
    }
    
    println!();
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Total Mt modules analyzed: {}", mt_modules.len());
    log!("  Modules with inherent parallelism: {}", modules_with_inherent);
    log!("  Modules with transitive parallelism only: {}", modules_with_transitive_only);
    log!("  Modules not parallel: {}", modules_not_parallel);
    log!("  Total inherent parallel methods: {}", total_inherent_parallel_methods);
    log!("  Total transitive parallel methods: {}", total_transitive_parallel_methods);
    log!("{}", "=".repeat(80));
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

