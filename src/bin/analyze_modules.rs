use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use ra_ap_syntax::{ast, AstNode, SyntaxKind};
use rusticate::parse_file;

// Crates that wrap/re-export everything - we want to filter these out
const WRAPPER_CRATES: &[&str] = &[
    "tokio",
    "futures",
    "actix_web",
    "rocket",
    "axum",
    "bevy",
    "diesel",
    "sqlx",
    "polars",
    "ndarray",
    "serde",
    "async_std",
    "smol",
    "rayon",
];

struct Args {
    codebase: PathBuf,
    max_codebases: Option<usize>,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args_iter = std::env::args().skip(1);
        let mut codebase = None;
        let mut max_codebases = None;

        while let Some(arg) = args_iter.next() {
            match arg.as_str() {
                "-C" | "--codebase" => {
                    codebase = Some(PathBuf::from(
                        args_iter
                            .next()
                            .context("Expected path after -C/--codebase")?
                    ));
                }
                "-m" | "--max-codebases" => {
                    let max = args_iter
                        .next()
                        .context("Expected number after -m/--max-codebases")?
                        .parse::<usize>()
                        .context("Invalid number for -m/--max-codebases")?;
                    max_codebases = Some(max);
                }
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => {
                    bail!("Unknown argument: {}\nRun with --help for usage", arg);
                }
            }
        }

        // Fail fast on missing required argument
        let codebase = codebase.context(
            "Missing required argument: -C/--codebase\nRun with --help for usage"
        )?;

        // Fail fast if path doesn't exist
        if !codebase.exists() {
            bail!("Codebase path does not exist: {}", codebase.display());
        }

        // Fail fast if not a directory
        if !codebase.is_dir() {
            bail!("Codebase path is not a directory: {}", codebase.display());
        }

        Ok(Args { 
            codebase,
            max_codebases,
        })
    }
}

fn print_help() {
    println!(
        r#"rusticate-analyze-modules - Analyze module usage in codebases

USAGE:
    rusticate-analyze-modules -C <PATH> [-m <N>]

OPTIONS:
    -C, --codebase <PATH>       Path to a codebase, or a directory of codebases [required]
    -m, --max-codebases <N>     Limit number of codebases to analyze (default: unlimited)
    -h, --help                  Print this help message

DESCRIPTION:
    Analyzes which modules are used in Rust codebases. Filters out wrapper
    crates like tokio, futures, etc. that re-export everything.
    
    -C can accept:
      - A single codebase directory (e.g., ~/projects/my-project)
      - A directory containing multiple codebases (e.g., ~/projects/VerusCodebases)
    
    Provides two analyses:
    1. Which std modules are called in total
    2. Which data structures (types/structs) from each module are used
    
    Also tracks vstd usage specifically for Verus projects.

EXAMPLES:
    # Analyze a single codebase
    rusticate-analyze-modules -C ~/projects/my-project

    # Analyze a directory containing multiple codebases
    rusticate-analyze-modules -C ~/projects/VerusCodebases

    # Test with first 5 codebases only
    rusticate-analyze-modules -C ~/projects/VerusCodebases -m 5

    # Analyze modules in multiple codebases individually
    for dir in ~/projects/VerusCodebases/*; do
        rusticate-analyze-modules -C "$dir"
    done
"#
    );
}

#[derive(Debug, Clone)]
struct ModuleUsage {
    module_path: String,
    file: PathBuf,
    line: usize,
}

fn extract_paths_from_use_tree(use_tree: &ast::UseTree, prefix: String) -> Vec<String> {
    let mut paths = Vec::new();
    
    if let Some(path) = use_tree.path() {
        let path_str = path.to_string();
        let full_path = if prefix.is_empty() {
            path_str.clone()
        } else {
            format!("{}::{}", prefix, path_str)
        };
        
        // Check if there's a UseTreeList (grouped imports like {A, B, C})
        if let Some(use_tree_list) = use_tree.use_tree_list() {
            // Recurse into each item in the list
            for child_tree in use_tree_list.use_trees() {
                paths.extend(extract_paths_from_use_tree(&child_tree, full_path.clone()));
            }
        } else {
            // Simple path - just add it
            paths.push(full_path);
        }
    } else if let Some(use_tree_list) = use_tree.use_tree_list() {
        // List without a path (e.g., use {self, A, B})
        for child_tree in use_tree_list.use_trees() {
            paths.extend(extract_paths_from_use_tree(&child_tree, prefix.clone()));
        }
    }
    
    paths
}

fn extract_use_paths(file: &Path) -> Result<Vec<(String, usize)>> {
    let content = fs::read_to_string(file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;
    
    let parse = parse_file(&content)?;
    let root = parse.syntax();
    
    let mut uses = Vec::new();

    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                if let Some(use_tree) = use_item.use_tree() {
                    // Extract paths properly via AST traversal
                    let paths = extract_paths_from_use_tree(&use_tree, String::new());
                    
                    // Calculate line number by counting newlines before this node
                    let offset = node.text_range().start().into();
                    let line = content[..offset].lines().count();
                    
                    // Add all extracted paths with the same line number
                    for path in paths {
                        uses.push((path, line));
                    }
                }
            }
        }
    }

    Ok(uses)
}

fn is_wrapper_crate(path: &str) -> bool {
    // Check if the path starts with any wrapper crate
    for wrapper in WRAPPER_CRATES {
        if path == *wrapper || path.starts_with(&format!("{}::", wrapper)) {
            return true;
        }
        // Also check for crate:: prefix
        if path == format!("crate::{}", wrapper) || 
           path.starts_with(&format!("crate::{}::", wrapper)) {
            return true;
        }
    }
    false
}

fn find_rust_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map_or(false, |ext| ext == "rs")
                && !e.path().to_string_lossy().contains("/target/")
        })
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn find_codebases(dir: &Path) -> Vec<PathBuf> {
    // Find subdirectories that look like codebases (have Cargo.toml or .rs files)
    let mut codebases = Vec::new();
    
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                // Check if this looks like a codebase
                let has_cargo = path.join("Cargo.toml").exists();
                let has_rust = find_rust_files(&path).len() > 0;
                
                if has_cargo || has_rust {
                    codebases.push(path);
                }
            }
        }
    }
    
    codebases.sort();
    codebases
}

fn main() -> Result<()> {
    let start = std::time::Instant::now();
    
    // Parse args with fail-fast validation
    let args = Args::parse()?;

    // Set up logging
    let log_path = PathBuf::from("analyses/analyze_modules.log");
    fs::create_dir_all("analyses")?;
    let mut log_file = fs::File::create(&log_path)
        .context("Failed to create log file")?;

    macro_rules! log {
        ($($arg:tt)*) => {
            writeln!(log_file, $($arg)*).ok();
        };
    }

    log!("rusticate-analyze-modules");
    log!("==========================");
    log!("Codebase: {}", args.codebase.display());
    if let Some(max) = args.max_codebases {
        log!("Max codebases: {}", max);
    }
    log!("Started at: {:?}\n", start);

    println!("rusticate-analyze-modules");
    println!("==========================");
    println!("Codebase: {}", args.codebase.display());
    if let Some(max) = args.max_codebases {
        println!("Max codebases: {}", max);
    }
    println!();

    // Check if this is a directory of codebases or a single codebase
    let codebases = find_codebases(&args.codebase);
    
    let codebases_to_analyze = if codebases.is_empty() {
        // Single codebase - analyze the given path directly
        vec![args.codebase.clone()]
    } else {
        // Multiple codebases - apply -m limit
        println!("Found {} codebases", codebases.len());
        log!("Found {} codebases", codebases.len());
        
        let mut limited = codebases;
        if let Some(max) = args.max_codebases {
            limited.truncate(max);
            println!("Limiting to {} codebases", limited.len());
            log!("Limiting to {} codebases", limited.len());
        }
        println!();
        log!("");
        limited
    };

    // Find all Rust files across selected codebases
    println!("Finding Rust files...");
    let mut rust_files = Vec::new();
    for codebase in &codebases_to_analyze {
        rust_files.extend(find_rust_files(codebase));
    }
    println!("Found {} Rust files across {} codebases", rust_files.len(), codebases_to_analyze.len());
    log!("Found {} Rust files across {} codebases\n", rust_files.len(), codebases_to_analyze.len());

    // Collect module usage
    let mut module_usage: Vec<ModuleUsage> = Vec::new();
    let mut vstd_usage: Vec<ModuleUsage> = Vec::new();
    let mut errors = 0;

    println!("Analyzing module usage...\n");
    
    for file in &rust_files {
        match extract_use_paths(file) {
            Ok(uses) => {
                for (use_path, line) in uses {
                    // Skip wrapper crates
                    if is_wrapper_crate(&use_path) {
                        continue;
                    }

                    let usage = ModuleUsage {
                        module_path: use_path.clone(),
                        file: file.clone(),
                        line,
                    };

                    // Check if it's vstd usage
                    if use_path.starts_with("vstd::") || use_path == "vstd" {
                        vstd_usage.push(usage.clone());
                    }

                    module_usage.push(usage);
                }
            }
            Err(e) => {
                errors += 1;
                log!("Error parsing {}: {}", file.display(), e);
            }
        }
    }

    println!("Analysis complete!");
    println!("  Total module uses: {}", module_usage.len());
    println!("  vstd uses: {}", vstd_usage.len());
    println!("  Parse errors: {}", errors);
    println!();

    log!("Analysis complete!");
    log!("  Total module uses: {}", module_usage.len());
    log!("  vstd uses: {}", vstd_usage.len());
    log!("  Parse errors: {}\n", errors);

    // Analyze vstd usage
    if !vstd_usage.is_empty() {
        println!("=== vstd Module Usage ===\n");
        log!("=== vstd Module Usage ===\n");

        // Count by module
        let mut vstd_modules: HashMap<String, usize> = HashMap::new();
        for usage in &vstd_usage {
            *vstd_modules.entry(usage.module_path.clone()).or_insert(0) += 1;
        }

        // Sort by usage count
        let mut sorted: Vec<_> = vstd_modules.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));

        println!("vstd modules by usage count:");
        log!("vstd modules by usage count:");
        for (module, count) in &sorted {
            println!("  {:4} {}", count, module);
            log!("  {:4} {}", count, module);
        }
        println!();
        log!("");

        // Show unique vstd modules used
        let unique_modules: HashSet<String> = vstd_usage
            .iter()
            .map(|u| u.module_path.clone())
            .collect();
        
        println!("Unique vstd modules: {}", unique_modules.len());
        log!("Unique vstd modules: {}", unique_modules.len());
        
        // Show file locations for vstd usage (first 10)
        println!("\nSample vstd usage locations:");
        log!("\nSample vstd usage locations:");
        for usage in vstd_usage.iter().take(10) {
            let msg = format!("  {}:{} - {}", 
                usage.file.display(), 
                usage.line, 
                usage.module_path);
            println!("{}", msg);
            log!("{}", msg);
        }
        if vstd_usage.len() > 10 {
            println!("  ... and {} more", vstd_usage.len() - 10);
            log!("  ... and {} more", vstd_usage.len() - 10);
        }
    } else {
        println!("No vstd usage found.");
        log!("No vstd usage found.");
    }

    // Summary of all modules (excluding wrappers)
    println!("\n=== Top Non-Wrapper Modules ===\n");
    log!("\n=== Top Non-Wrapper Modules ===\n");

    let mut all_modules: HashMap<String, usize> = HashMap::new();
    for usage in &module_usage {
        // Extract root crate from module path
        let root = if let Some(pos) = usage.module_path.find("::") {
            &usage.module_path[..pos]
        } else {
            &usage.module_path
        };
        *all_modules.entry(root.to_string()).or_insert(0) += 1;
    }

    let mut sorted_all: Vec<_> = all_modules.into_iter().collect();
    sorted_all.sort_by(|a, b| b.1.cmp(&a.1));

    println!("All modules by usage ({} total):", sorted_all.len());
    log!("All modules by usage ({} total):", sorted_all.len());
    for (module, count) in sorted_all.iter() {
        println!("  {:4} {}", count, module);
        log!("  {:4} {}", count, module);
    }

    let elapsed = start.elapsed();
    println!("\nCompleted in {} ms.", elapsed.as_millis());
    log!("\nCompleted in {} ms.", elapsed.as_millis());
    println!("Log written to: {}", log_path.display());

    Ok(())
}

