use anyhow::Result;
use ra_ap_syntax::{ast, AstNode, Edition, NodeOrToken, SourceFile, SyntaxKind, SyntaxNode};
use rusticate::{find_rust_files, StandardArgs};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/fix_min_typing.log")
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
    line: usize,
    category: SimplificationCategory,
}

fn get_line_number(node: &SyntaxNode, content: &str) -> usize {
    let offset = node.text_range().start();
    content[..usize::from(offset)].lines().count()
}

struct RewriteContext {
    // For TypeConstructor: when we see "TypeName::", add turbofish
    target_type_name: Option<String>,
    generic_args: Option<String>,
    seen_target: bool,
    
    // For Collect: when we see "collect", add turbofish
    collect_type: Option<String>,
    collect_node_ptr: usize, // Pointer to the specific collect node to rewrite
}

fn rewrite_file(file_path: &Path, dry_run: bool) -> Result<Vec<SimplifiableCase>> {
    let content = fs::read_to_string(file_path)?;
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();

    let mut cases = Vec::new();
    let mut output = String::new();
    
    rewrite_node(root, &content, &mut output, &mut cases, &RewriteContext {
        target_type_name: None,
        generic_args: None,
        seen_target: false,
        collect_type: None,
        collect_node_ptr: 0,
    });
    
    if !cases.is_empty() && !dry_run {
        fs::write(file_path, output)?;
    }

    Ok(cases)
}

fn rewrite_node(
    node: &SyntaxNode,
    content: &str,
    output: &mut String,
    cases: &mut Vec<SimplifiableCase>,
    ctx: &RewriteContext,
) {
    // Check if this is a LET statement we should rewrite
    if node.kind() == SyntaxKind::LET_STMT {
        if let Some(let_stmt) = ast::LetStmt::cast(node.clone()) {
            if should_rewrite_let_stmt(&let_stmt) {
                rewrite_let_stmt(&let_stmt, content, output, cases);
                return;
            }
        }
    }
    
    // Handle nodes within a rewrite context
    match node.kind() {
        SyntaxKind::CALL_EXPR if ctx.target_type_name.is_some() && !ctx.seen_target => {
            // Check if this is Type::method()
            if let Some(call_expr) = ast::CallExpr::cast(node.clone()) {
                if let Some(expr) = call_expr.expr() {
                    if let ast::Expr::PathExpr(path_expr) = expr {
                        if let Some(path) = path_expr.path() {
                            // Convert path to string to check pattern
                            let path_text = path.to_string();
                            if let Some(ref target_type) = ctx.target_type_name {
                                if path_text.starts_with(&format!("{}::", target_type)) {
                                    // This is Type::method() where we want Type::<T>::method()
                                    // Emit Type
                                    output.push_str(target_type);
                                    // Emit turbofish only if there are generic args
                                    if let Some(ref args) = ctx.generic_args {
                                        if !args.is_empty() {
                                            output.push_str("::");
                                            output.push_str(args);
                                        }
                                    }
                                    // Emit ::method_name
                                    output.push_str("::");
                                    // Get method name (everything after the first ::)
                                    if let Some(method_part) = path_text.strip_prefix(&format!("{}::", target_type)) {
                                        output.push_str(method_part);
                                    }
                                    // Emit arg list by finding ARG_LIST child
                                    for child in call_expr.syntax().children_with_tokens() {
                                        if child.kind() == SyntaxKind::ARG_LIST {
                                            match child {
                                                NodeOrToken::Node(n) => emit_node_text(&n, output),
                                                NodeOrToken::Token(t) => output.push_str(t.text()),
                                            }
                                        }
                                    }
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            // Fall through to default
        },
        SyntaxKind::METHOD_CALL_EXPR if ctx.collect_type.is_some() => {
            // Check if this is THE specific collect node we want to rewrite
            let node_ptr = node as *const SyntaxNode as usize;
            if node_ptr == ctx.collect_node_ptr {
                if let Some(method_call) = ast::MethodCallExpr::cast(node.clone()) {
                    if let Some(name_ref) = method_call.name_ref() {
                        if name_ref.text() == "collect" {
                            // Emit the receiver (without context, so nested collects are not affected)
                            if let Some(receiver) = method_call.receiver() {
                                rewrite_node(receiver.syntax(), content, output, cases, &RewriteContext {
                                    target_type_name: None,
                                    generic_args: None,
                                    seen_target: false,
                                    collect_type: None,
                                    collect_node_ptr: 0,
                                });
                            }
                            output.push_str(".collect");
                            // Add turbofish
                            if let Some(ref coll_type) = ctx.collect_type {
                                output.push_str("::");
                                output.push_str(coll_type);
                            }
                            // Emit arg list
                            for child in method_call.syntax().children_with_tokens() {
                                if child.kind() == SyntaxKind::ARG_LIST {
                                    match child {
                                        NodeOrToken::Node(n) => emit_node_text(&n, output),
                                        NodeOrToken::Token(t) => output.push_str(t.text()),
                                    }
                                }
                            }
                            return;
                        }
                    }
                }
            }
            // Fall through to default
        },
        _ => {}
    }
    
    // Default: emit this node's text by recursing through children
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => rewrite_node(&n, content, output, cases, ctx),
            NodeOrToken::Token(t) => output.push_str(t.text()),
        }
    }
}

impl Clone for RewriteContext {
    fn clone(&self) -> Self {
        RewriteContext {
            target_type_name: self.target_type_name.clone(),
            generic_args: self.generic_args.clone(),
            seen_target: self.seen_target,
            collect_type: self.collect_type.clone(),
            collect_node_ptr: self.collect_node_ptr,
        }
    }
}

fn should_rewrite_let_stmt(let_stmt: &ast::LetStmt) -> bool {
    if let_stmt.ty().is_none() {
        return false;
    }
    if let_stmt.initializer().is_none() {
        return false;
    }
    
    let type_ref = let_stmt.ty().unwrap();
    let initializer = let_stmt.initializer().unwrap();
    let init_syntax = initializer.syntax();
    
    // Check if already has turbofish
    if has_turbofish(init_syntax) {
        return true;
    }
    
    // Check for collect()
    if is_toplevel_collect(init_syntax) {
        return true;
    }
    
    // Check for Type::method()
    if let Some(type_name) = extract_base_type_name(&type_ref) {
        if contains_path_starting_with(init_syntax, &type_name) {
            return true;
        }
    }
    
    false
}

fn rewrite_let_stmt(
    let_stmt: &ast::LetStmt,
    content: &str,
    output: &mut String,
    cases: &mut Vec<SimplifiableCase>,
) {
    let type_ref = let_stmt.ty().unwrap();
    let initializer = let_stmt.initializer().unwrap();
    let pat = let_stmt.pat().unwrap();
    let init_syntax = initializer.syntax();
    
    // Emit "let pattern = "
    output.push_str("let ");
    for child in pat.syntax().children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => emit_node_text(&n, output),
            NodeOrToken::Token(t) => output.push_str(t.text()),
        }
    }
    output.push_str(" = ");
    
    // Determine category and context
    let (category, ctx) = if is_toplevel_collect(init_syntax) {
        let type_str = type_ref.to_string();
        let generic_str = format!("<{}>", type_str);
        let node_ptr = init_syntax as *const SyntaxNode as usize;
        (
            SimplificationCategory::Collect,
            RewriteContext {
                target_type_name: None,
                generic_args: None,
                seen_target: false,
                collect_type: Some(generic_str),
                collect_node_ptr: node_ptr,
            }
        )
    } else if has_turbofish(init_syntax) {
        (
            SimplificationCategory::TypeConstructor,
            RewriteContext {
                target_type_name: None,
                generic_args: None,
                seen_target: false,
                collect_type: None,
                collect_node_ptr: 0,
            }
        )
    } else {
        let type_name = extract_base_type_name(&type_ref).unwrap();
        let generics = extract_generic_args_string(&type_ref);
        (
            SimplificationCategory::TypeConstructor,
            RewriteContext {
                target_type_name: Some(type_name),
                generic_args: Some(generics),
                seen_target: false,
                collect_type: None,
                collect_node_ptr: 0,
            }
        )
    };
    
    // Emit initializer with context
    rewrite_node(init_syntax, content, output, cases, &ctx);
    
    // Emit semicolon
    output.push(';');
    
    cases.push(SimplifiableCase {
        line: get_line_number(let_stmt.syntax(), content),
        category,
    });
}

fn emit_node_text(node: &SyntaxNode, output: &mut String) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => emit_node_text(&n, output),
            NodeOrToken::Token(t) => output.push_str(t.text()),
        }
    }
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

fn extract_generic_args_string(type_ref: &ast::Type) -> String {
    if let ast::Type::PathType(path_type) = type_ref {
        if let Some(path) = path_type.path() {
            if let Some(segment) = path.segment() {
                for child in segment.syntax().children_with_tokens() {
                    if child.kind() == SyntaxKind::GENERIC_ARG_LIST {
                        return child.to_string();
                    }
                }
            }
        }
    }
    String::new()
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

fn main() -> Result<()> {
    let start_time = Instant::now();
    
    if let Ok(mut file) = fs::File::create("analyses/fix_min_typing.log") {
        use std::io::Write;
        writeln!(file, "=== Fix Min Typing ===")?;
    }

    // Check for --help flag and print custom help
    let cmd_args: Vec<String> = std::env::args().collect();
    if cmd_args.len() > 1 && (cmd_args[1] == "--help" || cmd_args[1] == "-h") {
        println!("Usage: fix-min-typing [OPTIONS]");
        println!();
        println!("Options:");
        println!("  -c, --codebase             Analyze src/, tests/, benches/ (default)");
        println!("  -d, --dir DIR [DIR...]     Analyze specific directories");
        println!("  -f, --file FILE            Analyze a single file");
        println!("  -m, --module NAME          Find module in src/ and its tests/benches");
        println!("  -n, --dry-run              Show what would be changed without modifying files");
        println!("  -h, --help                 Show this help message");
        println!();
        println!("Examples:");
        println!("  fix-min-typing                           # Analyze codebase (src/, tests/, benches/)");
        println!("  fix-min-typing -c                        # Same as above");
        println!("  fix-min-typing --dry-run -c              # Preview changes without applying");
        println!("  fix-min-typing -d src tests benches      # Analyze multiple directories");
        println!("  fix-min-typing -d src                    # Analyze just src/");
        println!("  fix-min-typing -f src/lib.rs             # Analyze single file");
        println!("  fix-min-typing -m ArraySeqStEph          # Analyze module + tests + benches");
        std::process::exit(0);
    }

    // Check for --dry-run flag
    let dry_run = cmd_args.iter().any(|arg| arg == "--dry-run" || arg == "-n");

    let args = StandardArgs::parse()?;
    let current_dir = std::env::current_dir()?;

    if dry_run {
        log!("DRY RUN MODE - No files will be modified");
    }
    log!("Entering directory '{}'", current_dir.display());
    println!();

    let target_dirs = args.base_dir();
    let files = find_rust_files(&[target_dirs]);

    let mut total_fixes = 0;
    let mut files_modified = 0;
    let mut by_category: HashMap<SimplificationCategory, usize> = HashMap::new();

    for file in &files {
        let cases = rewrite_file(file, dry_run)?;
        
        if !cases.is_empty() {
            files_modified += 1;
            let rel_path = file.strip_prefix(&current_dir).unwrap_or(file);
            
            let mut type_constructor_count = 0;
            let mut collect_count = 0;
            
            for case in &cases {
                match case.category {
                    SimplificationCategory::TypeConstructor => type_constructor_count += 1,
                    SimplificationCategory::Collect => collect_count += 1,
                }
            }
            
            let action = if dry_run { "Would fix" } else { "Fixing" };
            log!("{} {} cases in {}...", action, cases.len(), rel_path.display());
            if type_constructor_count > 0 {
                log!("  TypeConstructor: {}", type_constructor_count);
                *by_category.entry(SimplificationCategory::TypeConstructor).or_insert(0) += type_constructor_count;
            }
            if collect_count > 0 {
                log!("  Collect: {}", collect_count);
                *by_category.entry(SimplificationCategory::Collect).or_insert(0) += collect_count;
            }
            
            total_fixes += cases.len();
        }
    }

    log!("");
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    if dry_run {
        log!("  Files that would be modified: {}", files_modified);
        log!("  Total fixes that would be applied: {}", total_fixes);
    } else {
        log!("  Files modified: {}", files_modified);
        log!("  Total fixes applied: {}", total_fixes);
    }
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());

    Ok(())
}
