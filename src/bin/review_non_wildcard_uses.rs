use ra_ap_syntax::{ast::{self, AstNode}, Edition, SyntaxKind};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_non_wildcard_uses.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum ViolationType {
    ToplevelWildcard, // (!) BOGUS
    Trait,            // (a)
    Function,         // (b)
    Type,             // (c)
}

impl ViolationType {
    fn letter(&self) -> &'static str {
        match self {
            ViolationType::ToplevelWildcard => "(!)",
            ViolationType::Trait => "(a)",
            ViolationType::Function => "(b)",
            ViolationType::Type => "(c)",
        }
    }
    
    fn description(&self) -> &'static str {
        match self {
            ViolationType::ToplevelWildcard => "(!) BOGUS top-level wildcard",
            ViolationType::Trait => "(a) single trait import",
            ViolationType::Function => "(b) single function import",
            ViolationType::Type => "(c) single type import",
        }
    }
}

fn get_final_import_name(use_item: &ast::Use) -> Option<String> {
    // Get the final segment of the path being imported
    if let Some(use_tree) = use_item.use_tree() {
        if let Some(path) = use_tree.path() {
            if let Some(last_segment) = path.segments().last() {
                return Some(last_segment.to_string());
            }
        }
    }
    None
}

fn has_rename(use_item: &ast::Use) -> bool {
    if let Some(use_tree) = use_item.use_tree() {
        use_tree.rename().is_some()
    } else {
        false
    }
}

fn categorize_import(use_item: &ast::Use) -> Option<ViolationType> {
    // Skip imports with "as" renames - these are legitimate type aliases
    if has_rename(use_item) {
        return None;
    }
    
    // Get the final imported name
    let item = get_final_import_name(use_item)?;
    
    // Skip macro imports (end with Lit)
    if item.ends_with("Lit") {
        return None; // Macro import, OK
    }
    
    // Check what's being imported
    if item.ends_with("Trait") {
        Some(ViolationType::Trait)
    } else {
        // Check if it's a function (lowercase) or Type (PascalCase)
        if !item.is_empty() {
            let first_char = item.chars().next()?;
            if first_char.is_lowercase() {
                Some(ViolationType::Function)
            } else if first_char.is_uppercase() {
                Some(ViolationType::Type)
            } else {
                None
            }
        } else {
            None
        }
    }
}

fn is_from_apas_ai(use_item: &ast::Use) -> bool {
    if let Some(use_tree) = use_item.use_tree() {
        if let Some(path) = use_tree.path() {
            // Check if the first segment is "apas_ai"
            if let Some(first_segment) = path.segments().next() {
                return first_segment.to_string() == "apas_ai";
            }
        }
    }
    false
}

fn is_wildcard_import(use_item: &ast::Use) -> bool {
    if let Some(use_tree) = use_item.use_tree() {
        use_tree.syntax().descendants_with_tokens()
            .any(|n| n.kind() == SyntaxKind::STAR)
    } else {
        false
    }
}

fn is_toplevel_wildcard(use_item: &ast::Use) -> bool {
    // Check if it's "use apas_ai::*;" (only 2 segments with wildcard)
    if let Some(use_tree) = use_item.use_tree() {
        if let Some(path) = use_tree.path() {
            let segments: Vec<_> = path.segments().collect();
            // Should have exactly 1 segment (apas_ai) and a wildcard
            if segments.len() == 1 && is_wildcard_import(use_item) {
                return segments[0].to_string() == "apas_ai";
            }
        }
    }
    false
}

fn check_file(file_path: &PathBuf) -> Vec<(usize, String, ViolationType)> {
    let mut violations = Vec::new();
    
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return violations,
    };
    
    let parse = ra_ap_syntax::SourceFile::parse(&content, Edition::Edition2021);
    let root = parse.syntax_node();
    
    // Find all use statements
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                // Check if it's importing from apas_ai
                if is_from_apas_ai(&use_item) {
                    let use_text = use_item.to_string().trim().to_string();
                    
                    // First check for BOGUS top-level wildcard "use apas_ai::*;"
                    if is_toplevel_wildcard(&use_item) {
                        let line = rusticate::line_number(&node, &content);
                        violations.push((line, use_text, ViolationType::ToplevelWildcard));
                        continue;
                    }
                    
                    // Check if it's NOT a wildcard import
                    if !is_wildcard_import(&use_item) {
                        if let Some(vtype) = categorize_import(&use_item) {
                            // Get line number
                            let line = rusticate::line_number(&node, &content);
                            violations.push((line, use_text, vtype));
                        }
                    }
                }
            }
        }
    }
    
    violations
}

fn main() {
    let standard_args = match rusticate::StandardArgs::parse() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };
    
    let start = std::time::Instant::now();
    
    let files = rusticate::find_rust_files(&standard_args.paths);
    
    let mut total_violations = 0;
    let mut files_with_violations = 0;
    let mut all_violations: Vec<(PathBuf, usize, String, ViolationType)> = Vec::new();
    
    let mut type_counts: HashMap<ViolationType, usize> = HashMap::new();
    type_counts.insert(ViolationType::ToplevelWildcard, 0);
    type_counts.insert(ViolationType::Trait, 0);
    type_counts.insert(ViolationType::Function, 0);
    type_counts.insert(ViolationType::Type, 0);
    
    for file_path in &files {
        let violations = check_file(file_path);
        
        if !violations.is_empty() {
            files_with_violations += 1;
            total_violations += violations.len();
            
            for (line, use_stmt, vtype) in violations {
                *type_counts.entry(vtype.clone()).or_insert(0) += 1;
                all_violations.push((file_path.clone(), line, use_stmt, vtype));
            }
        }
    }
    
    // Always print all violations with type letter (Emacs-clickable)
    let cwd = std::env::current_dir().ok();
    for (file_path, line, use_stmt, vtype) in &all_violations {
        // Make path relative to CWD if possible
        let display_path = if let Some(ref cwd) = cwd {
            file_path.strip_prefix(cwd)
                .unwrap_or(file_path)
                .display()
                .to_string()
        } else {
            file_path.display().to_string()
        };
        
        log!("{}:{}: {} {}", display_path, line, vtype.letter(), use_stmt);
    }
    
    if !all_violations.is_empty() {
        log!("");
    }
    log!("{}", "=".repeat(80));
    log!("PARETO: BY VIOLATION TYPE");
    log!("{}", "=".repeat(80));
    
    let mut sorted_types: Vec<_> = type_counts.iter().collect();
    sorted_types.sort_by(|a, b| b.1.cmp(a.1));
    
    let mut cumulative = 0;
    for (vtype, count) in &sorted_types {
        cumulative += **count;
        let (percentage, cumulative_pct) = if total_violations > 0 {
            ((**count as f64 / total_violations as f64) * 100.0,
             (cumulative as f64 / total_violations as f64) * 100.0)
        } else {
            (0.0, 0.0)
        };
        log!("{:6} ({:5.1}%, cumulative {:5.1}%): {}",
            rusticate::format_number(**count), percentage, cumulative_pct, vtype.description());
    }
    
    log!("");
    log!("{}", "=".repeat(80));
    log!("PARETO: BY DIRECTORY");
    log!("{}", "=".repeat(80));
    
    // Group by directory - initialize all standard directories to 0
    let mut dir_counts: HashMap<String, usize> = HashMap::new();
    let mut dir_type_counts: HashMap<String, HashMap<ViolationType, usize>> = HashMap::new();
    
    // Initialize all standard directories to 0
    for dir in &["src", "tests", "benches"] {
        dir_counts.insert(dir.to_string(), 0);
        let mut type_map = HashMap::new();
        type_map.insert(ViolationType::ToplevelWildcard, 0);
        type_map.insert(ViolationType::Trait, 0);
        type_map.insert(ViolationType::Function, 0);
        type_map.insert(ViolationType::Type, 0);
        dir_type_counts.insert(dir.to_string(), type_map);
    }
    
    for (file_path, _, _, vtype) in &all_violations {
        let dir = if file_path.to_string_lossy().contains("/src/") {
            "src"
        } else if file_path.to_string_lossy().contains("/tests/") {
            "tests"
        } else if file_path.to_string_lossy().contains("/benches/") {
            "benches"
        } else {
            "other"
        };
        
        *dir_counts.entry(dir.to_string()).or_insert(0) += 1;
        *dir_type_counts.entry(dir.to_string())
            .or_default()
            .entry(vtype.clone())
            .or_insert(0) += 1;
    }
    
    let mut sorted_dirs: Vec<_> = dir_counts.iter().collect();
    // Sort by count descending, then by name for consistent ordering
    sorted_dirs.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    
    let mut cumulative = 0;
    for (dir, count) in &sorted_dirs {
        cumulative += **count;
        let (percentage, cumulative_pct) = if total_violations > 0 {
            ((**count as f64 / total_violations as f64) * 100.0,
             (cumulative as f64 / total_violations as f64) * 100.0)
        } else {
            (0.0, 0.0)
        };
        
        let type_breakdown = dir_type_counts.get(*dir).unwrap();
        let toplevel_count = type_breakdown.get(&ViolationType::ToplevelWildcard).unwrap_or(&0);
        let trait_count = type_breakdown.get(&ViolationType::Trait).unwrap_or(&0);
        let func_count = type_breakdown.get(&ViolationType::Function).unwrap_or(&0);
        let type_count = type_breakdown.get(&ViolationType::Type).unwrap_or(&0);
        
        log!("{:6} ({:5.1}%, cumulative {:5.1}%): {} [!:{}, a:{}, b:{}, c:{}]",
            rusticate::format_number(**count), percentage, cumulative_pct, dir,
            toplevel_count, trait_count, func_count, type_count);
    }
    
    log!("");
    log!("{}", "-".repeat(80));
    log!("Total non-wildcard uses: {}", rusticate::format_number(total_violations));
    log!("Files affected: {}/{}", 
        rusticate::format_number(files_with_violations),
        rusticate::format_number(files.len()));
    log!("");
    
    if total_violations > 0 {
        log!("✗ Found {} non-wildcard use statements in {} file(s)",
            rusticate::format_number(total_violations),
            rusticate::format_number(files_with_violations));
    } else {
        log!("✓ All apas_ai imports use wildcard imports");
    }
    
    log!("Completed in {}ms", start.elapsed().as_millis());
    
    std::process::exit(if total_violations > 0 { 1 } else { 0 });
}

