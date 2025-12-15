// Copyright (C) Brian G. Milnes 2025

//! Review: Detect string hacking instead of AST-based analysis
//!
//! Checks for patterns that indicate string manipulation on Rust source code
//! instead of proper AST traversal. See rules/RusticateRules.md Rule 1-4.
//!
//! Red flags detected:
//! - .find() or .contains() with Rust syntax patterns ("fn ", "impl ", etc.)
//! - Manual parenthesis/bracket depth counting
//! - String splitting on "::" or other Rust syntax
//! - Character-by-character parsing of source code
//! - trim_start_matches/trim_end_matches on syntax characters
//!
//! Binary: rusticate-review-string-hacking

use anyhow::Result;
use ra_ap_syntax::{
    ast::{self, AstNode, HasArgList},
    SyntaxKind, SourceFile, Edition
};
use std::fs;
use std::time::Instant;
use rusticate::{StandardArgs, format_number};


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_string_hacking.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    
    let rust_files = rusticate::find_rust_files(&args.paths);
    let search_dirs = args.get_search_dirs();
    
    if let Some(dir) = search_dirs.first() {
        log!("Entering directory '{}'", dir.display());
        log!("");
    }
    
    let mut total_violations = 0;
    
    for file_path in &rust_files {
        let source = fs::read_to_string(file_path)?;
        let violations = check_for_string_hacking(&source, file_path.to_str().unwrap())?;
        
        for violation in &violations {
            log!("{}", violation);
        }
        
        total_violations += violations.len();
    }
    
    log!("");
    log!("Total violations: {} files checked, {} violations found", 
             format_number(rust_files.len()), 
             format_number(total_violations));
    
    let elapsed = start.elapsed().as_millis();
    log!("\nCompleted in {}ms", elapsed);
    
    Ok(())
}

fn check_for_string_hacking(source: &str, file_path: &str) -> Result<Vec<String>> {
    let mut violations = Vec::new();
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Ok(violations); // Skip files with parse errors
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Check for suspicious method calls on source-like variables
    for node in root.descendants() {
        if node.kind() == SyntaxKind::METHOD_CALL_EXPR {
            if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
                let call_text = call.to_string();
                
                // Check for .find() or .contains() with Rust syntax patterns
                if let Some(name_ref) = call.name_ref() {
                    let method_name = name_ref.text();
                    
                    if method_name == "find" || method_name == "contains" {
                        if let Some(arg_list) = call.arg_list() {
                            // Check each argument - look for STRING_LITERAL nodes
                            for arg in arg_list.args() {
                                if let Some(literal_expr) = ast::Expr::cast(arg.syntax().clone()) {
                                    // Check if it's a literal expression
                                    if let ast::Expr::Literal(lit) = literal_expr {
                                        let token = lit.token();
                                        let token_text = token.text();
                                        // Check if the literal is a Rust syntax pattern
                                        let syntax_patterns = [
                                            "fn ", "impl ", "trait ", "struct ",
                                            "pub ", "use ", "mod ", "let ",
                                            "::", "(", ")", "{", "}",
                                            "<", ">", "&", "mut ",
                                        ];
                                        
                                        for pattern in &syntax_patterns {
                                            // Check if token is a string containing this pattern
                                            if token_text.contains(pattern) && token_text.starts_with('"') {
                                                // Get receiver to check if it's source-like
                                                if let Some(receiver) = call.receiver() {
                                                    let receiver_text = receiver.to_string();
                                                    if is_source_like_variable(&receiver_text) {
                                                        let line = get_line_number(source, node.text_range().start().into());
                                                        let arg_display = token_text;
                                                        violations.push(format!(
                                                            "{file_path}:{line}: String hacking detected: {receiver_text}.{method_name}({arg_display}) - Use AST traversal instead"
                                                        ));
                                                        break; // Only report once per call
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    // Check for .split("::") or other syntax splitting
                    if method_name == "split" {
                        if let Some(arg_list) = call.arg_list() {
                            for arg in arg_list.args() {
                                if let Some(literal_expr) = ast::Expr::cast(arg.syntax().clone()) {
                                    if let ast::Expr::Literal(lit) = literal_expr {
                                        let token = lit.token();
                                        let token_text = token.text();
                                        if token_text == "\"::\"" {
                                            if let Some(receiver) = call.receiver() {
                                                let receiver_text = receiver.to_string();
                                                if is_source_like_variable(&receiver_text) {
                                                    let line = get_line_number(source, node.text_range().start().into());
                                                    violations.push(format!(
                                                        "{}:{}: String hacking detected: {}.split(\"{}\") - Use ast::Path instead",
                                                        file_path, line, receiver_text, "::"
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    // Check for .replace() on code/source variables
                    if method_name == "replace" {
                        if let Some(receiver) = call.receiver() {
                            // Check if receiver is a path expression (variable reference)
                            if let ast::Expr::PathExpr(path_expr) = receiver {
                                if let Some(path) = path_expr.path() {
                                    // Get the variable name from the path
                                    if let Some(segment) = path.segments().last() {
                                        let var_name = segment.to_string();
                                        // Check for code-related variable names using AST name, not string contains
                                        let code_related = ["source", "body", "text", "result", "modified_body", "body_text"];
                                        if code_related.iter().any(|&name| var_name == name) {
                                            let line = get_line_number(source, node.text_range().start().into());
                                            violations.push(format!(
                                                "{file_path}:{line}: String hacking detected: {var_name}.replace() on code - Use AST node replacement instead"
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    // Check for trim_start_matches/trim_end_matches on syntax chars
                    if method_name == "trim_start_matches" || method_name == "trim_end_matches" {
                        if let Some(arg_list) = call.arg_list() {
                            for arg in arg_list.args() {
                                if let Some(literal_expr) = ast::Expr::cast(arg.syntax().clone()) {
                                    if let ast::Expr::Literal(lit) = literal_expr {
                                        let token = lit.token();
                                        let token_text = token.text();
                                        let syntax_chars = ["'{'", "'}'", "'('", "')'", "'<'", "'>'"];
                                        
                                        for char_pattern in &syntax_chars {
                                            if token_text == *char_pattern {
                                                if let Some(receiver) = call.receiver() {
                                                    let receiver_text = receiver.to_string();
                                                    if is_source_like_variable(&receiver_text) {
                                                        let line = get_line_number(source, node.text_range().start().into());
                                                        violations.push(format!(
                                                            "{file_path}:{line}: String hacking detected: {receiver_text}.{method_name}({token_text}) - Use AST node ranges instead"
                                                        ));
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
                    
                    // Check for .rfind() on source-like variables with char predicates
                    if method_name == "rfind" {
                        if let Some(receiver) = call.receiver() {
                            let receiver_text = receiver.to_string();
                            if is_source_like_variable(&receiver_text) {
                                if let Some(arg_list) = call.arg_list() {
                                    // Check if argument is a closure (indicates char-by-char processing)
                                    for arg in arg_list.args() {
                                        if let Some(closure_expr) = ast::ClosureExpr::cast(arg.syntax().clone()) {
                                            let line = get_line_number(source, node.text_range().start().into());
                                            violations.push(format!(
                                                "{file_path}:{line}: String hacking detected: {receiver_text}.rfind(<closure>) - Use AST traversal instead"
                                            ));
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    // Check for .to_string() on syntax-like variables
                    // BUT allow legitimate AST extraction patterns:
                    // - .name()?.text().to_string() - getting identifier from AST
                    // - .ident.to_string() - getting identifier from AST
                    // - .name_ref().to_string() - getting name reference from AST
                    // - source_file.to_string() - converting file path, not source code
                    if method_name == "to_string" {
                        if let Some(receiver) = call.receiver() {
                            let receiver_text = receiver.to_string();
                            
                            // Allow legitimate AST extraction patterns
                            let is_legitimate = is_legitimate_ast_extraction(&receiver_text);
                            
                            if !is_legitimate && is_syntax_like_variable(&receiver_text) {
                                let line = get_line_number(source, node.text_range().start().into());
                                violations.push(format!(
                                    "{file_path}:{line}: String hacking detected: {receiver_text}.to_string() - Extract path/items from AST structure, not raw text"
                                ));
                            }
                        }
                    }
                }
            }
        }
        
        // Check for manual depth counting patterns
        if node.kind() == SyntaxKind::LET_STMT {
            if let Some(let_stmt) = ast::LetStmt::cast(node.clone()) {
                // Check if this is initializing a "depth" variable to 0
                if let Some(pat) = let_stmt.pat() {
                    let pat_text = pat.to_string();
                    if pat_text.contains("depth") {
                        if let Some(init) = let_stmt.initializer() {
                            let init_text = init.to_string();
                            if init_text == "0" {
                                // Found `let mut depth = 0` - likely manual depth counting
                                // Check if there's a loop following that iterates over chars
                                let mut found_char_iteration = false;
                                if let Some(parent) = node.parent() {
                                    for sibling in parent.children() {
                                        if sibling.text_range().start() > node.text_range().end()
                                            && sibling.kind() == SyntaxKind::FOR_EXPR {
                                                // Check if the for loop iterates over a .chars() or .enumerate() call
                                                if let Some(for_expr) = ast::ForExpr::cast(sibling.clone()) {
                                                    if let Some(iterable) = for_expr.iterable() {
                                                        if let ast::Expr::MethodCallExpr(method_call) = iterable {
                                                            if let Some(name_ref) = method_call.name_ref() {
                                                                let method = name_ref.text();
                                                                if method == "chars" || method == "enumerate" {
                                                                    found_char_iteration = true;
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                    }
                                }
                                
                                if found_char_iteration {
                                    let line = get_line_number(source, node.text_range().start().into());
                                    violations.push(format!(
                                        "{file_path}:{line}: Manual depth counting detected - Use ast::CallExpr and .arg_list() instead"
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(violations)
}

fn is_source_like_variable(var_name: &str) -> bool {
    let source_names = ["source", "src", "code", "text", "content", "body", 
                        "result", "call_text", "callee_text", "impl_text",
                        "line", "lines", "fn_line", "search_line"];
    source_names.iter().any(|name| var_name.contains(name))
}

fn is_syntax_like_variable(var_name: &str) -> bool {
    let syntax_names = ["syntax", "node", "tree", "ast", "parse", "parsed", 
                        "use_tree", "item", "stmt", "expr",
                        "use_text", "path_text"];
    syntax_names.iter().any(|name| var_name.contains(name))
}

/// Check if a .to_string() call is a legitimate AST extraction pattern
/// These patterns extract data FROM a parsed AST, not parse source text
fn is_legitimate_ast_extraction(receiver_text: &str) -> bool {
    // Pattern: .name()?.text().to_string() - getting identifier name from AST
    // Pattern: .name().text().to_string()
    if receiver_text.contains(".name()") && receiver_text.contains(".text()") {
        return true;
    }
    
    // Pattern: .ident.to_string() or .ident().to_string() - getting identifier from AST node
    if receiver_text.contains(".ident") {
        return true;
    }
    
    // Pattern: .name_ref().to_string() - getting name reference from AST
    if receiver_text.contains(".name_ref()") {
        return true;
    }
    
    // Pattern: .text().to_string() - getting text from an already-parsed token
    // This is only valid if preceded by name()/ident extraction
    if receiver_text.ends_with(".text()") && !receiver_text.contains("syntax()") {
        return true;
    }
    
    // Pattern: source_file.to_string() - converting file path, not source code
    if receiver_text == "source_file" || receiver_text.ends_with("_file") 
       || receiver_text.contains("file_path") || receiver_text.contains("rel_path") {
        return true;
    }
    
    // Pattern: .to_token_stream().to_string() - converting AST to tokens for display
    if receiver_text.contains("to_token_stream()") {
        return true;
    }
    
    // Pattern: .syntax().text() - extracting text from any AST node
    // This is ALWAYS extracting from a parsed AST, never parsing source text
    // The AST has already done the work of finding the node
    if receiver_text.contains(".syntax().text()") {
        return true;
    }
    
    // Pattern: single letter variables in map closures (p, t, b, w, etc.)
    // These are typically AST node iterators
    if receiver_text.len() == 1 && receiver_text.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
        return true;
    }
    
    false
}

fn get_line_number(source: &str, byte_offset: usize) -> usize {
    source[..byte_offset].lines().count()
}

