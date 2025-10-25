use anyhow::Result;
use ra_ap_syntax::{ast, AstNode, Edition, SourceFile, SyntaxKind, SyntaxNode};
use rusticate::{find_rust_files, StandardArgs};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_min_typing.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum SimplificationCategory {
    TypeConstructor,
    Collect,
}

#[derive(Debug)]
struct SimplifiableCase {
    file: PathBuf,
    line: usize,
    variable: String,
    left_type: String,
    right_expr: String,
    category: SimplificationCategory,
}

fn get_line_number(node: &SyntaxNode, content: &str) -> usize {
    let offset = node.text_range().start();
    content[..usize::from(offset)].lines().count()
}

fn extract_base_type_name(type_ref: &ast::Type) -> Option<String> {
    if let ast::Type::PathType(path_type) = type_ref {
        if let Some(path) = path_type.path() {
            if let Some(segment) = path.segment() {
                if let Some(name_ref) = segment.name_ref() {
                    return Some(name_ref.text().to_string());
                }
            }
        }
    }
    None
}

fn contains_path_starting_with(node: &SyntaxNode, type_name: &str) -> bool {
    // Look for pattern: TypeName::something  
    // Walk all CALL_EXPR nodes and check their receiver PATH
    for child in node.descendants() {
        if child.kind() == SyntaxKind::CALL_EXPR {
            // Check if this is a call to Type::method()
            if let Some(call_expr) = ast::CallExpr::cast(child.clone()) {
                if let Some(expr) = call_expr.expr() {
                    // Check if it's a path expression
                    if let ast::Expr::PathExpr(path_expr) = expr {
                        if let Some(path) = path_expr.path() {
                            // Get the path as text
                            let path_text = path.to_string();
                            // Check if it starts with our type name followed by ::
                            if path_text.starts_with(&format!("{}::", type_name)) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_toplevel_collect(node: &SyntaxNode) -> bool {
    if node.kind() == SyntaxKind::METHOD_CALL_EXPR {
        if let Some(method_call) = ast::MethodCallExpr::cast(node.clone()) {
            if let Some(name_ref) = method_call.name_ref() {
                return name_ref.text() == "collect";
            }
        }
    }
    false
}

fn has_turbofish(syntax: &SyntaxNode) -> bool {
    for child in syntax.descendants() {
        if child.kind() == SyntaxKind::GENERIC_ARG_LIST {
            if let Some(prev) = child.prev_sibling_or_token() {
                if prev.kind() == SyntaxKind::COLON2 {
                    return true;
                }
            }
        }
    }
    false
}

fn analyze_file(file_path: &Path) -> Result<Vec<SimplifiableCase>> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut cases = Vec::new();
    
    // Find all LET statements - use the EXACT same logic as the fix tool
    for node in root.descendants() {
        if node.kind() == SyntaxKind::LET_STMT {
            if let Some(let_stmt) = ast::LetStmt::cast(node.clone()) {
                // Must have type annotation
                if let_stmt.ty().is_none() {
                    continue;
                }
                // Must have initializer
                if let_stmt.initializer().is_none() {
                    continue;
                }
                
                let type_ref = let_stmt.ty().unwrap();
                let initializer = let_stmt.initializer().unwrap();
                let init_syntax = initializer.syntax();
                
                let type_str = type_ref.to_string();
                let right_expr = initializer.to_string();
                
                let variable = if let Some(pat) = let_stmt.pat() {
                    pat.to_string()
                } else {
                    continue;
                };
                
                let line = get_line_number(&node, &content);
                
                // Check if already has turbofish - just remove type annotation
                if has_turbofish(init_syntax) {
                    cases.push(SimplifiableCase {
                        file: file_path.to_path_buf(),
                        line,
                        variable: variable.clone(),
                        left_type: type_str.clone(),
                        right_expr: right_expr.clone(),
                        category: SimplificationCategory::TypeConstructor,
                    });
                    continue;
                }
                
                // Check for top-level collect()
                if is_toplevel_collect(init_syntax) {
                    cases.push(SimplifiableCase {
                        file: file_path.to_path_buf(),
                        line,
                        variable,
                        left_type: type_str.clone(),
                        right_expr,
                        category: SimplificationCategory::Collect,
                    });
                    continue;
                }
                
                // Check for Type::method() pattern
                if let Some(type_name) = extract_base_type_name(&type_ref) {
                    if contains_path_starting_with(init_syntax, &type_name) {
                        cases.push(SimplifiableCase {
                            file: file_path.to_path_buf(),
                            line,
                            variable,
                            left_type: type_str,
                            right_expr,
                            category: SimplificationCategory::TypeConstructor,
                        });
                    }
                }
            }
        }
    }
    
    Ok(cases)
}

fn main() -> Result<()> {
    let start_time = Instant::now();
    
    if let Ok(mut file) = fs::File::create("analyses/review_min_typing.log") {
        use std::io::Write;
        writeln!(file, "=== Review Min Typing ===")?;
    }

    let args = StandardArgs::parse()?;
    let current_dir = std::env::current_dir()?;

    log!("Entering directory '{}'", current_dir.display());
    println!();

    let target_dirs = args.base_dir();
    let files = find_rust_files(&[target_dirs]);

    let mut total_cases = 0;
    let mut category_counts: HashMap<SimplificationCategory, usize> = HashMap::new();

    for file in &files {
        let cases = analyze_file(file)?;
        
        if !cases.is_empty() {
            total_cases += cases.len();
            
            for case in &cases {
                *category_counts.entry(case.category.clone()).or_insert(0) += 1;
                
                let rel_path = case.file.strip_prefix(&current_dir).unwrap_or(&case.file);
                let category_label = match case.category {
                    SimplificationCategory::TypeConstructor => "[TypeConstructor]",
                    SimplificationCategory::Collect => "[Collect]",
                };
                
                log!("{}:{}:  {}", rel_path.display(), case.line, category_label);
                log!("  Current:    let {}: {} = {};", case.variable, case.left_type, case.right_expr);
                log!("  Simplified: let {} = <simplified>;", case.variable);
                println!();
            }
        }
    }

    log!("");
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("By category:");
    if let Some(count) = category_counts.get(&SimplificationCategory::TypeConstructor) {
        log!("  Type constructors (Type<T> = Type::new()): {}", count);
    }
    if let Some(count) = category_counts.get(&SimplificationCategory::Collect) {
        log!("  Collect with type (Vec<T> = iter.collect()): {}", count);
    }
    log!("");
    log!("  TOTAL: {}", total_cases);
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());

    Ok(())
}
