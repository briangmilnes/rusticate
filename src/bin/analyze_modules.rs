use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use ra_ap_syntax::{ast, AstNode, SyntaxKind, SyntaxNode};
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
    codebase: Option<PathBuf>,
    max_codebases: Option<usize>,
    jobs: usize,
    rust_libs: bool,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args_iter = std::env::args().skip(1);
        let mut codebase = None;
        let mut max_codebases = None;
        let mut jobs = 4;
        let mut rust_libs = false;

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
                "-R" | "--rust-libs" => {
                    rust_libs = true;
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

        // Fail fast: need either -C or -R
        if codebase.is_none() && !rust_libs {
            bail!("Must specify either -C/--codebase or -R/--rust-libs\nRun with --help for usage");
        }

        // Validate codebase path if provided
        if let Some(ref cb) = codebase {
            if !cb.exists() {
                bail!("Codebase path does not exist: {}", cb.display());
            }
            if !cb.is_dir() {
                bail!("Codebase path is not a directory: {}", cb.display());
            }
        }

        Ok(Args { 
            codebase,
            max_codebases,
            jobs,
            rust_libs,
        })
    }
}

fn print_help() {
    println!(
        r#"rusticate-analyze-modules - Analyze module usage in codebases

USAGE:
    rusticate-analyze-modules -C <PATH> [-m <N>] [-j <N>]
    rusticate-analyze-modules -R [-j <N>]

OPTIONS:
    -C, --codebase <PATH>       Path to a codebase, or a directory of codebases
    -R, --rust-libs             Parse Rust stdlib (std/core/alloc) and count all functions
    -m, --max-codebases <N>     Limit number of codebases to analyze (default: unlimited)
    -j, --jobs <N>              Number of parallel threads (default: 4)
    -h, --help                  Print this help message

MODES:
    Either -C (analyze codebases) or -R (analyze Rust stdlib) is required.

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

// Struct to hold detailed function information
#[derive(Debug, Clone)]
struct FunctionInfo {
    name: String,
    module_path: String,
    context: FunctionContext,
    is_public: bool,
    is_unsafe: bool,
}

#[derive(Debug, Clone)]
enum FunctionContext {
    Standalone,
    Trait(String),      // trait name
    Impl(String),       // type name
}

fn extract_function_name(fn_node: &SyntaxNode) -> Option<String> {
    fn_node.children_with_tokens()
        .find(|c| c.kind() == SyntaxKind::NAME)
        .map(|n| n.to_string())
}

fn extract_impl_type(impl_node: &SyntaxNode) -> Option<String> {
    // Look for the type being implemented
    // Format: impl [<generics>] Type [<generics>] [for Trait] { ... }
    for child in impl_node.children_with_tokens() {
        if child.kind() == SyntaxKind::PATH_TYPE {
            if let Some(path) = child.as_node() {
                // Extract the type path
                return Some(path.to_string().trim().to_string());
            }
        }
    }
    None
}

fn extract_trait_name(trait_node: &SyntaxNode) -> Option<String> {
    trait_node.children_with_tokens()
        .find(|c| c.kind() == SyntaxKind::NAME)
        .map(|n| n.to_string())
}

fn file_to_module_path(file: &Path, lib_root: &Path) -> String {
    // Convert file path to module path
    // e.g., std/collections/hash_map.rs -> std::collections::hash_map
    let rel = file.strip_prefix(lib_root).unwrap_or(file);
    let path_str = rel.to_string_lossy();
    
    // Remove .rs extension and convert / to ::
    let module = path_str
        .strip_suffix(".rs").unwrap_or(&path_str)
        .replace("/", "::")
        .replace("mod::", "")  // Remove mod:: for mod.rs files
        .replace("lib::", ""); // Remove lib:: for lib.rs files
    
    module
}

fn count_functions_in_file(file: &Path, lib_root: &Path) -> Result<(usize, usize, usize, usize, usize, Vec<FunctionInfo>)> {
    let content = fs::read_to_string(file)
        .with_context(|| format!("Failed to read: {}", file.display()))?;
    
    let parse = match parse_file(&content) {
        Ok(p) => p,
        Err(_) => return Ok((0, 0, 0, 0, 0, Vec::new())), // Skip files with parse errors
    };
    
    let root = parse.syntax();
    let mut pub_count = 0;
    let mut unsafe_count = 0;
    let mut total_fns = 0;
    let mut trait_fns = 0;
    let mut impl_fns = 0;
    let mut functions = Vec::new();
    
    let module_path = file_to_module_path(file, lib_root);
    
    // Walk AST looking for all function definitions
    for item_node in root.descendants() {
        if item_node.kind() == SyntaxKind::FN {
            if let Some(fn_node) = ast::Fn::cast(item_node.clone()) {
                total_fns += 1;
                
                // Check if unsafe
                if fn_node.unsafe_token().is_some() {
                    unsafe_count += 1;
                }
                
                // Check visibility: VISIBILITY token is a child of the FN node itself
                let is_pub = item_node.children_with_tokens().any(|child| {
                    child.kind() == SyntaxKind::VISIBILITY
                });
                
                // Extract function name
                let fn_name = extract_function_name(&item_node).unwrap_or_else(|| "<anonymous>".to_string());
                
                // Determine context: trait, impl, or standalone
                let mut current = item_node.parent();
                let mut context = FunctionContext::Standalone;
                
                while let Some(parent) = current {
                    match parent.kind() {
                        SyntaxKind::TRAIT => {
                            if let Some(trait_name) = extract_trait_name(&parent) {
                                context = FunctionContext::Trait(trait_name);
                            }
                            break;
                        }
                        SyntaxKind::IMPL => {
                            if let Some(type_name) = extract_impl_type(&parent) {
                                context = FunctionContext::Impl(type_name);
                            }
                            break;
                        }
                        _ => {}
                    }
                    current = parent.parent();
                }
                
                // Update counts based on context
                match &context {
                    FunctionContext::Trait(_) => {
                        trait_fns += 1;
                        pub_count += 1; // Trait methods are implicitly public
                    }
                    FunctionContext::Impl(_) => {
                        impl_fns += 1;
                        if is_pub {
                            pub_count += 1;
                        }
                    }
                    FunctionContext::Standalone => {
                        if is_pub {
                            pub_count += 1;
                        }
                    }
                }
                
                // Create FunctionInfo
                let is_trait = matches!(context, FunctionContext::Trait(_));
                functions.push(FunctionInfo {
                    name: fn_name,
                    module_path: module_path.clone(),
                    context,
                    is_public: is_pub || is_trait,
                    is_unsafe: fn_node.unsafe_token().is_some(),
                });
            }
        }
    }
    
    Ok((pub_count, unsafe_count, total_fns, trait_fns, impl_fns, functions))
}

fn count_stdlib_functions(jobs: usize) -> Result<()> {
    let overall_start = std::time::Instant::now();
    
    // Set up logging FIRST
    let log_path = PathBuf::from("analyses/analyze_modules.log");
    fs::create_dir_all("analyses")?;
    let mut log_file = fs::File::create(&log_path)
        .context("Failed to create log file")?;

    macro_rules! log {
        ($($arg:tt)*) => {
            writeln!(log_file, $($arg)*).ok();
        };
    }
    
    // Log header
    log!("rusticate-analyze-modules --rust-libs");
    log!("======================================");
    log!("Command: {}", std::env::args().collect::<Vec<_>>().join(" "));
    log!("Jobs: {}", jobs);
    log!("Started: {:?}\n", overall_start);
    
    println!("rusticate-analyze-modules --rust-libs");
    println!("======================================");
    println!("Finding Rust stdlib source...\n");
    
    let stdlib_path = find_rust_stdlib()?;
    log!("Stdlib path: {}", stdlib_path.display());
    println!("Found: {}\n", stdlib_path.display());
    
    // Analyze std, core, alloc, proc_macro, test
    let libs = vec![
        ("std", stdlib_path.join("std")),
        ("core", stdlib_path.join("core")),
        ("alloc", stdlib_path.join("alloc")),
        ("proc_macro", stdlib_path.join("proc_macro")),
        ("test", stdlib_path.join("test")),
    ];
    
    let mut total_pub = 0;
    let mut total_unsafe = 0;
    let mut total_files = 0;
    let mut total_fns = 0;
    let mut total_trait = 0;
    let mut total_impl = 0;
    let mut all_functions: Vec<FunctionInfo> = Vec::new();
    
    for (lib_name, lib_path) in libs {
        if !lib_path.exists() {
            println!("Warning: {} not found at {}", lib_name, lib_path.display());
            continue;
        }
        
        log!("\nAnalyzing {}...", lib_name);
        log!("  Path: {}", lib_path.display());
        println!("Analyzing {}...", lib_name);
        let files = find_rust_files(&lib_path);
        log!("  Files: {}", files.len());
        println!("  {} files", files.len());
        
        // Parse files in parallel using chunks
        let chunk_size = (files.len() + jobs - 1) / jobs;
        let chunks: Vec<_> = files.chunks(chunk_size).map(|c| c.to_vec()).collect();
        
        let lib_path_clone = lib_path.clone();
        let handles: Vec<_> = chunks
            .into_iter()
            .enumerate()
            .map(|(chunk_idx, chunk)| {
                let lib_root = lib_path_clone.clone();
                std::thread::spawn(move || {
                    let mut pub_count = 0;
                    let mut unsafe_count = 0;
                    let mut total_count = 0;
                    let mut trait_count = 0;
                    let mut impl_count = 0;
                    let mut all_functions = Vec::new();
                    
                    for file in chunk.iter() {
                        if let Ok((pub_fns, unsafe_fns, total_fns, trait_fns, impl_fns, functions)) = count_functions_in_file(file, &lib_root) {
                            pub_count += pub_fns;
                            unsafe_count += unsafe_fns;
                            total_count += total_fns;
                            trait_count += trait_fns;
                            impl_count += impl_fns;
                            all_functions.extend(functions);
                        }
                    }
                    
                    (pub_count, unsafe_count, total_count, trait_count, impl_count, all_functions)
                })
            })
            .collect();
        
        // Merge results
        let mut lib_pub = 0;
        let mut lib_unsafe = 0;
        let mut lib_total = 0;
        let mut lib_trait = 0;
        let mut lib_impl = 0;
        let mut lib_functions = Vec::new();
        for handle in handles {
            let (pub_fns, unsafe_fns, total_fns, trait_fns, impl_fns, functions) = handle.join().unwrap();
            lib_pub += pub_fns;
            lib_unsafe += unsafe_fns;
            lib_total += total_fns;
            lib_trait += trait_fns;
            lib_impl += impl_fns;
            lib_functions.extend(functions);
        }
        
        let standalone = lib_total - lib_trait - lib_impl;
        log!("  Total functions: {}", lib_total);
        log!("    Standalone: {}", standalone);
        log!("    In traits: {}", lib_trait);
        log!("    In impls: {}", lib_impl);
        log!("  Public: {}", lib_pub);
        log!("  Unsafe: {}", lib_unsafe);
        
        println!("  {} total functions ({} standalone, {} in traits, {} in impls)", 
                 lib_total, standalone, lib_trait, lib_impl);
        println!("    {} public, {} unsafe", lib_pub, lib_unsafe);
        total_pub += lib_pub;
        total_unsafe += lib_unsafe;
        total_fns += lib_total;
        total_trait += lib_trait;
        total_impl += lib_impl;
        total_files += files.len();
        all_functions.extend(lib_functions);
    }
    
    let elapsed = overall_start.elapsed();
    let standalone = total_fns - total_trait - total_impl;
    
    log!("\n=== SUMMARY ===");
    log!("Total files: {}", total_files);
    log!("Total functions: {}", total_fns);
    log!("  Standalone: {}", standalone);
    log!("  In traits: {}", total_trait);
    log!("  In impls: {}", total_impl);
    log!("Total public: {}", total_pub);
    log!("Total unsafe: {}", total_unsafe);
    log!("\nAnalysis completed in {} ms.", elapsed.as_millis());
    log!("Finished: {:?}", std::time::Instant::now());
    
    println!("\n=== Summary ===");
    println!("Total files: {}", total_files);
    println!("Total functions: {}", total_fns);
    println!("  Standalone: {}", standalone);
    println!("  In traits: {}", total_trait);
    println!("  In impls: {}", total_impl);
    println!("Total public: {}", total_pub);
    println!("Total unsafe: {}", total_unsafe);
    println!("\nAnalysis completed in {} ms.", elapsed.as_millis());
    
    // Output detailed function list organized by type
    let inventory_start = std::time::Instant::now();
    output_function_inventory(&all_functions, &mut log_file)?;
    let inventory_elapsed = inventory_start.elapsed();
    
    let total_elapsed = overall_start.elapsed();
    log!("\nInventory output: {} ms", inventory_elapsed.as_millis());
    log!("TOTAL TIME: {} ms ({:.2} seconds)", total_elapsed.as_millis(), total_elapsed.as_secs_f64());
    
    println!("\nInventory written to log");
    println!("TOTAL TIME: {} ms ({:.2} seconds)", total_elapsed.as_millis(), total_elapsed.as_secs_f64());
    
    Ok(())
}

fn output_function_inventory(functions: &[FunctionInfo], log_file: &mut fs::File) -> Result<()> {
    use std::collections::BTreeMap;
    
    // Organize by module::type
    let mut by_type: BTreeMap<String, Vec<&FunctionInfo>> = BTreeMap::new();
    
    for func in functions {
        let key = match &func.context {
            FunctionContext::Standalone => {
                format!("{}::<standalone>", func.module_path)
            }
            FunctionContext::Trait(trait_name) => {
                format!("{}::trait::{}", func.module_path, trait_name)
            }
            FunctionContext::Impl(type_name) => {
                format!("{}::{}", func.module_path, type_name)
            }
        };
        by_type.entry(key).or_insert_with(Vec::new).push(func);
    }
    
    writeln!(log_file, "\n\n=== DETAILED FUNCTION INVENTORY ===")?;
    writeln!(log_file, "Format: module::Type")?;
    writeln!(log_file, "  - function_name\n")?;
    
    for (type_key, funcs) in by_type.iter() {
        writeln!(log_file, "{}", type_key)?;
        for func in funcs {
            let flags = format!("{}{}",
                if func.is_public { "pub " } else { "" },
                if func.is_unsafe { "unsafe " } else { "" }
            );
            writeln!(log_file, "  - {}{}", flags, func.name)?;
        }
        writeln!(log_file)?;
    }
    
    writeln!(log_file, "=== END INVENTORY ===")?;
    
    Ok(())
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

fn find_rust_stdlib() -> Result<PathBuf> {
    // Try rustup sysroot first
    let output = std::process::Command::new("rustc")
        .arg("--print")
        .arg("sysroot")
        .output();
    
    if let Ok(output) = output {
        if output.status.success() {
            let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stdlib_path = PathBuf::from(sysroot)
                .join("lib/rustlib/src/rust/library");
            
            if stdlib_path.exists() {
                return Ok(stdlib_path);
            }
        }
    }
    
    // Try ~/projects/rust
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home".to_string());
    let projects_rust = PathBuf::from(home).join("projects/rust/library");
    
    if projects_rust.exists() {
        return Ok(projects_rust);
    }
    
    bail!(
        "Rust stdlib source not found!\n\
        Install with: rustup component add rust-src\n\
        Or clone to: ~/projects/rust"
    );
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

    // Handle -R mode separately
    if args.rust_libs {
        return count_stdlib_functions(args.jobs);
    }

    // Must have codebase for normal mode
    let codebase = args.codebase.context("Codebase required for non -R mode")?;

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
        codebase.display(), 
        args.max_codebases.map_or("unlimited".to_string(), |m| m.to_string()),
        args.jobs,
        start);

    println!("rusticate-analyze-modules");
    println!("==========================");
    println!("Codebase: {}", codebase.display());
    if let Some(max) = args.max_codebases {
        println!("Max codebases: {}", max);
    }
    println!("Jobs: {}", args.jobs);
    println!();

    // Check if this is a directory of codebases or a single codebase
    let codebases = find_codebases(&codebase);
    
    let codebases_to_analyze = if codebases.is_empty() {
        // Single codebase - analyze the given path directly
        vec![codebase.clone()]
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

