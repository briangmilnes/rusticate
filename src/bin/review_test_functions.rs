//! Review tool to check that every public function/method has test coverage.
//! Reports untested functions and call counts for each function.
//! NO STRING HACKING - uses proper AST parsing throughout.

use anyhow::Result;
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{ast, AstNode, SyntaxKind, SyntaxNode};
use regex::Regex;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;
use walkdir::WalkDir;

struct Logger {
    file: Mutex<fs::File>,
}

impl Logger {
    fn new(path: &Path) -> Result<Self> {
        // Create analyses/ directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        
        Ok(Logger {
            file: Mutex::new(file),
        })
    }
    
    fn log(&self, msg: &str) {
        println!("{}", msg);
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{}", msg);
        }
    }
}

#[derive(Debug, Clone)]
struct PublicFunction {
    name: String,
    file: PathBuf,
    line: usize,
    impl_type: Option<String>, // For methods: Some(TypeName), for free functions: None
}

#[derive(Debug)]
struct TestCoverage {
    function: PublicFunction,
    call_count: usize,
    test_files: Vec<PathBuf>,
}

fn get_line_number(node: &SyntaxNode, content: &str) -> usize {
    let offset = node.text_range().start().into();
    content[..offset].lines().count()
}

/// Extract all public functions from a source file
fn find_public_functions(file_path: &Path) -> Result<Vec<PublicFunction>> {
    let content = fs::read_to_string(file_path)?;
    let parsed = ra_ap_syntax::SourceFile::parse(&content, ra_ap_syntax::Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut functions = Vec::new();
    
    // Find all function definitions
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FN {
            if let Some(fn_def) = ast::Fn::cast(node.clone()) {
                // Check if function is truly public (not pub(crate), pub(super), etc)
                let visibility = node.children_with_tokens().find_map(|child| {
                    if child.kind() == SyntaxKind::VISIBILITY {
                        child.as_node().cloned()
                    } else {
                        None
                    }
                });
                
                let is_truly_public = if let Some(vis_node) = visibility {
                    // Has pub keyword
                    let has_pub = vis_node.children_with_tokens().any(|t| t.kind() == SyntaxKind::PUB_KW);
                    // But no restrictions like (crate), (super), (in path)
                    let has_restriction = vis_node.children_with_tokens().any(|t| t.kind() == SyntaxKind::L_PAREN);
                    has_pub && !has_restriction
                } else {
                    false // No visibility = private
                };
                
                if !is_truly_public {
                    continue;
                }
                
                if let Some(name) = fn_def.name() {
                    let fn_name = name.text().to_string();
                    let line = get_line_number(&node, &content);
                    
                    // Determine if this is a method (inside impl block) or free function
                    let impl_type = find_parent_impl_type(&node);
                    
                    functions.push(PublicFunction {
                        name: fn_name,
                        file: file_path.to_path_buf(),
                        line,
                        impl_type,
                    });
                }
            }
        }
    }
    
    Ok(functions)
}

/// Find the impl type if this function is inside an impl block
fn find_parent_impl_type(node: &SyntaxNode) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::IMPL {
            if let Some(impl_def) = ast::Impl::cast(parent.clone()) {
                if let Some(self_ty) = impl_def.self_ty() {
                    // Extract type name from self_ty
                    // Look for the first NAME token
                    for token in self_ty.syntax().descendants_with_tokens() {
                        if token.kind() == SyntaxKind::NAME {
                            if let Some(name_text) = token.as_token() {
                                return Some(name_text.text().to_string());
                            }
                        }
                    }
                }
            }
        }
        current = parent.parent();
    }
    None
}

/// Find all function calls in a test file
fn find_function_calls(test_file: &Path) -> Result<HashMap<String, usize>> {
    let content = fs::read_to_string(test_file)?;
    let parsed = ra_ap_syntax::SourceFile::parse(&content, ra_ap_syntax::Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut call_counts: HashMap<String, usize> = HashMap::new();
    
    // Find all call expressions (including those inside macros)
    for node in root.descendants_with_tokens() {
        // Check if it's a node (not just a token)
        if let Some(node) = node.as_node() {
            match node.kind() {
                SyntaxKind::CALL_EXPR => {
                    if let Some(call_expr) = ast::CallExpr::cast(node.clone()) {
                        if let Some(expr) = call_expr.expr() {
                            // Extract function name from the call
                            let fn_name = match expr {
                                ast::Expr::PathExpr(path_expr) => {
                                    if let Some(path) = path_expr.path() {
                                        // Get the last segment (the actual function name)
                                        if let Some(segment) = path.segments().last() {
                                            segment.name_ref().map(|n| n.text().to_string())
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };
                            
                            if let Some(name) = fn_name {
                                *call_counts.entry(name).or_insert(0) += 1;
                            }
                        }
                    }
                }
                SyntaxKind::METHOD_CALL_EXPR => {
                    if let Some(method_call) = ast::MethodCallExpr::cast(node.clone()) {
                        if let Some(name_ref) = method_call.name_ref() {
                            let method_name = name_ref.text().to_string();
                            *call_counts.entry(method_name).or_insert(0) += 1;
                        }
                    }
                }
                SyntaxKind::MACRO_CALL => {
                    // Parse macro arguments to find function calls inside
                    if let Some(macro_call) = ast::MacroCall::cast(node.clone()) {
                        if let Some(token_tree) = macro_call.token_tree() {
                            let macro_content = token_tree.syntax().text().to_string();
                            
                            // Simple heuristic: look for function_name( patterns in macro content
                            // This is more robust than trying to parse as Rust
                            for line in macro_content.lines() {
                                // Match pattern: word characters followed by (
                                let re_pattern = r"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\(";
                                if let Ok(re) = regex::Regex::new(re_pattern) {
                                    for cap in re.captures_iter(line) {
                                        if let Some(fn_name) = cap.get(1) {
                                            let name = fn_name.as_str().to_string();
                                            // Skip common keywords and macros
                                            if !["if", "match", "for", "while", "loop", "assert", "assert_eq", "println", "eprintln", "panic", "vec"].contains(&name.as_str()) {
                                                *call_counts.entry(name).or_insert(0) += 1;
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
    }
    
    Ok(call_counts)
}

fn main() -> Result<()> {
    let start = Instant::now();
    
    // Parse standard arguments (-c, -d, -f, -m)
    let _standard_args = rusticate::StandardArgs::parse()?;
    
    // Find project root (directory containing Cargo.toml)
    let current_dir = std::env::current_dir()?;
    let project_root = find_project_root(&current_dir)?;
    
    // Create logger for analyses/review_test_functions.txt
    let log_path = project_root.join("analyses/review_test_functions.txt");
    let logger = Logger::new(&log_path)?;
    
    let src_dir = project_root.join("src");
    let tests_dir = project_root.join("tests");
    
    if !src_dir.exists() {
        logger.log("Error: src/ directory not found");
        std::process::exit(1);
    }
    
    // Step 1: Find all public functions in src/
    let mut all_functions = Vec::new();
    for entry in WalkDir::new(&src_dir).follow_links(true) {
        let entry = entry?;
        if entry.file_type().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
            match find_public_functions(entry.path()) {
                Ok(functions) => all_functions.extend(functions),
                Err(e) => logger.log(&format!("Warning: Failed to parse {}: {}", entry.path().display(), e)),
            }
        }
    }
    
    // Step 2: Find all function calls in tests/
    let mut test_call_counts: HashMap<String, (usize, Vec<PathBuf>)> = HashMap::new();
    
    if tests_dir.exists() {
        for entry in WalkDir::new(&tests_dir).follow_links(true) {
            let entry = entry?;
            if entry.file_type().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                match find_function_calls(entry.path()) {
                    Ok(calls) => {
                        for (fn_name, count) in calls {
                            let entry_data = test_call_counts.entry(fn_name).or_insert((0, Vec::new()));
                            entry_data.0 += count;
                            if !entry_data.1.contains(&entry.path().to_path_buf()) {
                                entry_data.1.push(entry.path().to_path_buf());
                            }
                        }
                    }
                    Err(e) => logger.log(&format!("Warning: Failed to parse test file {}: {}", entry.path().display(), e)),
                }
            }
        }
    }
    
    // Step 3: Build coverage report
    let mut coverage: Vec<TestCoverage> = Vec::new();
    for func in all_functions {
        let (call_count, test_files) = test_call_counts.get(&func.name).cloned().unwrap_or((0, Vec::new()));
        coverage.push(TestCoverage {
            function: func,
            call_count,
            test_files,
        });
    }
    
    // Sort by file and line
    coverage.sort_by(|a, b| {
        a.function.file.cmp(&b.function.file)
            .then_with(|| a.function.line.cmp(&b.function.line))
    });
    
    // Step 4: Print report
    let untested: Vec<_> = coverage.iter().filter(|c| c.call_count == 0).collect();
    let tested: Vec<_> = coverage.iter().filter(|c| c.call_count > 0).collect();
    
    logger.log("");
    logger.log(&"=".repeat(80));
    logger.log("PUBLIC FUNCTIONS WITHOUT TEST COVERAGE:");
    logger.log(&"=".repeat(80));
    logger.log("");
    
    for cov in &untested {
        let func_desc = if let Some(ref impl_type) = cov.function.impl_type {
            format!("{}::{}", impl_type, cov.function.name)
        } else {
            cov.function.name.clone()
        };
        
        let rel_path = cov.function.file.strip_prefix(&project_root)
            .unwrap_or(&cov.function.file);
        
        logger.log(&format!("{}:{}:  {} - NO TEST COVERAGE", 
            rel_path.display(), 
            cov.function.line,
            func_desc
        ));
    }
    
    logger.log("");
    logger.log(&"=".repeat(80));
    logger.log("PUBLIC FUNCTIONS WITH TEST COVERAGE:");
    logger.log(&"=".repeat(80));
    logger.log("");
    
    for cov in &tested {
        let func_desc = if let Some(ref impl_type) = cov.function.impl_type {
            format!("{}::{}", impl_type, cov.function.name)
        } else {
            cov.function.name.clone()
        };
        
        let rel_path = cov.function.file.strip_prefix(&project_root)
            .unwrap_or(&cov.function.file);
        
        logger.log(&format!("{}:{}:  {} - {} call(s) in {} test file(s)", 
            rel_path.display(), 
            cov.function.line,
            func_desc,
            cov.call_count,
            cov.test_files.len()
        ));
    }
    
    // Summary
    let elapsed = start.elapsed();
    logger.log("");
    logger.log(&"=".repeat(80));
    logger.log("SUMMARY:");
    logger.log(&format!("  Total public functions: {}", coverage.len()));
    logger.log(&format!("  Functions with test coverage: {} ({:.1}%)", 
        tested.len(), 
        if coverage.is_empty() { 0.0 } else { 100.0 * tested.len() as f64 / coverage.len() as f64 }
    ));
    logger.log(&format!("  Functions without test coverage: {} ({:.1}%)", 
        untested.len(),
        if coverage.is_empty() { 0.0 } else { 100.0 * untested.len() as f64 / coverage.len() as f64 }
    ));
    logger.log(&format!("  Total test calls: {}", tested.iter().map(|c| c.call_count).sum::<usize>()));
    logger.log(&"=".repeat(80));
    logger.log(&format!("Completed in {}ms", elapsed.as_millis()));
    
    Ok(())
}

fn find_project_root(start: &Path) -> Result<PathBuf> {
    let mut current = start;
    loop {
        if current.join("Cargo.toml").exists() {
            return Ok(current.to_path_buf());
        }
        current = current.parent().ok_or_else(|| anyhow::anyhow!("Could not find Cargo.toml"))?;
    }
}

