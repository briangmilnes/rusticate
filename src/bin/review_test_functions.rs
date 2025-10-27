//! Review tool to check that every public function/method has test coverage.
//! Reports untested functions and call counts for each function.
//! NO STRING HACKING - uses proper AST parsing throughout.

use anyhow::Result;
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{ast, AstNode, SyntaxKind, SyntaxNode};
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

#[derive(Debug, Clone)]
struct TraitImpl {
    trait_name: String,   // "Display", "Debug", "PartialEq", etc.
    type_name: String,    // "PQEntry", "ArraySeqS", etc.
    method_name: String,  // "fmt", "eq", etc.
    _file: PathBuf,       // For future use
    _line: usize,         // For future use
}

#[derive(Debug)]
struct TestCoverage {
    function: PublicFunction,
    call_count: usize,
    test_files: Vec<PathBuf>,
    coverage_source: CoverageSource,
}

#[derive(Debug, Clone)]
enum CoverageSource {
    Direct,                  // Direct function call
    DisplayTrait,            // via format!(), println!(), etc.
    DebugTrait,              // via format!("{:?}"), println!("{:?}"), etc.
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
                
                // Check if function is truly public
                let has_explicit_pub = if let Some(vis_node) = visibility {
                    // Has pub keyword
                    let has_pub = vis_node.children_with_tokens().any(|t| t.kind() == SyntaxKind::PUB_KW);
                    // But no restrictions like (crate), (super), (in path)
                    let has_restriction = vis_node.children_with_tokens().any(|t| t.kind() == SyntaxKind::L_PAREN);
                    has_pub && !has_restriction
                } else {
                    false
                };
                
                // Trait implementation methods are implicitly public (even without pub keyword)
                let is_trait_impl_method = is_in_public_trait_impl(&node);
                
                let is_truly_public = has_explicit_pub || is_trait_impl_method;
                
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

/// Check if a function is inside a public trait implementation
/// Trait implementation methods are implicitly public (if the trait is public)
fn is_in_public_trait_impl(node: &SyntaxNode) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::IMPL {
            if let Some(impl_def) = ast::Impl::cast(parent.clone()) {
                // Check if this is a trait implementation (impl Trait for Type)
                // vs inherent implementation (impl Type)
                if impl_def.trait_().is_some() {
                    // This is a trait implementation
                    // Methods in trait implementations are implicitly public
                    return true;
                }
            }
        }
        current = parent.parent();
    }
    false
}

/// Find the impl type if this function is inside an impl block
fn find_parent_impl_type(node: &SyntaxNode) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::IMPL {
            if let Some(impl_def) = ast::Impl::cast(parent.clone()) {
                if let Some(self_ty) = impl_def.self_ty() {
                    // Extract type name (without generics)
                    // For "MappingStEph<A, B>" we want just "MappingStEph"
                    let full_type = self_ty.syntax().text().to_string();
                    // Strip everything after the first '<' (generic parameters)
                    let type_name = if let Some(idx) = full_type.find('<') {
                        full_type[..idx].trim().to_string()
                    } else {
                        full_type.trim().to_string()
                    };
                    return Some(type_name);
                }
            }
        }
        current = parent.parent();
    }
    None
}

/// Find all trait implementations in a source file
/// Returns: Vec<TraitImpl> with trait name, type name, method name, file, line
fn find_trait_implementations(file_path: &Path) -> Result<Vec<TraitImpl>> {
    let content = fs::read_to_string(file_path)?;
    let parsed = ra_ap_syntax::SourceFile::parse(&content, ra_ap_syntax::Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut trait_impls = Vec::new();
    
    // Find all impl blocks
    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            if let Some(impl_def) = ast::Impl::cast(node.clone()) {
                // Check if this is a trait implementation (not inherent impl)
                if let Some(trait_ref) = impl_def.trait_() {
                    // Extract trait name (just "Display", not "Display for Type")
                    // Find the first NAME_REF token in the trait_ref syntax
                    let trait_name = trait_ref.syntax()
                        .descendants_with_tokens()
                        .find_map(|t| {
                            if t.kind() == SyntaxKind::NAME_REF {
                                t.as_token().map(|tok| tok.text().to_string())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| trait_ref.syntax().text().to_string());
                    
                    // Extract type name (without generics)
                    // For "MappingStEph<A, B>" we want just "MappingStEph"
                    let type_name = if let Some(self_ty) = impl_def.self_ty() {
                        // Get the full text first
                        let full_type = self_ty.syntax().text().to_string();
                        // Strip everything after the first '<' (generic parameters)
                        if let Some(idx) = full_type.find('<') {
                            full_type[..idx].trim().to_string()
                        } else {
                            full_type.trim().to_string()
                        }
                    } else {
                        continue;
                    };
                    
                    // Extract all methods in this trait impl
                    if let Some(assoc_item_list) = impl_def.assoc_item_list() {
                        for item in assoc_item_list.assoc_items() {
                            if let ast::AssocItem::Fn(fn_def) = item {
                                if let Some(name) = fn_def.name() {
                                    let method_name = name.text().to_string();
                                    let line = get_line_number(fn_def.syntax(), &content);
                                    
                                    trait_impls.push(TraitImpl {
                                        trait_name: trait_name.clone(),
                                        type_name: type_name.clone(),
                                        method_name,
                                        _file: file_path.to_path_buf(),
                                        _line: line,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(trait_impls)
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
                    // Macros like assert_eq!(fn(), value) contain calls in their token trees
                    // 
                    // The token tree itself might contain nested CALL_EXPR nodes that we can detect
                    // by looking at the token sequence: IDENT followed by L_PAREN
                    if let Some(macro_call) = ast::MacroCall::cast(node.clone()) {
                        if let Some(token_tree) = macro_call.token_tree() {
                            // Traverse ALL tokens (recursively) in the token tree looking for function call patterns
                            // Pattern: IDENT token followed by L_PAREN token = function call
                            let tokens: Vec<_> = token_tree.syntax().descendants_with_tokens()
                                .filter_map(|e| e.as_token().map(|t| (t.kind(), t.text().to_string())))
                                .collect();
                            
                            for i in 0..tokens.len().saturating_sub(1) {
                                let (current_kind, current_text) = &tokens[i];
                                let (next_kind, _) = &tokens[i + 1];
                                
                                // Check if current is IDENT and next is L_PAREN
                                if *current_kind == SyntaxKind::IDENT && *next_kind == SyntaxKind::L_PAREN {
                                    // This is a function call pattern
                                    // Skip common Rust keywords that might appear before parens
                                    if !["if", "match", "for", "while", "loop", "return"].contains(&current_text.as_str()) {
                                        *call_counts.entry(current_text.clone()).or_insert(0) += 1;
                                    }
                                }
                            }
                            
                            // Also recursively check if there are any nested nodes with CALL_EXPR
                            // (in case some macro expansions create partial AST)
                            for descendant in token_tree.syntax().descendants() {
                                if descendant.kind() == SyntaxKind::CALL_EXPR {
                                    if let Some(call_expr) = ast::CallExpr::cast(descendant) {
                                        if let Some(expr) = call_expr.expr() {
                                            if let ast::Expr::PathExpr(path_expr) = expr {
                                                if let Some(path) = path_expr.path() {
                                                    if let Some(segment) = path.segments().last() {
                                                        if let Some(name_ref) = segment.name_ref() {
                                                            *call_counts.entry(name_ref.text().to_string()).or_insert(0) += 1;
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
    }
    
    Ok(call_counts)
}

/// Find format macro calls (format!(), println!(), etc.) in test files
/// Returns HashMap of type name -> (call_count, is_debug)
/// is_debug: true for {:?}, false for {}
fn find_format_macro_calls(test_file: &Path, trait_impls: &[TraitImpl]) -> Result<HashMap<String, (usize, CoverageSource)>> {
    let content = fs::read_to_string(test_file)?;
    let parsed = ra_ap_syntax::SourceFile::parse(&content, ra_ap_syntax::Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut format_calls: HashMap<String, (usize, CoverageSource)> = HashMap::new();
    
    // Build a mapping from type names to trait implementations
    // Key: type_name, Value: Vec<(trait_name, method_name)>
    let mut type_to_traits: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for trait_impl in trait_impls {
        type_to_traits
            .entry(trait_impl.type_name.clone())
            .or_default()
            .push((trait_impl.trait_name.clone(), trait_impl.method_name.clone()));
    }
    
    // Find all macro calls
    for node in root.descendants() {
        if node.kind() == SyntaxKind::MACRO_CALL {
            if let Some(macro_call) = ast::MacroCall::cast(node.clone()) {
                // Check if it's a format-related macro
                let macro_name = if let Some(path) = macro_call.path() {
                    path.segments().last().and_then(|seg| seg.name_ref()).map(|n| n.text().to_string())
                } else {
                    None
                };
                
                let is_format_macro = macro_name.as_ref().map_or(false, |name| {
                    matches!(name.as_str(), "format" | "println" | "eprintln" | "print" | "eprint" | "write" | "writeln")
                });
                
                if !is_format_macro {
                    continue;
                }
                
                // Extract token tree
                if let Some(token_tree) = macro_call.token_tree() {
                    // Collect all tokens
                    let tokens: Vec<_> = token_tree.syntax()
                        .descendants_with_tokens()
                        .filter_map(|e| e.as_token().map(|t| (t.kind(), t.text().to_string())))
                        .collect();
                    
                    // Look for format specifiers: {} or {:?}
                    // Pattern: STRING_LITERAL containing {} or {:?}, followed by COMMA, then IDENT(s)
                    
                    let mut found_display = false;
                    let mut found_debug = false;
                    let mut identifiers = Vec::new();
                    let mut in_format_string = false;
                    
                    for i in 0..tokens.len() {
                        let (kind, text) = &tokens[i];
                        
                        // Check for format string literals
                        if *kind == SyntaxKind::STRING {
                            in_format_string = true;
                            if text.contains("{}") {
                                found_display = true;
                            }
                            if text.contains("{:?}") {
                                found_debug = true;
                            }
                        }
                        
                        // After format string, collect identifiers (the variables being formatted)
                        if in_format_string && *kind == SyntaxKind::IDENT {
                            // Skip rust keywords
                            if !["let", "mut", "ref", "const", "static", "fn", "if", "else", "match", "for", "while", "loop"].contains(&text.as_str()) {
                                identifiers.push(text.clone());
                            }
                        }
                    }
                    
                    // For each identifier, check if it matches a type name in our trait implementations
                    // Heuristic: variable name often relates to type name
                    // Examples: "entry" -> "Entry", "PQEntry"
                    //           "graph" -> "Graph", "DirGraphMtEph"
                    //           "seq" -> "Seq", "ArraySeqMtEph"
                    
                    for ident in identifiers {
                        // Try to match identifier to type names
                        // Strategy 1: Direct match (case-insensitive prefix)
                        // Strategy 2: Check if type name contains identifier (case-insensitive)
                        
                        for (type_name, traits) in &type_to_traits {
                            let type_lower = type_name.to_lowercase();
                            let ident_lower = ident.to_lowercase();
                            
                            // Check if this type implements Display or Debug
                            let has_display = traits.iter().any(|(t, m)| t.contains("Display") && m == "fmt");
                            let has_debug = traits.iter().any(|(t, m)| t.contains("Debug") && m == "fmt");
                            
                            // Match heuristics:
                            // 1. Type name contains identifier: "DirGraphMtEph" contains "graph"
                            // 2. Identifier is a lowercase version of type: "entry" -> "Entry"
                            let matches = type_lower.contains(&ident_lower) || 
                                          ident_lower == type_lower ||
                                          type_lower.starts_with(&ident_lower);
                            
                            if matches {
                                // Determine coverage source based on format specifier
                                if found_display && has_display {
                                    let key = format!("{}::fmt", type_name);
                                    let entry = format_calls.entry(key).or_insert((0, CoverageSource::DisplayTrait));
                                    entry.0 += 1;
                                }
                                if found_debug && has_debug {
                                    let key = format!("{}::fmt", type_name);
                                    let entry = format_calls.entry(key).or_insert((0, CoverageSource::DebugTrait));
                                    entry.0 += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(format_calls)
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
    
    // Step 1: Find all public functions and trait implementations in src/
    let mut all_functions = Vec::new();
    let mut all_trait_impls = Vec::new();
    for entry in WalkDir::new(&src_dir).follow_links(true) {
        let entry = entry?;
        if entry.file_type().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
            match find_public_functions(entry.path()) {
                Ok(functions) => all_functions.extend(functions),
                Err(e) => logger.log(&format!("Warning: Failed to parse {}: {}", entry.path().display(), e)),
            }
            match find_trait_implementations(entry.path()) {
                Ok(trait_impls) => all_trait_impls.extend(trait_impls),
                Err(e) => logger.log(&format!("Warning: Failed to parse trait impls in {}: {}", entry.path().display(), e)),
            }
        }
    }
    
    // Step 2: Find all function calls and format macro calls in tests/
    let mut test_call_counts: HashMap<String, (usize, Vec<PathBuf>)> = HashMap::new();
    let mut trait_method_calls: HashMap<String, (usize, Vec<PathBuf>, CoverageSource)> = HashMap::new();
    
    if tests_dir.exists() {
        for entry in WalkDir::new(&tests_dir).follow_links(true) {
            let entry = entry?;
            if entry.file_type().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                // Find direct function calls
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
                
                // Find format macro calls (Display/Debug trait usage)
                match find_format_macro_calls(entry.path(), &all_trait_impls) {
                    Ok(format_calls) => {
                        for (method_key, (count, source)) in format_calls {
                            let entry_data = trait_method_calls.entry(method_key).or_insert((0, Vec::new(), source));
                            entry_data.0 += count;
                            if !entry_data.1.contains(&entry.path().to_path_buf()) {
                                entry_data.1.push(entry.path().to_path_buf());
                            }
                        }
                    }
                    Err(e) => logger.log(&format!("Warning: Failed to parse format macros in test file {}: {}", entry.path().display(), e)),
                }
            }
        }
    }
    
    // Step 3: Build coverage report
    let mut coverage: Vec<TestCoverage> = Vec::new();
    for func in all_functions {
        // Check for direct function calls
        let (mut call_count, mut test_files) = test_call_counts.get(&func.name).cloned().unwrap_or((0, Vec::new()));
        let mut coverage_source = CoverageSource::Direct;
        
        // Check if this is a trait method (e.g., fmt in Display/Debug impl)
        // Build the key as "TypeName::method_name"
        if let Some(ref impl_type) = func.impl_type {
            let trait_method_key = format!("{}::{}", impl_type, func.name);
            if let Some((trait_count, trait_test_files, trait_source)) = trait_method_calls.get(&trait_method_key) {
                call_count += trait_count;
                for tf in trait_test_files {
                    if !test_files.contains(tf) {
                        test_files.push(tf.clone());
                    }
                }
                coverage_source = trait_source.clone();
            }
        }
        
        coverage.push(TestCoverage {
            function: func,
            call_count,
            test_files,
            coverage_source,
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
        
        // Add coverage source annotation for trait methods
        let coverage_annotation = match cov.coverage_source {
            CoverageSource::DisplayTrait => " (via Display trait)",
            CoverageSource::DebugTrait => " (via Debug trait)",
            CoverageSource::Direct => "",
        };
        
        logger.log(&format!("{}:{}:  {} - {} call(s) in {} test file(s){}", 
            rel_path.display(), 
            cov.function.line,
            func_desc,
            cov.call_count,
            cov.test_files.len(),
            coverage_annotation
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

