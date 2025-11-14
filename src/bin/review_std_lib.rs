use anyhow::Result;
use ra_ap_syntax::{ast, AstNode, SyntaxKind, SyntaxNode};
use rusticate::{find_rust_files, format_number, parse_file, StandardArgs};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::fs;
use std::time::Instant;

macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_std_lib.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

#[derive(Debug, Default)]
struct FileStdUsage {
    file: PathBuf,
    std_items: HashSet<String>,
}

#[derive(Debug, Default)]
struct ProjectStdUsage {
    name: String,
    files: Vec<FileStdUsage>,
    all_std_items: HashSet<String>,
}

#[derive(Debug, Default)]
struct MultiProjectAnalysis {
    projects: Vec<ProjectStdUsage>,
}

fn main() -> Result<()> {
    // Check flags BEFORE parsing StandardArgs
    let top100_mode = std::env::args().any(|arg| arg == "--top100");
    let coverage_mode = std::env::args().any(|arg| arg == "--coverage");

    if top100_mode {
        if coverage_mode {
            analyze_top100_with_coverage()?;
        } else {
            analyze_top100_projects()?;
        }
    } else {
        let args = StandardArgs::parse()?;
        analyze_single_project(&args)?;
    }

    Ok(())
}

fn analyze_single_project(args: &StandardArgs) -> Result<()> {
    let start_time = Instant::now();
    
    log!("Analyzing standard library usage...");
    log!("");

    let files = find_rust_files(&args.paths);
    let mut project = ProjectStdUsage::default();

    for file_path in &files {
        let usage = analyze_file_std_usage(file_path)?;
        
        if !usage.std_items.is_empty() {
            log!("{}:", file_path.display());
            let mut items: Vec<_> = usage.std_items.iter().collect();
            items.sort();
            for item in &items {
                log!("  std::{}", item);
                project.all_std_items.insert((*item).clone());
            }
            log!("");
            
            project.files.push(usage);
        }
    }

    print_summary(&project, files.len());
    
    let elapsed = start_time.elapsed();
    log!("");
    log!("Completed in {} ms.", elapsed.as_millis());

    Ok(())
}

fn analyze_top100_projects() -> Result<()> {
    let start_time = Instant::now();
    let top100_dir = Path::new("/home/milnes/projects/Top100Rust");
    
    log!("Analyzing std library usage across Top 100 Rust projects...");
    log!("============================================================");
    log!("");

    let mut multi = MultiProjectAnalysis::default();
    let mut project_count = 0;

    // Iterate through all project directories
    for entry in fs::read_dir(top100_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if !path.is_dir() || path.file_name().unwrap().to_str().unwrap().ends_with(".txt") {
            continue;
        }
        
        project_count += 1;
        let project_name = path.file_name().unwrap().to_str().unwrap().to_string();
        
        log!("[{}] Analyzing {}...", project_count, project_name);
        
        // Find src directories
        let mut src_dirs = Vec::new();
        for src_path in [path.join("src"), path.join("*/src")] {
            if src_path.exists() && src_path.is_dir() {
                src_dirs.push(src_path);
            }
        }
        
        // Also check for workspace members
        for subdir_entry in fs::read_dir(&path).unwrap_or_else(|_| fs::read_dir(".").unwrap()) {
            if let Ok(subdir) = subdir_entry {
                let subdir_path = subdir.path();
                if subdir_path.is_dir() {
                    let src_path = subdir_path.join("src");
                    if src_path.exists() && src_path.is_dir() {
                        src_dirs.push(src_path);
                    }
                }
            }
        }
        
        if src_dirs.is_empty() {
            log!("  → No src/ directory found, skipping");
            log!("");
            continue;
        }
        
        // Analyze this project
        let files = find_rust_files(&src_dirs);
        let mut project = ProjectStdUsage {
            name: project_name,
            ..Default::default()
        };
        
        for file_path in &files {
            if let Ok(usage) = analyze_file_std_usage(file_path) {
                let has_items = !usage.std_items.is_empty();
                for item in &usage.std_items {
                    project.all_std_items.insert(item.clone());
                }
                if has_items {
                    project.files.push(usage);
                }
            }
        }
        
        if project.files.is_empty() {
            log!("  → No std usage found");
        } else {
            log!("  ✓ Found {} files using std, {} unique items", 
                 project.files.len(), project.all_std_items.len());
        }
        log!("");
        
        multi.projects.push(project);
    }
    
    print_multi_project_histogram(&multi);
    print_project_rankings(&multi);
    
    let elapsed = start_time.elapsed();
    log!("");
    log!("Completed in {} ms.", elapsed.as_millis());

    Ok(())
}

fn analyze_top100_with_coverage() -> Result<()> {
    let start_time = Instant::now();
    let top100_dir = Path::new("/home/milnes/projects/Top100Rust");
    let std_src = Path::new("/home/milnes/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/std/src");
    
    log!("Analyzing std library usage WITH COVERAGE ANALYSIS...");
    log!("==========================================================");
    log!("");
    
    // First, run the normal Top100 analysis
    let mut multi = MultiProjectAnalysis::default();
    let mut project_count = 0;

    for entry in fs::read_dir(top100_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if !path.is_dir() || path.file_name().unwrap().to_str().unwrap().ends_with(".txt") {
            continue;
        }
        
        project_count += 1;
        let project_name = path.file_name().unwrap().to_str().unwrap().to_string();
        
        log!("[{}] Analyzing {}...", project_count, project_name);
        
        let mut src_dirs = Vec::new();
        for src_path in [path.join("src"), path.join("*/src")] {
            if src_path.exists() && src_path.is_dir() {
                src_dirs.push(src_path);
            }
        }
        
        for subdir_entry in fs::read_dir(&path).unwrap_or_else(|_| fs::read_dir(".").unwrap()) {
            if let Ok(subdir) = subdir_entry {
                let subdir_path = subdir.path();
                if subdir_path.is_dir() {
                    let src_path = subdir_path.join("src");
                    if src_path.exists() && src_path.is_dir() {
                        src_dirs.push(src_path);
                    }
                }
            }
        }
        
        if src_dirs.is_empty() {
            log!("  → No src/ directory found, skipping");
            log!("");
            continue;
        }
        
        let files = find_rust_files(&src_dirs);
        let mut project = ProjectStdUsage {
            name: project_name,
            ..Default::default()
        };
        
        for file_path in &files {
            if let Ok(usage) = analyze_file_std_usage(file_path) {
                let has_items = !usage.std_items.is_empty();
                for item in &usage.std_items {
                    project.all_std_items.insert(item.clone());
                }
                if has_items {
                    project.files.push(usage);
                }
            }
        }
        
        if project.files.is_empty() {
            log!("  → No std usage found");
        } else {
            log!("  ✓ Found {} files using std, {} unique items", 
                 project.files.len(), project.all_std_items.len());
        }
        log!("");
        
        multi.projects.push(project);
    }
    
    // Now analyze std library source for available functions
    log!("Analyzing std library source for available functions...");
    log!("");
    
    let std_functions = extract_std_functions(std_src)?;
    
    // Calculate coverage
    print_coverage_analysis(&multi, &std_functions);
    
    let elapsed = start_time.elapsed();
    log!("");
    log!("Completed in {} ms.", elapsed.as_millis());

    Ok(())
}

fn extract_std_functions(std_src: &Path) -> Result<HashMap<String, HashSet<String>>> {
    // Map of module -> set of public function names
    let mut functions: HashMap<String, HashSet<String>> = HashMap::new();
    
    // Scan std library source files
    for entry in walkdir::WalkDir::new(std_src)
        .min_depth(1)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rs"))
    {
        let file_path = entry.path();
        
        // Determine module name from path
        let module = determine_module_from_path(std_src, file_path);
        
        // Parse file and extract public functions
        if let Ok(source) = fs::read_to_string(file_path) {
            if let Ok(parse) = parse_file(&source) {
                let root = parse.syntax();
                let funcs = extract_public_functions(&root);
                
                let module_funcs = functions.entry(module).or_default();
                for func in funcs {
                    module_funcs.insert(func);
                }
            }
        }
    }
    
    Ok(functions)
}

fn determine_module_from_path(std_src: &Path, file_path: &Path) -> String {
    let relative = file_path.strip_prefix(std_src).unwrap_or(file_path);
    let parts: Vec<_> = relative.components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    
    if parts.is_empty() {
        return String::new();
    }
    
    // Remove .rs extension from last part if it's a file
    let mut module_parts = parts.clone();
    if let Some(last) = module_parts.last_mut() {
        if last.ends_with(".rs") {
            *last = last.strip_suffix(".rs").unwrap();
        }
        if *last == "mod" || *last == "lib" {
            module_parts.pop();
        }
    }
    
    module_parts.join("::")
}

fn extract_public_functions(node: &SyntaxNode) -> Vec<String> {
    let mut functions = Vec::new();
    
    for child in node.descendants() {
        if child.kind() == SyntaxKind::FN {
            // Check for "pub" keyword before the fn
            let text = child.to_string();
            let has_pub = child.prev_sibling_or_token()
                .map(|t| t.to_string().contains("pub"))
                .unwrap_or(false);
            
            if has_pub {
                // Extract function name - it's the first identifier after "fn"
                if let Some(name_start) = text.find("fn ") {
                    let after_fn = &text[name_start + 3..];
                    if let Some(name_end) = after_fn.find(|c: char| c == '(' || c == '<' || c.is_whitespace()) {
                        let name = after_fn[..name_end].trim();
                        if !name.is_empty() {
                            functions.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    
    functions
}

fn print_coverage_analysis(multi: &MultiProjectAnalysis, std_functions: &HashMap<String, HashSet<String>>) {
    log!("");
    log!("STD LIBRARY COVERAGE ANALYSIS");
    log!("=============================");
    log!("");
    
    // Collect all used function names by module
    let mut used_by_module: HashMap<String, HashSet<String>> = HashMap::new();
    
    for project in &multi.projects {
        for item in &project.all_std_items {
            if let Some(module) = item.split("::").next() {
                used_by_module
                    .entry(module.to_string())
                    .or_default()
                    .insert(item.clone());
            }
        }
    }
    
    // Calculate coverage per module
    let mut coverage_data: Vec<(String, usize, usize, f64)> = Vec::new();
    
    for (module, available) in std_functions {
        let used = used_by_module.get(module).map(|s| s.len()).unwrap_or(0);
        let total = available.len();
        let percentage = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        
        coverage_data.push((module.clone(), used, total, percentage));
    }
    
    coverage_data.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());
    
    log!("Module coverage (functions used / functions available):");
    log!("");
    
    for (module, used, total, percentage) in coverage_data {
        if total > 0 {
            log!("  {:20} {:4}/{:4} ({:5.1}%)", module, used, total, percentage);
        }
    }
}

fn analyze_file_std_usage(file_path: &PathBuf) -> Result<FileStdUsage> {
    let source = std::fs::read_to_string(file_path)?;
    let parse = parse_file(&source)?;
    let root = parse.syntax();

    let mut usage = FileStdUsage {
        file: file_path.clone(),
        std_items: HashSet::new(),
    };

    // Find all use statements
    collect_use_statements(&root, &mut usage.std_items);

    // Find all path expressions that reference std
    collect_path_expressions(&root, &mut usage.std_items);

    Ok(usage)
}

fn collect_use_statements(node: &SyntaxNode, std_items: &mut HashSet<String>) {
    for child in node.descendants() {
        if child.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(child) {
                if let Some(use_tree) = use_item.use_tree() {
                    extract_std_from_use_tree(&use_tree, "", std_items);
                }
            }
        }
    }
}

fn extract_std_from_use_tree(use_tree: &ast::UseTree, prefix: &str, std_items: &mut HashSet<String>) {
    if let Some(path) = use_tree.path() {
        let path_text = path.to_string();
        
        // Check if this path starts with std or if we're already in a std context
        let new_prefix = if prefix.is_empty() {
            if path_text.starts_with("std::") {
                path_text[5..].to_string()
            } else if path_text == "std" {
                String::new()
            } else {
                return;
            }
        } else {
            format!("{}::{}", prefix, path_text)
        };

        // Handle use tree list (e.g., use std::collections::{HashMap, HashSet};)
        if let Some(use_tree_list) = use_tree.use_tree_list() {
            for nested_tree in use_tree_list.use_trees() {
                extract_std_from_use_tree(&nested_tree, &new_prefix, std_items);
            }
        } else {
            // Single import
            let final_path = if new_prefix.is_empty() {
                path_text.strip_prefix("std::").unwrap_or(&path_text).to_string()
            } else {
                new_prefix
            };
            
            if !final_path.is_empty() && !final_path.contains("self") && !final_path.contains("super") {
                std_items.insert(final_path);
            }
        }
    }
}

fn collect_path_expressions(node: &SyntaxNode, std_items: &mut HashSet<String>) {
    for child in node.descendants() {
        if child.kind() == SyntaxKind::PATH {
            if let Some(path) = ast::Path::cast(child) {
                let path_text = path.to_string();
                
                // Check if this is a std path
                if path_text.starts_with("std::") {
                    let std_path = path_text[5..].to_string();
                    if !std_path.is_empty() {
                        // Get the module/item path (not including method calls)
                        // e.g., "collections::HashMap::new" -> "collections::HashMap"
                        std_items.insert(std_path);
                    }
                }
            }
        }
    }
}

fn print_summary(project: &ProjectStdUsage, total_files: usize) {
    log!("=====================================");
    log!("Standard Library Usage Summary");
    log!("=====================================");
    log!("");
    log!("Files analyzed: {}", format_number(total_files));
    log!("Files using std: {}", format_number(project.files.len()));
    log!("Unique std items: {}", format_number(project.all_std_items.len()));
    log!("");
    
    if !project.all_std_items.is_empty() {
        log!("All std items used across project:");
        log!("");
        
        // Group by top-level module
        let mut modules: HashMap<String, Vec<String>> = HashMap::new();
        
        for item in &project.all_std_items {
            let module = item.split("::").next().unwrap_or(item).to_string();
            modules.entry(module.clone()).or_default().push(item.clone());
        }
        
        let mut module_names: Vec<_> = modules.keys().collect();
        module_names.sort();
        
        for module_name in module_names {
            let items = modules.get(module_name).unwrap();
            log!("  std::{}:: ({} items)", module_name, items.len());
            
            let mut sorted_items = items.clone();
            sorted_items.sort();
            for item in sorted_items.iter().take(10) {
                log!("    - {}", item);
            }
            if items.len() > 10 {
                log!("    ... and {} more", items.len() - 10);
            }
            log!("");
        }
    }
}

fn print_multi_project_histogram(multi: &MultiProjectAnalysis) {
    log!("============================================================");
    log!("Standard Library Usage Histogram");
    log!("============================================================");
    log!("");
    
    // Build histogram: std_item -> set of projects using it
    let mut item_to_projects: HashMap<String, HashSet<String>> = HashMap::new();
    
    for project in &multi.projects {
        for item in &project.all_std_items {
            item_to_projects
                .entry(item.clone())
                .or_default()
                .insert(project.name.clone());
        }
    }
    
    // Create sorted list: (std_item, project_count)
    let mut histogram: Vec<(String, usize)> = item_to_projects
        .iter()
        .map(|(item, projects)| (item.clone(), projects.len()))
        .collect();
    
    // Sort by project count (descending), then by item name
    histogram.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    
    log!("Total projects analyzed: {}", multi.projects.len());
    log!("Total unique std items: {}", histogram.len());
    log!("");
    log!("Note: Numbers indicate how many projects (out of {}) use each item.", multi.projects.len());
    log!("");
    
    // Categorize items
    let mut modules = Vec::new();
    let mut traits = Vec::new();
    let mut types = Vec::new();
    let mut impls = Vec::new();
    
    for (item, count) in &histogram {
        if is_module(item) {
            modules.push((item.clone(), *count));
        } else if is_trait(item) {
            traits.push((item.clone(), *count));
        } else if is_impl(item) {
            impls.push((item.clone(), *count));
        } else {
            types.push((item.clone(), *count));
        }
    }
    
    // Print Modules
    log!("MODULES (number = projects using this module)");
    log!("==============================================");
    log!("");
    for (item, count) in &modules {
        log!("{:3} std::{}", count, item);
    }
    log!("");
    log!("Total modules: {}", modules.len());
    log!("");
    
    // Print Traits
    log!("TRAITS (number = projects using this trait)");
    log!("============================================");
    log!("");
    for (item, count) in &traits {
        log!("{:3} std::{}", count, item);
    }
    log!("");
    log!("Total traits: {}", traits.len());
    log!("");
    
    // Print Types
    log!("TYPES (number = projects using this type)");
    log!("==========================================");
    log!("");
    for (item, count) in &types {
        log!("{:3} std::{}", count, item);
    }
    log!("");
    log!("Total types: {}", types.len());
    log!("");
    
    // Print Impls
    log!("IMPLS (number = projects using this method/function)");
    log!("=====================================================");
    log!("");
    for (item, count) in &impls {
        log!("{:3} std::{}", count, item);
    }
    log!("");
    log!("Total impls: {}", impls.len());
    log!("");
    
    // Distribution summary
    log!("DISTRIBUTION");
    log!("============");
    log!("");
    log!("Shows how many std items are used by N projects.");
    log!("Format: N projects: X std items (meaning X items are each used by N projects)");
    log!("");
    
    let mut distribution: HashMap<usize, usize> = HashMap::new();
    for (_, count) in &histogram {
        *distribution.entry(*count).or_default() += 1;
    }
    
    let mut dist_sorted: Vec<_> = distribution.iter().collect();
    dist_sorted.sort_by(|a, b| b.0.cmp(a.0));
    
    for (proj_count, item_count) in dist_sorted {
        log!("{:3} projects use {:4} std items", proj_count, item_count);
    }
}

fn print_project_rankings(multi: &MultiProjectAnalysis) {
    log!("");
    log!("TOP PROJECTS BY STD USAGE");
    log!("=========================");
    log!("");
    
    // Rank projects by number of unique std items used
    let mut ranked: Vec<_> = multi.projects.iter()
        .filter(|p| !p.all_std_items.is_empty())
        .map(|p| (&p.name, p.all_std_items.len()))
        .collect();
    
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    
    log!("Top 20 projects by unique std items used:");
    log!("");
    
    for (i, (name, count)) in ranked.iter().take(20).enumerate() {
        log!("{:2}. {:3} std items - {}", i + 1, count, name);
    }
}

fn is_module(item: &str) -> bool {
    // Modules end with :: or are single-segment paths
    item.ends_with("::") || !item.contains("::")
}

fn is_trait(item: &str) -> bool {
    // Common trait names (not exhaustive, but covers most)
    let trait_names = [
        "Error", "Display", "Debug", "Clone", "Copy", "Default",
        "Eq", "PartialEq", "Ord", "PartialOrd", "Hash",
        "From", "Into", "TryFrom", "TryInto", "AsRef", "AsMut",
        "Deref", "DerefMut", "Drop", "Fn", "FnMut", "FnOnce",
        "Iterator", "IntoIterator", "ExactSizeIterator", "DoubleEndedIterator",
        "Future", "Stream", "Read", "Write", "Seek", "BufRead",
        "Send", "Sync", "Sized", "Unpin", "UnwindSafe", "RefUnwindSafe",
        "Hasher", "BuildHasher", "FromStr", "ToString",
    ];
    
    // Check if the last segment matches a known trait name
    if let Some(last_segment) = item.split("::").last() {
        // Remove generic parameters if present
        let base_name = last_segment.split('<').next().unwrap_or(last_segment);
        trait_names.contains(&base_name)
    } else {
        false
    }
}

fn is_impl(item: &str) -> bool {
    // Impls are methods/functions - they have a function name as the last segment
    // Function names typically start with lowercase or contain underscores
    if let Some(last_segment) = item.split("::").last() {
        // Remove generic parameters and method call parens if present
        let base_name = last_segment
            .split('<').next().unwrap_or(last_segment)
            .split('(').next().unwrap_or(last_segment);
        
        // Skip if it's empty or is a module (ends with ::)
        if base_name.is_empty() || item.ends_with("::") {
            return false;
        }
        
        // Check if it starts with lowercase (function/method convention in Rust)
        // or contains underscore (snake_case function name)
        let first_char = base_name.chars().next().unwrap_or('X');
        first_char.is_lowercase() || base_name.contains('_')
    } else {
        false
    }
}

