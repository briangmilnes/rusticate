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
    jobs: usize,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args_iter = std::env::args().skip(1);
        let mut codebase = None;
        let mut max_codebases = None;
        let mut jobs = 4;

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
                "-j" | "--jobs" => {
                    jobs = args_iter
                        .next()
                        .context("Expected number after -j/--jobs")?
                        .parse::<usize>()
                        .context("Invalid number for -j/--jobs")?;
                    if jobs == 0 {
                        bail!("--jobs must be at least 1");
                    }
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
            jobs,
        })
    }
}

fn print_help() {
    println!(
        r#"rusticate-analyze-modules - Analyze module usage in codebases

USAGE:
    rusticate-analyze-modules -C <PATH> [-m <N>] [-j <N>]

OPTIONS:
    -C, --codebase <PATH>       Path to a codebase, or a directory of codebases [required]
    -m, --max-codebases <N>     Limit number of codebases to analyze (default: unlimited)
    -j, --jobs <N>              Number of parallel threads (default: 4)
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
    # Analyze a single codebase (default 4 threads)
    rusticate-analyze-modules -C ~/projects/my-project

    # Analyze with 8 threads for faster processing
    rusticate-analyze-modules -C ~/projects/RustCodebases -j 8

    # Use all 64 threads! 🚀
    rusticate-analyze-modules -C ~/projects/RustCodebases -j 64

    # Test with first 5 codebases, 4 threads (don't hog CPU)
    rusticate-analyze-modules -C ~/projects/RustCodebases -m 5 -j 4

NOTE:
    Default is -j 4 to leave CPU for other work. Crank it up for speed!
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
        // Extract path properly via AST segments
        let path_str = path.segments()
            .map(|seg| seg.to_string())
            .collect::<Vec<_>>()
            .join("::");
            
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
    log!("Command: {}", std::env::args().collect::<Vec<_>>().join(" "));
    log!("Codebase: {} | Max: {} | Jobs: {} | Started: {:?}", 
        args.codebase.display(), 
        args.max_codebases.map_or("unlimited".to_string(), |m| m.to_string()),
        args.jobs,
        start);

    println!("rusticate-analyze-modules");
    println!("==========================");
    println!("Codebase: {}", args.codebase.display());
    if let Some(max) = args.max_codebases {
        println!("Max codebases: {}", max);
    }
    println!("Jobs: {}", args.jobs);
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

    // Parallel analysis with per-thread aggregation
    println!("Analyzing standard library usage with {} threads...\n", args.jobs);
    
    let builtin_libs = ["std", "core", "alloc", "proc_macro", "test"];
    let chunk_size = (rust_files.len() + args.jobs - 1) / args.jobs;
    let mut handles = Vec::new();

    for chunk in rust_files.chunks(chunk_size) {
        let chunk = chunk.to_vec();
        let builtin_libs = builtin_libs.to_vec();
        
        let handle = std::thread::spawn(move || {
            let mut local_usage: HashMap<String, usize> = HashMap::new();
            let mut local_errors = Vec::new();
            
            for file in chunk {
                match extract_use_paths(&file) {
                    Ok(uses) => {
                        for (use_path, _line) in uses {
                            // Only track built-in Rust libraries
                            let is_builtin = builtin_libs.iter().any(|&lib| {
                                use_path == lib || use_path.starts_with(&format!("{}::", lib))
                            });
                            
                            if is_builtin {
                                *local_usage.entry(use_path).or_insert(0) += 1;
                            }
                        }
                    }
                    Err(_) => {
                        local_errors.push(file);
                    }
                }
            }
            
            (local_usage, local_errors)
        });
        
        handles.push(handle);
    }

    // Merge results from all threads
    let mut std_lib_usage: HashMap<String, usize> = HashMap::new();
    let mut error_files = Vec::new();
    
    for handle in handles {
        let (local_usage, local_errors) = handle.join().unwrap();
        
        // Merge usage counts
        for (path, count) in local_usage {
            *std_lib_usage.entry(path).or_insert(0) += count;
        }
        
        // Collect errors
        error_files.extend(local_errors);
    }
    
    let errors = error_files.len();
    
    // Log errors
    for file in &error_files {
        log!("{}:1: Parse error", file.display());
    }

    println!("Analysis complete!");
    println!("  Standard library uses: {}", std_lib_usage.values().sum::<usize>());
    println!("  Unique std paths: {}", std_lib_usage.len());
    println!("  Parse errors: {}", errors);
    println!();

    log!("Analysis complete: {} std lib uses, {} unique paths, {} errors", 
        std_lib_usage.values().sum::<usize>(), std_lib_usage.len(), errors);

    // Separate modules from data types
    let mut modules_only: HashMap<String, usize> = HashMap::new();
    let mut data_types_used: HashMap<String, usize> = HashMap::new();
    
    for (path, count) in &std_lib_usage {
        // Count segments - more than 2 means it has a type/trait/item
        let segments: Vec<&str> = path.split("::").collect();
        
        if segments.len() <= 2 || path.ends_with("::self") {
            // Just a module like "std::io", "std", or "std::io::self"
            *modules_only.entry(path.clone()).or_insert(0) += count;
        } else {
            // Has a type like "std::collections::HashMap"
            // ONLY count the full path as a data type, DON'T count parent as module
            *data_types_used.entry(path.clone()).or_insert(0) += count;
        }
    }

    // Display modules
    println!("\n=== Standard Library Modules Used ===\n");
    log!("\n=== Standard Library Modules Used ===");
    
    let mut sorted_modules: Vec<_> = modules_only.into_iter().collect();
    sorted_modules.sort_by(|a, b| b.1.cmp(&a.1));
    
    println!("Modules ({} total):", sorted_modules.len());
    log!("Modules ({} total):", sorted_modules.len());
    for (module, count) in sorted_modules.iter() {
        println!("  {:4} {}", count, module);
        log!("  {:4} {}", count, module);
    }

    // Display data types
    println!("\n=== Standard Library Data Types Used ===\n");
    log!("\n=== Standard Library Data Types Used ===");
    
    let mut sorted_types: Vec<_> = data_types_used.into_iter().collect();
    sorted_types.sort_by(|a, b| b.1.cmp(&a.1));
    
    println!("Data types ({} total):", sorted_types.len());
    log!("Data types ({} total):", sorted_types.len());
    for (dtype, count) in sorted_types.iter() {
        println!("  {:4} {}", count, dtype);
        log!("  {:4} {}", count, dtype);
    }

    let elapsed = start.elapsed();
    println!("\nCompleted in {} ms.", elapsed.as_millis());
    log!("\nCompleted in {} ms.", elapsed.as_millis());
    println!("Log written to: {}", log_path.display());

    Ok(())
}

