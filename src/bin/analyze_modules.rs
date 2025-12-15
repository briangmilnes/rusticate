use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use ra_ap_syntax::{ast, AstNode, SyntaxKind, SyntaxNode};
use rusticate::parse_file;

// Crates that wrap/re-export everything - we want to filter these out
#[allow(dead_code)]
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
    usage_analysis: bool,  // Compile projects and count stdlib usage from MIR
    mir_analysis: Option<PathBuf>,  // Analyze MIR from a single crate
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args_iter = std::env::args().skip(1);
        let mut codebase = None;
        let mut max_codebases = None;
        let mut jobs = 4;
        let mut rust_libs = false;
        let mut usage_analysis = false;
        let mut mir_analysis = None;

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
                "-U" | "--usage" => {
                    usage_analysis = true;
                }
                "-M" | "--mir" => {
                    mir_analysis = Some(PathBuf::from(
                        args_iter
                            .next()
                            .context("Expected path after -M/--mir")?
                    ));
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

        // Fail fast: need either -C, -R, or -M
        if codebase.is_none() && !rust_libs && mir_analysis.is_none() {
            bail!("Must specify -C/--codebase, -R/--rust-libs, or -M/--mir\nRun with --help for usage");
        }
        
        // -U requires -C
        if usage_analysis && codebase.is_none() {
            bail!("-U/--usage requires -C/--codebase\nRun with --help for usage");
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
            usage_analysis,
            mir_analysis,
        })
    }
}

fn print_help() {
    println!(
        r#"rusticate-analyze-modules - Analyze module usage in codebases

USAGE:
    rusticate-analyze-modules -C <PATH> [-m <N>] [-j <N>]
    rusticate-analyze-modules -M <PATH>
    rusticate-analyze-modules -R [-j <N>]

OPTIONS:
    -C, --codebase <PATH>       Path to a codebase, or a directory of codebases
    -M, --mir <PATH>            Analyze MIR files from a crate (reads existing .mir files)
    -R, --rust-libs             Parse Rust stdlib (std/core/alloc) and count all functions
    -m, --max-codebases <N>     Limit number of codebases to analyze (default: unlimited)
    -j, --jobs <N>              Number of parallel threads (default: 4)
    -h, --help                  Print this help message

MODES:
    -C: Analyze what modules/types are imported (fast, parse-only)
    -M: Analyze stdlib usage from pre-generated MIR files (fast, reads disk)
    -R: Generate stdlib function inventory from Rust source

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
    is_test: bool,
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

fn has_test_attribute(fn_node: &SyntaxNode) -> bool {
    // Check if function has #[test] attribute
    for child in fn_node.children_with_tokens() {
        if child.kind() == SyntaxKind::ATTR {
            let attr_text = child.to_string();
            if attr_text.contains("#[test]") || attr_text.contains("#[bench]") {
                return true;
            }
        }
    }
    false
}

fn is_in_test_module(node: &SyntaxNode) -> bool {
    // Check if function is inside a #[cfg(test)] module
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::MODULE {
            // Check if this module has #[cfg(test)] attribute
            for child in parent.children_with_tokens() {
                if child.kind() == SyntaxKind::ATTR {
                    let attr_text = child.to_string();
                    if attr_text.contains("cfg(test)") || attr_text.contains("cfg(test") {
                        return true;
                    }
                }
            }
        }
        current = parent.parent();
    }
    false
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

fn count_functions_in_file(file: &Path, lib_root: &Path) -> Result<(usize, usize, usize, usize, usize, usize, Vec<FunctionInfo>)> {
    let content = fs::read_to_string(file)
        .with_context(|| format!("Failed to read: {}", file.display()))?;
    
    let parse = match parse_file(&content) {
        Ok(p) => p,
        Err(_) => return Ok((0, 0, 0, 0, 0, 0, Vec::new())), // Skip files with parse errors
    };
    
    let root = parse.syntax();
    let mut pub_count = 0;
    let mut unsafe_count = 0;
    let mut total_fns = 0;
    let mut trait_fns = 0;
    let mut impl_fns = 0;
    let mut test_fns = 0;
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
                
                // Check if this is a test function
                let is_test = has_test_attribute(&item_node) || is_in_test_module(&item_node);
                if is_test {
                    test_fns += 1;
                }
                
                // Update counts based on context (but not for test functions)
                if !is_test {
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
                }
                
                // Create FunctionInfo
                let is_trait = matches!(context, FunctionContext::Trait(_));
                functions.push(FunctionInfo {
                    name: fn_name,
                    module_path: module_path.clone(),
                    context,
                    is_public: is_pub || is_trait,
                    is_unsafe: fn_node.unsafe_token().is_some(),
                    is_test,
                });
            }
        }
    }
    
    Ok((pub_count, unsafe_count, total_fns, trait_fns, impl_fns, test_fns, functions))
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
    
    use chrono::Local;
    let datetime = Local::now().format("%Y-%m-%d %H:%M:%S %Z");
    log!("Started at: {}\n", datetime);
    
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
    let mut total_test = 0;
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
            .map(|(_chunk_idx, chunk)| {
                let lib_root = lib_path_clone.clone();
                std::thread::spawn(move || {
                    let mut pub_count = 0;
                    let mut unsafe_count = 0;
                    let mut total_count = 0;
                    let mut trait_count = 0;
                    let mut impl_count = 0;
                    let mut test_count = 0;
                    let mut all_functions = Vec::new();
                    
                    for file in chunk.iter() {
                        if let Ok((pub_fns, unsafe_fns, total_fns, trait_fns, impl_fns, test_fns, functions)) = count_functions_in_file(file, &lib_root) {
                            pub_count += pub_fns;
                            unsafe_count += unsafe_fns;
                            total_count += total_fns;
                            trait_count += trait_fns;
                            impl_count += impl_fns;
                            test_count += test_fns;
                            all_functions.extend(functions);
                        }
                    }
                    
                    (pub_count, unsafe_count, total_count, trait_count, impl_count, test_count, all_functions)
                })
            })
            .collect();
        
        // Merge results
        let mut lib_pub = 0;
        let mut lib_unsafe = 0;
        let mut lib_total = 0;
        let mut lib_trait = 0;
        let mut lib_impl = 0;
        let mut lib_test = 0;
        let mut lib_functions = Vec::new();
        for handle in handles {
            let (pub_fns, unsafe_fns, total_fns, trait_fns, impl_fns, test_fns, functions) = handle.join().unwrap();
            lib_pub += pub_fns;
            lib_unsafe += unsafe_fns;
            lib_total += total_fns;
            lib_trait += trait_fns;
            lib_impl += impl_fns;
            lib_test += test_fns;
            lib_functions.extend(functions);
        }
        
        let standalone = lib_total - lib_trait - lib_impl - lib_test;
        log!("  Total functions: {}", lib_total);
        log!("    Standalone: {}", standalone);
        log!("    In traits: {}", lib_trait);
        log!("    In impls: {}", lib_impl);
        log!("    Tests: {} (excluded from usage)", lib_test);
        log!("  Public: {}", lib_pub);
        log!("  Unsafe: {}", lib_unsafe);
        
        println!("  {} total functions ({} standalone, {} in traits, {} in impls)", 
                 lib_total, standalone, lib_trait, lib_impl);
        println!("  {} test functions (excluded from usage analysis)", lib_test);
        println!("    {} public, {} unsafe", lib_pub, lib_unsafe);
        total_pub += lib_pub;
        total_unsafe += lib_unsafe;
        total_fns += lib_total;
        total_trait += lib_trait;
        total_impl += lib_impl;
        total_test += lib_test;
        total_files += files.len();
        all_functions.extend(lib_functions);
    }
    
    let elapsed = overall_start.elapsed();
    
    // Calculate library functions (excluding tests)
    let lib_fns = total_fns - total_test;
    let standalone = lib_fns - total_trait - total_impl;
    
    log!("\nParsing completed in {} ms.", elapsed.as_millis());
    
    // Output detailed function list organized by type
    let inventory_start = std::time::Instant::now();
    output_function_inventory(&all_functions, &mut log_file)?;
    let inventory_elapsed = inventory_start.elapsed();
    
    let total_elapsed = overall_start.elapsed();
    log!("\nInventory output: {} ms", inventory_elapsed.as_millis());
    
    // Final summary - to both log and stdout
    let end_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z");
    
    log!("\n=== FINAL SUMMARY ===");
    log!("Total files analyzed: {}", total_files);
    log!("Library functions: {}", lib_fns);
    log!("  Standalone: {}", standalone);
    log!("  In traits: {}", total_trait);
    log!("  In impls: {}", total_impl);
    log!("Test functions: {} (excluded from usage)", total_test);
    log!("Total public: {}", total_pub);
    log!("Total unsafe: {}", total_unsafe);
    log!("\nTOTAL TIME: {} ms ({:.2} seconds)", total_elapsed.as_millis(), total_elapsed.as_secs_f64());
    log!("Ended at: {}", end_time);
    
    println!("\nInventory written to log");
    
    println!("\n=== FINAL SUMMARY ===");
    println!("Total files analyzed: {}", total_files);
    println!("Library functions: {}", lib_fns);
    println!("  Standalone: {}", standalone);
    println!("  In traits: {}", total_trait);
    println!("  In impls: {}", total_impl);
    println!("Test functions: {} (excluded from usage)", total_test);
    println!("Total public: {}", total_pub);
    println!("Total unsafe: {}", total_unsafe);
    println!("\nTOTAL TIME: {} ms ({:.2} seconds)", total_elapsed.as_millis(), total_elapsed.as_secs_f64());
    println!("Ended at: {}", end_time);
    println!("Log written to: {}", log_path.display());
    
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

fn check_mir_exists(project_path: &Path) -> bool {
    let target_dir = project_path.join("target/debug/deps");
    if !target_dir.exists() {
        return false;
    }
    
    // Check if any .mir files exist
    if let Ok(entries) = fs::read_dir(&target_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("mir") {
                return true;
            }
        }
    }
    false
}

fn compile_with_mir(project_path: &Path) -> Result<()> {
    println!("  Checking and emitting MIR (no codegen)...");
    
    let output = std::process::Command::new("cargo")
        .arg("check")  // Use check instead of build - faster, no codegen
        .current_dir(project_path)
        .env("RUSTFLAGS", "--emit=mir")
        .output()
        .context("Failed to run cargo check")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Cargo check failed:\n{}", stderr);
    }
    
    Ok(())
}

fn normalize_function_name(raw: &str) -> String {
    // Handle: <Type<T> as Trait>::method -> Type::method
    // Handle: Type::<T, U>::method -> Type::method
    // Handle: std::vec::Vec::<T>::push -> std::vec::Vec::push
    
    let mut result = raw.to_string();
    
    // Remove trait syntax: <Type as Trait>:: -> Type::
    if result.starts_with('<') {
        if let Some(as_pos) = result.find(" as ") {
            // Extract type name between < and " as "
            let type_part = &result[1..as_pos];
            // Find the closing > for the outer <...>
            if let Some(close_pos) = result.find(">::") {
                let rest = &result[close_pos + 3..];
                result = format!("{}::{}", type_part, rest);
            }
        } else if let Some(close_pos) = result.find(">::") {
            // Just <Type>:: without "as", remove the angle brackets
            let type_part = &result[1..close_pos];
            let rest = &result[close_pos + 3..];
            result = format!("{}::{}", type_part, rest);
        }
    }
    
    // Remove generic parameters: Type::<T, U> -> Type
    // This is tricky because we need to balance angle brackets
    let mut cleaned = String::new();
    let mut depth = 0;
    let mut skip_mode = false;
    
    for ch in result.chars() {
        match ch {
            ':' if depth == 0 => {
                cleaned.push(ch);
                skip_mode = false;
            }
            '<' if skip_mode => {
                depth += 1;
            }
            '>' if skip_mode => {
                depth -= 1;
                if depth == 0 {
                    skip_mode = false;
                }
            }
            _ if skip_mode => {
                // Skip generics content
            }
            '<' if !skip_mode => {
                // Start of generics, skip it
                skip_mode = true;
                depth = 1;
            }
            _ => {
                cleaned.push(ch);
            }
        }
    }
    
    // Final cleanup
    cleaned = cleaned.replace("<", "").replace(">", "");
    
    // Remove multiple consecutive :: 
    while cleaned.contains("::::") {
        cleaned = cleaned.replace("::::", "::");
    }
    while cleaned.contains(":::") {
        cleaned = cleaned.replace(":::", "::");
    }
    
    // Remove trailing ::
    if cleaned.ends_with("::") {
        cleaned = cleaned[..cleaned.len() - 2].to_string();
    }
    
    // Trim
    cleaned.trim().to_string()
}

fn parse_mir_file(mir_path: &Path) -> Result<Vec<String>> {
    let content = fs::read_to_string(mir_path)
        .with_context(|| format!("Failed to read MIR file: {}", mir_path.display()))?;
    
    let mut calls = Vec::new();
    
    // Parse lines looking for function calls
    // Format: _N = Type::<Generics>::method(...) -> [...]
    for line in content.lines() {
        let line = line.trim();
        
        // Look for stdlib function calls
        if line.contains("std::") || line.contains("core::") || line.contains("alloc::") {
            // Extract function call: Type::<...>::method
            if let Some(eq_pos) = line.find('=') {
                let after_eq = line[eq_pos + 1..].trim();
                
                // Find the function call by balancing angle brackets
                let mut depth = 0;
                let mut end_pos = None;
                
                for (i, ch) in after_eq.char_indices() {
                    match ch {
                        '<' => depth += 1,
                        '>' => depth -= 1,
                        '(' if depth == 0 => {
                            end_pos = Some(i);
                            break;
                        }
                        ' ' if depth == 0 && after_eq[i..].starts_with(" ->") => {
                            end_pos = Some(i);
                            break;
                        }
                        _ => {}
                    }
                }
                
                if let Some(end) = end_pos {
                    let call = after_eq[..end].trim();
                    
                    // Filter for stdlib calls
                    if call.contains("std::") || call.contains("core::") || call.contains("alloc::") {
                        let normalized = normalize_function_name(call);
                        if !normalized.is_empty() {
                            calls.push(normalized);
                        }
                    }
                }
            }
        }
    }
    
    Ok(calls)
}

fn analyze_usage(codebase: &Path, max_codebases: Option<usize>, _jobs: usize) -> Result<()> {
    let overall_start = std::time::Instant::now();
    
    println!("rusticate-analyze-modules --usage");
    println!("==================================");
    println!("Codebase: {}", codebase.display());
    println!();
    
    // Check if this is a single project or directory of projects
    let mut codebases = if codebase.join("Cargo.toml").exists() {
        // Single project
        vec![codebase.to_path_buf()]
    } else {
        // Directory of projects
        find_codebases(codebase)
    };
    
    if codebases.is_empty() {
        bail!("No Rust projects found in {}", codebase.display());
    }
    
    // Apply max limit
    if let Some(max) = max_codebases {
        println!("Limiting to {} codebases", max);
        codebases.truncate(max);
    }
    
    println!("Processing {} projects (-j 1)...\n", codebases.len());
    
    let mut total_calls = 0;
    let mut projects_with_mir = 0;
    let mut projects_compiled = 0;
    let mut all_stdlib_calls: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    
    for (idx, project) in codebases.iter().enumerate() {
        println!("[{}/{}] {}", idx + 1, codebases.len(), project.file_name().unwrap().to_string_lossy());
        
        // Check if MIR exists
        let has_mir = check_mir_exists(project);
        
        if has_mir {
            println!("  MIR files found, reusing...");
            projects_with_mir += 1;
        } else {
            // Compile with MIR
            match compile_with_mir(project) {
                Ok(_) => {
                    println!("  Compilation successful");
                    projects_compiled += 1;
                }
                Err(e) => {
                    println!("  Compilation failed: {}", e);
                    continue;
                }
            }
        }
        
        // Parse MIR files
        let target_dir = project.join("target/debug/deps");
        if let Ok(entries) = fs::read_dir(&target_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|s| s.to_str()) == Some("mir") {
                    if let Ok(calls) = parse_mir_file(&entry.path()) {
                        for call in calls {
                            *all_stdlib_calls.entry(call).or_insert(0) += 1;
                            total_calls += 1;
                        }
                    }
                }
            }
        }
        
        println!();
    }
    
    let elapsed = overall_start.elapsed();
    
    println!("\n=== Summary ===");
    println!("Projects processed: {}", codebases.len());
    println!("  MIR reused: {}", projects_with_mir);
    println!("  Compiled: {}", projects_compiled);
    println!("Total stdlib calls found: {}", total_calls);
    println!("Unique stdlib functions: {}", all_stdlib_calls.len());
    println!("\nTop 20 most called:");
    
    let mut calls_vec: Vec<_> = all_stdlib_calls.iter().collect();
    calls_vec.sort_by(|a, b| b.1.cmp(a.1));
    
    for (call, count) in calls_vec.iter().take(20) {
        println!("  {:6} {}", count, call);
    }
    
    println!("\nTOTAL TIME: {} ms ({:.2} seconds)", elapsed.as_millis(), elapsed.as_secs_f64());
    
    Ok(())
}

/// Analyze MIR output from a crate or directory of crates to extract stdlib usage
fn analyze_mir_crate(path: &Path) -> Result<()> {
    use std::collections::BTreeMap;
    use regex::Regex;
    use std::io::Write;
    
    let start = std::time::Instant::now();
    let start_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z");
    
    // Set up logging
    let log_path = PathBuf::from("analyses/rusticate-analyze-modules-mir.log");
    fs::create_dir_all("analyses")?;
    let mut log_file = fs::File::create(&log_path)?;
    
    macro_rules! log {
        ($($arg:tt)*) => {
            writeln!(log_file, $($arg)*).ok();
        };
    }
    
    log!("rusticate-analyze-modules --mir");
    log!("================================");
    log!("Command: {}", std::env::args().collect::<Vec<_>>().join(" "));
    log!("Path: {}", path.display());
    log!("Started: {}", start_time);
    log!("");
    
    println!("rusticate-analyze-modules --mir");
    println!("================================");
    println!("Path: {}", path.display());
    
    // Check if this is a directory of projects or a single project
    let is_multi_project = path.is_dir() && !path.join("Cargo.toml").exists() && !path.join("target/debug/deps").exists();
    
    if is_multi_project {
        // Multi-project mode - aggregate stats across all projects
        return analyze_mir_multi_project(path, &mut log_file);
    }
    
    // Single project mode
    // Find MIR files
    let mir_files: Vec<PathBuf> = if path.is_file() && path.extension().map(|e| e == "mir").unwrap_or(false) {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        // Look for target/debug/deps/*.mir
        let deps_path = path.join("target/debug/deps");
        if deps_path.exists() {
            WalkDir::new(&deps_path)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|ext| ext == "mir").unwrap_or(false))
                .map(|e| e.path().to_path_buf())
                .collect()
        } else {
            bail!("No target/debug/deps found in {}", path.display());
        }
    } else {
        bail!("Path must be a .mir file or crate directory: {}", path.display());
    };
    
    println!("Found {} MIR files\n", mir_files.len());
    log!("Found {} MIR files", mir_files.len());
    for f in &mir_files {
        log!("  {}", f.display());
    }
    log!("");
    
    // Patterns for extracting stdlib calls - more comprehensive to catch all usage
    // Match any std/core/alloc path with 2+ segments (catches module refs AND method calls)
    let stdlib_path_re = Regex::new(r"(?:std|core|alloc)::[a-zA-Z_][a-zA-Z0-9_:]*").unwrap();
    // Match: <Type as std/core/alloc::Trait>::method patterns  
    let trait_impl_re = Regex::new(r"<[^>]+ as (?:std|core|alloc)::[^>]+>::[a-z_][a-z0-9_]*").unwrap();
    // Match: common stdlib types with methods (expanded list)
    // Match stdlib types with their full qualified path (std::/core::/alloc::)
    let type_method_re = Regex::new(r"(?:std|core|alloc)::[a-z_]+::(Option|Result|Vec|String|Box|Rc|Arc|Cell|RefCell|Mutex|RwLock|HashMap|BTreeMap|HashSet|BTreeSet|VecDeque|LinkedList|BinaryHeap|Formatter|Arguments|Error|Path|PathBuf|OsStr|OsString|File|OpenOptions|BufReader|BufWriter|Stdin|Stdout|Stderr|TcpStream|TcpListener|UdpSocket|Command|Child|Duration|Instant|SystemTime|Thread|JoinHandle|Sender|Receiver|Condvar|Barrier|Once|Cow|Pin|NonNull|MaybeUninit|ManuallyDrop|PhantomData|Ordering|Range|RangeInclusive|Chars|Bytes|Lines|Split|Iter|IterMut|IntoIter|Drain|Entry|Occupied|Vacant)::[a-z_][a-z0-9_]*").unwrap();
    
    let mut stdlib_calls: BTreeMap<String, usize> = BTreeMap::new();
    let mut data_types: BTreeMap<String, usize> = BTreeMap::new();
    let mut modules_used: BTreeMap<String, usize> = BTreeMap::new();
    
    for mir_file in &mir_files {
        let content = match fs::read_to_string(mir_file) {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        // Extract stdlib calls
        for cap in stdlib_path_re.find_iter(&content) {
            let call = cap.as_str().to_string();
            *stdlib_calls.entry(call.clone()).or_insert(0) += 1;
            
            // Extract module (first two parts)
            let parts: Vec<&str> = call.split("::").collect();
            if parts.len() >= 2 {
                let module = format!("{}::{}", parts[0], parts[1]);
                *modules_used.entry(module).or_insert(0) += 1;
            }
        }
        
        // Extract trait impl calls
        for cap in trait_impl_re.find_iter(&content) {
            let call = cap.as_str().to_string();
            *stdlib_calls.entry(call).or_insert(0) += 1;
        }
        
        // Extract type method calls - use qualified type paths
        for cap in type_method_re.find_iter(&content) {
            let call = cap.as_str().to_string();
            *stdlib_calls.entry(call.clone()).or_insert(0) += 1;
            
            // Extract qualified type name (std::module::Type)
            let parts: Vec<&str> = call.split("::").collect();
            if parts.len() >= 3 {
                let qualified_type = format!("{}::{}::{}", parts[0], parts[1], parts[2]);
                *data_types.entry(qualified_type).or_insert(0) += 1;
            }
        }
    }
    
    // Output results
    println!("=== Stdlib Modules Used ===");
    log!("=== Stdlib Modules Used ===");
    let mut mod_vec: Vec<_> = modules_used.iter().collect();
    mod_vec.sort_by(|a, b| b.1.cmp(a.1));
    for (module, count) in mod_vec.iter().take(30) {
        println!("  {:6} {}", count, module);
        log!("  {:6} {}", count, module);
    }
    
    println!("\n=== Data Types Used ===");
    log!("\n=== Data Types Used ===");
    let mut type_vec: Vec<_> = data_types.iter().collect();
    type_vec.sort_by(|a, b| b.1.cmp(a.1));
    for (dtype, count) in type_vec {
        println!("  {:6} {}", count, dtype);
        log!("  {:6} {}", count, dtype);
    }
    
    println!("\n=== Top 50 Stdlib Function Calls ===");
    log!("\n=== Top 50 Stdlib Function Calls ===");
    let mut call_vec: Vec<_> = stdlib_calls.iter().collect();
    call_vec.sort_by(|a, b| b.1.cmp(a.1));
    for (call, count) in call_vec.iter().take(50) {
        println!("  {:6} {}", count, call);
        log!("  {:6} {}", count, call);
    }
    
    // Also log ALL calls to the log file
    log!("\n=== ALL Stdlib Function Calls ({} total) ===", stdlib_calls.len());
    for (call, count) in &call_vec {
        log!("  {:6} {}", count, call);
    }
    
    let elapsed = start.elapsed();
    let end_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z");
    
    println!("\n=== SUMMARY ===");
    log!("\n=== SUMMARY ===");
    println!("MIR files analyzed: {}", mir_files.len());
    log!("MIR files analyzed: {}", mir_files.len());
    println!("Unique stdlib modules: {}", modules_used.len());
    log!("Unique stdlib modules: {}", modules_used.len());
    println!("Unique data types: {}", data_types.len());
    log!("Unique data types: {}", data_types.len());
    println!("Unique stdlib calls: {}", stdlib_calls.len());
    log!("Unique stdlib calls: {}", stdlib_calls.len());
    println!("Total call instances: {}", stdlib_calls.values().sum::<usize>());
    log!("Total call instances: {}", stdlib_calls.values().sum::<usize>());
    println!("\nTOTAL TIME: {} ms ({:.2} seconds)", elapsed.as_millis(), elapsed.as_secs_f64());
    log!("\nTOTAL TIME: {} ms ({:.2} seconds)", elapsed.as_millis(), elapsed.as_secs_f64());
    println!("Ended at: {}", end_time);
    log!("Ended at: {}", end_time);
    
    println!("\nLog written to: {}", log_path.display());
    
    Ok(())
}

/// Strip the hash suffix from a crate name
/// e.g., "serde-c2b867703b654298" -> "serde"
///       "async-trait-abc12345" -> "async-trait"
fn strip_crate_hash(name: &str) -> String {
    // The hash is the last segment after '-' and is a hex string (usually 16 chars)
    if let Some(last_dash) = name.rfind('-') {
        let suffix = &name[last_dash + 1..];
        // Check if suffix looks like a hash (all hex chars, typically 8-16 chars)
        if suffix.len() >= 8 && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
            return name[..last_dash].to_string();
        }
    }
    name.to_string()
}

/// Strip generic parameters from a method name
/// e.g., "Vec::<u8>::push" -> "Vec::push"
///       "Option::<String>::unwrap" -> "Option::unwrap"  
///       "HashMap::<K, V>::insert" -> "HashMap::insert"
fn strip_generics(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut depth = 0;
    let mut chars = name.chars().peekable();
    
    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                depth += 1;
                // Skip the '<' and everything until balanced '>'
            }
            '>' => {
                depth -= 1;
            }
            ':' if depth == 0 => {
                // Check for "::<" pattern (turbofish) - skip the extra ':'
                if chars.peek() == Some(&':') {
                    chars.next(); // consume second ':'
                    if chars.peek() == Some(&'<') {
                        // It's "::<", don't add anything, the '<' will be handled next iteration
                    } else {
                        // It's just "::" without generics
                        result.push_str("::");
                    }
                } else {
                    result.push(':');
                }
            }
            _ if depth == 0 => {
                result.push(ch);
            }
            _ => {
                // Inside generics, skip
            }
        }
    }
    
    // Clean up any trailing ::
    while result.ends_with("::") {
        result.truncate(result.len() - 2);
    }
    
    result
}

/// Analyze MIR across multiple projects and aggregate statistics
fn analyze_mir_multi_project(path: &Path, log_file: &mut fs::File) -> Result<()> {
    use std::collections::{BTreeMap, HashSet};
    use regex::Regex;
    use std::io::Write;
    
    let start = std::time::Instant::now();
    
    macro_rules! log {
        ($($arg:tt)*) => {
            writeln!(log_file, $($arg)*).ok();
        };
    }
    
    println!("Multi-project MIR analysis mode");
    log!("Multi-project MIR analysis mode");
    
    // Find all projects with MIR files
    let mut projects_with_mir = Vec::new();
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let project_path = entry.path();
            if project_path.is_dir() {
                let deps_path = project_path.join("target/debug/deps");
                if deps_path.exists() {
                    // Check for .mir files
                    if let Ok(mir_entries) = fs::read_dir(&deps_path) {
                        let has_mir = mir_entries
                            .filter_map(|e| e.ok())
                            .any(|e| e.path().extension().map(|ext| ext == "mir").unwrap_or(false));
                        if has_mir {
                            projects_with_mir.push(project_path);
                        }
                    }
                }
            }
        }
    }
    
    projects_with_mir.sort();
    println!("Found {} projects with MIR files\n", projects_with_mir.len());
    log!("Found {} projects with MIR files\n", projects_with_mir.len());
    
    // Patterns for extracting stdlib calls - more comprehensive to catch all usage
    // Match any std/core/alloc path with 2+ segments (catches module refs AND method calls)
    let stdlib_path_re = Regex::new(r"(?:std|core|alloc)::[a-zA-Z_][a-zA-Z0-9_:]*").unwrap();
    // Match: <Type as std/core/alloc::Trait>::method patterns  
    let trait_impl_re = Regex::new(r"<[^>]+ as (?:std|core|alloc)::[^>]+>::[a-z_][a-z0-9_]*").unwrap();
    // Match: <StdlibType as AnyTrait>::method patterns (e.g., <Vec<T> as IntoIterator>::into_iter)
    // Note: trait may have generics like IndexMut<usize>, so allow <...> in trait name
    let stdlib_type_trait_impl_re = Regex::new(r"<(Vec|Option|Result|String|Box|Rc|Arc|Cell|RefCell|Mutex|RwLock|HashMap|BTreeMap|HashSet|BTreeSet|VecDeque|LinkedList|BinaryHeap)<[^>]*> as [A-Za-z_][A-Za-z0-9_:<>]*>::([a-z_][a-z0-9_]*)").unwrap();
    // Match stdlib types with their full qualified path (std::/core::/alloc::module::Type::method)
    let type_method_qualified_re = Regex::new(r"(?:std|core|alloc)::[a-z_]+::(Option|Result|Vec|String|Box|Rc|Arc|Cell|RefCell|Mutex|RwLock|HashMap|BTreeMap|HashSet|BTreeSet|VecDeque|LinkedList|BinaryHeap|Formatter|Arguments|Error|Path|PathBuf|OsStr|OsString|File|OpenOptions|BufReader|BufWriter|Stdin|Stdout|Stderr|TcpStream|TcpListener|UdpSocket|Command|Child|Duration|Instant|SystemTime|Thread|JoinHandle|Sender|Receiver|Condvar|Barrier|Once|Cow|Pin|NonNull|MaybeUninit|ManuallyDrop|PhantomData|Ordering|Range|RangeInclusive|Chars|Bytes|Lines|Split|Iter|IterMut|IntoIter|Drain|Entry|Occupied|Vacant)::[a-z_][a-z0-9_]*").unwrap();
    // Match unqualified stdlib type method calls (Option::<T>::method, Result::<T,E>::unwrap, etc.)
    // In MIR, these common types appear without module prefix in method calls
    let type_method_unqualified_re = Regex::new(r"\b(Option|Result|Vec|String|Box|Rc|Arc|Cell|RefCell|Mutex|RwLock|HashMap|BTreeMap|HashSet|BTreeSet|VecDeque|LinkedList|BinaryHeap)::<[^>]*>::([a-z_][a-z0-9_]*)").unwrap();
    
    // Match type annotations in MIR: `std::option::Option<...>` (captures type usage without method calls)
    let type_annotation_re = Regex::new(r"(?:std|core|alloc)::[a-z_]+::(Option|Result|Vec|String|Box|Rc|Arc|Cell|RefCell|Mutex|RwLock|HashMap|BTreeMap|HashSet|BTreeSet|VecDeque|LinkedList|BinaryHeap|Error|Path|PathBuf|OsStr|OsString|File|Duration|Instant|SystemTime|Command|Child|Ordering|Cow|Pin)<").unwrap();
    
    // Match unqualified type constructors: Option::<T>::Some, Result::<T,E>::Ok, etc.
    // Match constructors: Option::<T>::Some, Result::<T,E>::Ok, core::option::Option::<T>::Some, etc.
    let type_constructor_re = Regex::new(r"(?:core::(?:option|result)::)?(Option|Result)::<[^>]*>::(Some|None|Ok|Err)").unwrap();
    
    // Match trait impl patterns: <Type as Trait>::method - captures type, trait, and method
    // This catches Iterator, Clone, Debug, IntoIterator, From, Into, etc.
    let trait_impl_pattern_re = Regex::new(r"<([A-Za-z_][A-Za-z0-9_:<>& ,]*?) as (?:std|core|alloc)::([a-z_]+)::([A-Za-z_][A-Za-z0-9_]*)(?:<[^>]*>)?>::([a-z_][a-z0-9_]*)").unwrap();
    
    // Map unqualified type names to their qualified stdlib paths
    let get_qualified_type = |type_name: &str| -> Option<&'static str> {
        match type_name {
            "Option" => Some("core::option::Option"),
            "Result" => Some("core::result::Result"),
            "Vec" => Some("alloc::vec::Vec"),
            "String" => Some("alloc::string::String"),
            "Box" => Some("alloc::boxed::Box"),
            "Rc" => Some("alloc::rc::Rc"),
            "Arc" => Some("alloc::sync::Arc"),
            "Cell" => Some("core::cell::Cell"),
            "RefCell" => Some("core::cell::RefCell"),
            "Mutex" => Some("std::sync::Mutex"),
            "RwLock" => Some("std::sync::RwLock"),
            "HashMap" => Some("std::collections::HashMap"),
            "BTreeMap" => Some("alloc::collections::BTreeMap"),
            "HashSet" => Some("std::collections::HashSet"),
            "BTreeSet" => Some("alloc::collections::BTreeSet"),
            "VecDeque" => Some("alloc::collections::VecDeque"),
            "LinkedList" => Some("alloc::collections::LinkedList"),
            "BinaryHeap" => Some("alloc::collections::BinaryHeap"),
            _ => None,
        }
    };
    
    // Use rayon for parallel processing
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    let progress = AtomicUsize::new(0);
    let total_projects = projects_with_mir.len();
    
    println!("Analyzing {} projects in parallel...", total_projects);
    log!("Analyzing {} projects in parallel...", total_projects);
    
    // Process projects in parallel, each returns its own data
    let results: Vec<_> = projects_with_mir.par_iter().map(|project_path| {
        let project_name = project_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        
        let idx = progress.fetch_add(1, Ordering::Relaxed);
        if idx % 50 == 0 {
            eprint!("\r[{}/{}] Processing...", idx, total_projects);
        }
        
        let deps_path = project_path.join("target/debug/deps");
        let mir_files: Vec<PathBuf> = WalkDir::new(&deps_path)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "mir").unwrap_or(false))
            .map(|e| e.path().to_path_buf())
            .collect();
        
        let mir_count = mir_files.len();
        
        // Per-project results
        let mut local_modules: BTreeMap<String, HashSet<String>> = BTreeMap::new();
        let mut local_types: BTreeMap<String, HashSet<String>> = BTreeMap::new();
        let mut local_methods: BTreeMap<String, HashSet<String>> = BTreeMap::new();
        let mut local_crates: HashSet<String> = HashSet::new();
        // Traits: trait_name -> crates (for greedy cover of traits themselves)
        let mut local_traits: BTreeMap<String, HashSet<String>> = BTreeMap::new();
        // Trait impls: trait_name -> (impl_type, method) -> crates
        let mut local_trait_impls: BTreeMap<String, BTreeMap<String, HashSet<String>>> = BTreeMap::new();
        
        for mir_file in &mir_files {
            let raw_name = mir_file.file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let crate_name = strip_crate_hash(&raw_name);
            local_crates.insert(crate_name.clone());
            
            let content = match fs::read_to_string(mir_file) {
                Ok(c) => c,
                Err(_) => continue,
            };
            
            // Extract stdlib modules and methods
            for cap in stdlib_path_re.find_iter(&content) {
                let call = cap.as_str();
                let parts: Vec<&str> = call.split("::").collect();
                if parts.len() >= 2 {
                    let module = format!("{}::{}", parts[0], parts[1]);
                    local_modules.entry(module).or_default().insert(crate_name.clone());
                }
                
                let normalized = strip_generics(call);
                local_methods.entry(normalized).or_default().insert(crate_name.clone());
            }
            
            // Extract trait impl calls AND extract the implementing type
            // Pattern: <Vec<T> as IntoIterator>::into_iter -> type is Vec, method is into_iter
            for cap in trait_impl_re.find_iter(&content) {
                let call = cap.as_str();
                let normalized = strip_generics(call);
                local_methods.entry(normalized.clone()).or_default().insert(crate_name.clone());
                
                // Extract the implementing type from <Type as Trait>::method
                // Look for common stdlib types at the start of the angle bracket
                if let Some(start) = call.find('<') {
                    let after_bracket = &call[start + 1..];
                    // Check for known stdlib types
                    for type_name in ["Vec", "Option", "Result", "String", "Box", "Rc", "Arc", 
                                      "Cell", "RefCell", "Mutex", "RwLock", "HashMap", "BTreeMap",
                                      "HashSet", "BTreeSet", "VecDeque", "LinkedList", "BinaryHeap"] {
                        if after_bracket.starts_with(type_name) {
                            if let Some(qualified) = get_qualified_type(type_name) {
                                local_types.entry(qualified.to_string()).or_default().insert(crate_name.clone());
                                // Also add as a method on the type: Vec::into_iter
                                if let Some(method_start) = normalized.rfind("::") {
                                    let method_name = &normalized[method_start + 2..];
                                    let qualified_method = format!("{}::{}", qualified, method_name);
                                    local_methods.entry(qualified_method).or_default().insert(crate_name.clone());
                                }
                            }
                            break;
                        }
                    }
                }
            }
            
            // Extract stdlib type trait impl calls (e.g., <Vec<T> as IntoIterator>::into_iter)
            // These have unqualified trait names in MIR
            for cap in stdlib_type_trait_impl_re.captures_iter(&content) {
                if let (Some(type_match), Some(method_match)) = (cap.get(1), cap.get(2)) {
                    let type_name = type_match.as_str();
                    let method_name = method_match.as_str();
                    
                    if let Some(qualified_type) = get_qualified_type(type_name) {
                        local_types.entry(qualified_type.to_string()).or_default().insert(crate_name.clone());
                        let qualified_method = format!("{}::{}", qualified_type, method_name);
                        local_methods.entry(qualified_method).or_default().insert(crate_name.clone());
                    }
                }
            }
            
            // Extract qualified type method calls (std::result::Result::unwrap)
            for cap in type_method_qualified_re.find_iter(&content) {
                let call = cap.as_str();
                let normalized = strip_generics(call);
                // Extract qualified type: first 3 segments (std::module::Type)
                let parts: Vec<&str> = normalized.split("::").collect();
                if parts.len() >= 3 {
                    let qualified_type = format!("{}::{}::{}", parts[0], parts[1], parts[2]);
                    local_types.entry(qualified_type).or_default().insert(crate_name.clone());
                }
                local_methods.entry(normalized).or_default().insert(crate_name.clone());
            }
            
            // Extract unqualified type method calls (Option::<T>::Some, Result::<T,E>::unwrap)
            for cap in type_method_unqualified_re.captures_iter(&content) {
                if let (Some(type_match), Some(method_match)) = (cap.get(1), cap.get(2)) {
                    let type_name = type_match.as_str();
                    let method_name = method_match.as_str();
                    
                    if let Some(qualified_type) = get_qualified_type(type_name) {
                        let qualified_method = format!("{}::{}", qualified_type, method_name);
                        local_types.entry(qualified_type.to_string()).or_default().insert(crate_name.clone());
                        local_methods.entry(qualified_method).or_default().insert(crate_name.clone());
                    }
                }
            }
            
            // Extract type annotations (std::option::Option<...>, std::result::Result<...>)
            // These capture type USAGE without requiring method calls
            for cap in type_annotation_re.captures_iter(&content) {
                if let Some(type_match) = cap.get(1) {
                    let type_name = type_match.as_str();
                    if let Some(qualified_type) = get_qualified_type(type_name) {
                        local_types.entry(qualified_type.to_string()).or_default().insert(crate_name.clone());
                    }
                }
            }
            
            // Extract type constructors (Option::<T>::Some, Result::<T,E>::Ok)
            for cap in type_constructor_re.captures_iter(&content) {
                if let (Some(type_match), Some(ctor_match)) = (cap.get(1), cap.get(2)) {
                    let type_name = type_match.as_str();
                    let ctor_name = ctor_match.as_str();
                    
                    if let Some(qualified_type) = get_qualified_type(type_name) {
                        let qualified_ctor = format!("{}::{}", qualified_type, ctor_name);
                        local_types.entry(qualified_type.to_string()).or_default().insert(crate_name.clone());
                        local_methods.entry(qualified_ctor).or_default().insert(crate_name.clone());
                    }
                }
            }
            
            // Extract trait implementations: <Type as std::trait::Trait>::method
            for cap in trait_impl_pattern_re.captures_iter(&content) {
                if let (Some(type_match), Some(mod_match), Some(trait_match), Some(method_match)) = 
                    (cap.get(1), cap.get(2), cap.get(3), cap.get(4)) {
                    let impl_type = type_match.as_str();
                    let trait_module = mod_match.as_str();
                    let trait_name = trait_match.as_str();
                    let method_name = method_match.as_str();
                    
                    // Determine stdlib prefix based on module
                    let stdlib_prefix = match trait_module {
                        "iter" | "clone" | "cmp" | "ops" | "convert" | "default" | "marker" | 
                        "fmt" | "hash" | "option" | "result" | "slice" | "str" | "mem" |
                        "ptr" | "cell" | "num" | "any" | "error" | "future" | "task" => "core",
                        "io" | "fs" | "net" | "path" | "env" | "thread" | "sync" | "time" |
                        "process" | "ffi" | "collections" => "std",
                        "string" | "vec" | "boxed" | "rc" | "borrow" | "alloc" => "alloc",
                        _ => "std", // default to std
                    };
                    
                    // Create fully qualified trait name
                    let qualified_trait = format!("{}::{}::{}", stdlib_prefix, trait_module, trait_name);
                    // Create entry: "Type::method" for type breakdown
                    let impl_entry = format!("{}::{}", impl_type, method_name);
                    
                    local_trait_impls.entry(qualified_trait.clone())
                        .or_default()
                        .entry(impl_entry)
                        .or_default()
                        .insert(crate_name.clone());
                    
                    // Track the trait itself (for greedy cover of traits)
                    local_traits.entry(qualified_trait.clone())
                        .or_default()
                        .insert(crate_name.clone());
                    
                    // Also track by method name alone (for greedy cover across all types)
                    local_trait_impls.entry(qualified_trait)
                        .or_default()
                        .entry(format!("__METHOD__::{}", method_name))
                        .or_default()
                        .insert(crate_name.clone());
                }
            }
        }
        
        (project_name, mir_count, local_crates, local_modules, local_types, local_methods, local_traits, local_trait_impls)
    }).collect();
    
    eprintln!("\r[{}/{}] Done!                    ", total_projects, total_projects);
    
    // Merge results
    let mut module_project_crates: BTreeMap<String, BTreeMap<String, HashSet<String>>> = BTreeMap::new();
    let mut type_project_crates: BTreeMap<String, BTreeMap<String, HashSet<String>>> = BTreeMap::new();
    let mut method_project_crates: BTreeMap<String, BTreeMap<String, HashSet<String>>> = BTreeMap::new();
    let mut module_total_crates: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    let mut type_total_crates: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    let mut method_total_crates: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    // Traits: trait -> crates (for greedy cover of traits themselves)
    let mut trait_total_crates: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    // Trait impls: trait -> impl_entry -> crates
    let mut trait_impl_total: BTreeMap<String, BTreeMap<String, HashSet<String>>> = BTreeMap::new();
    let mut total_mir_files = 0;
    let mut unique_crates: HashSet<String> = HashSet::new();
    
    for (project_name, mir_count, local_crates, local_modules, local_types, local_methods, local_traits, local_trait_impls) in results {
        total_mir_files += mir_count;
        unique_crates.extend(local_crates);
        
        for (module, crates) in local_modules {
            module_project_crates.entry(module.clone()).or_default()
                .insert(project_name.clone(), crates.clone());
            module_total_crates.entry(module).or_default().extend(crates);
        }
        
        for (type_name, crates) in local_types {
            type_project_crates.entry(type_name.clone()).or_default()
                .insert(project_name.clone(), crates.clone());
            type_total_crates.entry(type_name).or_default().extend(crates);
        }
        
        for (method, crates) in local_methods {
            method_project_crates.entry(method.clone()).or_default()
                .insert(project_name.clone(), crates.clone());
            method_total_crates.entry(method).or_default().extend(crates);
        }
        
        // Merge traits
        for (trait_name, crates) in local_traits {
            trait_total_crates.entry(trait_name).or_default().extend(crates);
        }
        
        // Merge trait impls
        for (trait_name, impls) in local_trait_impls {
            let trait_entry = trait_impl_total.entry(trait_name).or_default();
            for (impl_entry, crates) in impls {
                trait_entry.entry(impl_entry).or_default().extend(crates);
            }
        }
    }
    
    println!("\n\nAnalysis complete!");
    
    // =========================================================================
    // HELPER: Greedy Set Cover Algorithm
    // =========================================================================
    
    fn greedy_cover_90(
        items: &BTreeMap<String, HashSet<String>>,
        total_count: usize,
    ) -> (usize, f64) {
        let target_pct = 90.0;
        let target_count = (total_count as f64 * target_pct / 100.0).ceil() as usize;
        let mut covered: HashSet<String> = HashSet::new();
        let mut count = 0;
        let mut remaining: Vec<_> = items.iter()
            .map(|(name, crates)| (name.clone(), crates.clone()))
            .collect();
        
        while covered.len() < target_count && !remaining.is_empty() {
            let mut best_idx = 0;
            let mut best_new = 0;
            
            for (idx, (_, crates)) in remaining.iter().enumerate() {
                let new_cov = crates.difference(&covered).count();
                if new_cov > best_new {
                    best_new = new_cov;
                    best_idx = idx;
                }
            }
            
            if best_new == 0 { break; }
            
            let (_, crates) = remaining.remove(best_idx);
            for c in &crates { covered.insert(c.clone()); }
            count += 1;
        }
        let achieved = (covered.len() as f64 / total_count as f64) * 100.0;
        (count, achieved)
    }
    
    // Helper: Check if a name is a method/function (not a type)
    fn is_method_not_type(name: &str) -> bool {
        // Exclude single words with no :: (like "Option", "Result")
        if !name.contains("::") {
            return name.chars().next().map(|c| c.is_lowercase()).unwrap_or(false);
        }
        // Check if the final segment starts with lowercase (method) not uppercase (type)
        if let Some(last_segment) = name.split("::").last() {
            let first_char = last_segment.chars().next().unwrap_or('A');
            // Methods start lowercase, types start uppercase
            first_char.is_lowercase() || last_segment.starts_with('<')
        } else {
            false
        }
    }
    
    // Filter methods to exclude type names
    let filtered_methods: BTreeMap<String, HashSet<String>> = method_total_crates.iter()
        .filter(|(name, _)| is_method_not_type(name))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    
    // Compute crates WITH stdlib usage (for accurate greedy cover percentages)
    let total_crate_count = unique_crates.len();
    let mut crates_with_stdlib: HashSet<String> = HashSet::new();
    for crates in method_total_crates.values() {
        crates_with_stdlib.extend(crates.iter().cloned());
    }
    for crates in module_total_crates.values() {
        crates_with_stdlib.extend(crates.iter().cloned());
    }
    let covered_crate_count = crates_with_stdlib.len();
    let uncovered_crates: Vec<String> = unique_crates.difference(&crates_with_stdlib).cloned().collect();
    let uncovered_count = uncovered_crates.len();
    let uncovered_pct = (uncovered_count as f64 / total_crate_count as f64) * 100.0;
    
    // Compute key stats for Abstract - use covered_crate_count as denominator!
    let (types_for_90, types_achieved) = greedy_cover_90(&type_total_crates, covered_crate_count);
    let (traits_for_90, traits_achieved) = greedy_cover_90(&trait_total_crates, covered_crate_count);
    let (methods_for_90, methods_achieved) = greedy_cover_90(&filtered_methods, covered_crate_count);
    
    // =========================================================================
    // FRONT MATTER: Abstract, Table of Contents, Overview
    // =========================================================================
    
    let report_title = "RUST STANDARD LIBRARY USAGE ANALYSIS";
    let report_line = "=".repeat(80);
    
    // i. ABSTRACT
    println!("\n\n{}", report_line);
    println!("{:^80}", report_title);
    println!("{}", report_line);
    println!();
    println!("i. ABSTRACT");
    println!("{}", "-".repeat(40));
    println!();
    println!("This report analyzes stdlib usage across {} Rust projects ({} unique crates)", 
        projects_with_mir.len(), unique_crates.len());
    println!("compiled to MIR. We identify which modules, types, traits, and methods are most");
    println!("used, and compute minimum sets needed to cover 70-99% of real-world usage.");
    println!();
    println!("Key findings:");
    println!("  - {} crates ({:.4}%) have no stdlib usage (proc-macros, FFI, const-only)", uncovered_count, uncovered_pct);
    println!("  - Of {} crates WITH stdlib usage:", covered_crate_count);
    println!("    - {} types cover {:.4}%", types_for_90, types_achieved);
    println!("    - {} traits cover {:.4}%", traits_for_90, traits_achieved);
    println!("    - {} methods cover {:.4}%", methods_for_90, methods_achieved);
    println!();
    
    log!("\n\n{}", report_line);
    log!("{:^80}", report_title);
    log!("{}", report_line);
    log!("\ni. ABSTRACT");
    log!("{}", "-".repeat(40));
    log!("\nThis report analyzes stdlib usage across {} Rust projects ({} unique crates)", 
        projects_with_mir.len(), unique_crates.len());
    log!("compiled to MIR. We identify which modules, types, traits, and methods are most");
    log!("used, and compute minimum sets needed to cover 70-99% of real-world usage.");
    log!("\nKey findings:");
    log!("  - {} crates ({:.4}%) have no stdlib usage (proc-macros, FFI, const-only)", uncovered_count, uncovered_pct);
    log!("  - Of {} crates WITH stdlib usage:", covered_crate_count);
    log!("    - {} types cover {:.4}%", types_for_90, types_achieved);
    log!("    - {} traits cover {:.4}%", traits_for_90, traits_achieved);
    log!("    - {} methods cover {:.4}%", methods_for_90, methods_achieved);
    
    // TABLE OF CONTENTS
    println!();
    println!("TABLE OF CONTENTS");
    println!("{}", "-".repeat(40));
    println!();
    println!("  i.   ABSTRACT");
    println!("  1.   OVERVIEW");
    println!("  2.   STDLIB MODULES (by crate count)");
    println!("  3.   CRATES WITHOUT STDLIB USAGE");
    println!("  4.   DATA TYPES (by crate count)");  
    println!("  5.   STDLIB TRAITS (by crate count)");
    println!("  6.   ALL STDLIB METHODS/FUNCTIONS (by crate count)");
    println!("  7.   GREEDY COVER: MODULES");
    println!("  8.   GREEDY COVER: DATA TYPES");
    println!("  9.   GREEDY COVER: TRAITS");
    println!(" 10.   GREEDY COVER: METHODS/FUNCTIONS");
    println!(" 11.   GREEDY COVER: METHODS PER TYPE");
    println!(" 12.   TRAIT IMPLEMENTATIONS AND GREEDY METHOD COVER");
    println!(" 13.   SUMMARY");
    println!();
    
    log!("\nTABLE OF CONTENTS");
    log!("{}", "-".repeat(40));
    log!("\n  i.   ABSTRACT");
    log!("  1.   OVERVIEW");
    log!("  2.   STDLIB MODULES (by crate count)");
    log!("  3.   CRATES WITHOUT STDLIB USAGE");
    log!("  4.   DATA TYPES (by crate count)");
    log!("  5.   STDLIB TRAITS (by crate count)");
    log!("  6.   ALL STDLIB METHODS/FUNCTIONS (by crate count)");
    log!("  7.   GREEDY COVER: MODULES");
    log!("  8.   GREEDY COVER: DATA TYPES");
    log!("  9.   GREEDY COVER: TRAITS");
    log!(" 10.   GREEDY COVER: METHODS/FUNCTIONS");
    log!(" 11.   GREEDY COVER: METHODS PER TYPE");
    log!(" 12.   TRAIT IMPLEMENTATIONS AND GREEDY METHOD COVER");
    log!(" 13.   SUMMARY");
    
    // 1. OVERVIEW
    println!();
    println!("{}", report_line);
    println!("1. OVERVIEW");
    println!("{}", report_line);
    println!();
    println!("MIR (Mid-level Intermediate Representation) is Rust's fully-typed intermediate");
    println!("representation generated during compilation. It captures all function calls, method");
    println!("invocations, trait implementations, and type instantiations with their complete");
    println!("qualified paths. From MIR we extract: direct method calls like Vec::new() and");
    println!("Result::unwrap(), trait method calls like <Vec as IntoIterator>::into_iter, type");
    println!("annotations like std::result::Result<T, E>, and constructor calls like");
    println!("Option::<T>::Some(...). Each stdlib usage is attributed to the crate that generated");
    println!("the MIR file.");
    println!();
    println!("MIR has limitations for usage analysis. Pattern matching (match result {{ Ok(x) => ... }})");
    println!("compiles to numeric discriminant checks, not named variant references - we can only");
    println!("detect when Ok/Err/Some/None are constructed, not matched. The ? operator generates");
    println!("FromResidual trait calls rather than explicit Result method calls. Type inference means");
    println!("many uses are implicit. Macros expand before MIR, so macro-generated code appears as");
    println!("regular calls. Consequently, our method and type coverage percentages represent a lower");
    println!("bound - actual stdlib usage is higher than what MIR-based analysis can detect.");
    println!();
    println!("Greedy cover analysis uses different denominators for accuracy: module coverage uses all");
    println!("crates with stdlib usage, type coverage uses only crates where we detected type usage,");
    println!("and method coverage uses only crates where we detected method calls. This ensures 100%%");
    println!("coverage is achievable for each category.");
    println!();
    println!("2. STDLIB MODULES (by crate count)");
    println!();
    println!("   Lists {} stdlib modules (std::, core::, alloc::) extracted from MIR files.", module_total_crates.len());
    println!("   Each module is counted once per crate that references it. Sorted by the number of");
    println!("   unique crates using each module, then alphabetically. Shows Uses, Uses%%, No-use,");
    println!("   No-use%% relative to the {} crates with detected stdlib usage.", covered_crate_count);
    println!();
    println!("3. CRATES WITHOUT STDLIB USAGE");
    println!();
    println!("   Lists {} crates where MIR analysis detected no stdlib calls. These are typically", uncovered_count);
    println!("   proc-macro crates (code runs at compile time), FFI stubs (extern \"C\" only), or");
    println!("   const-only crates with no runtime code. Excluded from coverage percentage calculations");
    println!("   since they cannot be covered by any stdlib item.");
    println!();
    println!("4. DATA TYPES (by crate count)");
    println!();
    println!("   Lists {} stdlib types (Result, Option, Vec, HashMap, etc.) found in MIR.", type_total_crates.len());
    println!("   Types are detected from: constructor calls (Some(...), Ok(...)), method receivers");
    println!("   (vec.push()), type annotations in locals, and generic instantiations. Each type is");
    println!("   counted once per crate. Shows Uses, Uses%%, No-use, No-use%% relative to crates with");
    println!("   detected type usage.");
    println!();
    println!("5. STDLIB TRAITS (by crate count)");
    println!();
    println!("   Lists {} stdlib traits (Iterator, Clone, Debug, IntoIterator, etc.) found in MIR.", trait_total_crates.len());
    println!("   Traits are detected from trait method calls (<Type as Trait>::method patterns).");
    println!("   Unlike types which are data structures, traits define behavioral contracts that must");
    println!("   be specified separately for Verus verification. Each trait counted once per crate.");
    println!();
    println!("6. ALL STDLIB METHODS/FUNCTIONS (by crate count)");
    println!();
    println!("   Lists all {} stdlib methods and functions found in MIR, sorted by crate count.", method_total_crates.len());
    println!("   Extracted from direct calls (Vec::new()), method calls (result.unwrap()), and trait");
    println!("   method calls (<T as Iterator>::next()). Generic parameters are stripped for grouping");
    println!("   (Vec::<T>::push becomes Vec::push). Type names filtered out to show only methods.");
    println!();
    println!("7. GREEDY COVER: MODULES");
    println!();
    println!("   Applies greedy set cover algorithm to find minimum modules needed to cover 70%%,");
    println!("   80%%, 90%%, 95%%, 99%%, 100%% of the {} crates with stdlib usage. At each step,", covered_crate_count);
    println!("   selects the module covering the most remaining uncovered crates. Shows cumulative");
    println!("   coverage percentage after each module is added.");
    println!();
    println!("8. GREEDY COVER: DATA TYPES");
    println!();
    println!("   Applies greedy set cover to find minimum types needed for each coverage target.");
    println!("   Uses only crates where type usage was detected as the denominator, ensuring 100%%");
    println!("   coverage is achievable. Critical for verification prioritization: which types must");
    println!("   be formally specified first to cover the most real-world code.");
    println!();
    println!("9. GREEDY COVER: TRAITS");
    println!();
    println!("   Applies greedy set cover to find minimum traits needed for each coverage target.");
    println!("   Traits like Iterator, Clone, and IntoIterator require separate specs from their");
    println!("   implementing types. This shows which trait specs provide the best coverage ROI.");
    println!();
    println!("10. GREEDY COVER: METHODS/FUNCTIONS");
    println!();
    println!("   Applies greedy set cover to methods/functions. Filters out type names to count only");
    println!("   actual callable methods. Uses crates with detected method calls as denominator.");
    println!("   Key insight: a small fraction of stdlib methods cover the vast majority of usage.");
    println!("   For Verus verification, proves which method specs provide the best coverage ROI.");
    println!();
    println!("11. GREEDY COVER: METHODS PER TYPE");
    println!();
    println!("   For each major type (Vec, Option, Result, String, etc.), runs greedy set cover on");
    println!("   its methods. Denominator is crates that call methods on that specific type (not just");
    println!("   annotate it). Shows minimum methods needed per type for 70-100%% coverage. Critical");
    println!("   for prioritizing which Vec::* or Option::* methods to verify first.");
    println!();
    println!("12. TRAIT IMPLEMENTATIONS AND GREEDY METHOD COVER");
    println!();
    println!("   Extracts trait implementations from MIR (e.g., <Vec as IntoIterator>::into_iter).");
    println!("   Groups by trait, showing which types implement each trait and which methods are");
    println!("   called. For key traits (Iterator, Clone, Debug, io::Read/Write), runs greedy cover");
    println!("   on trait methods. Iterator has 70+ methods but ~7 cover 99%% of real usage.");
    println!();
    println!("13. SUMMARY");
    println!();
    println!("   Final statistics: total projects, crates analyzed (with/without stdlib), modules,");
    println!("   types, traits, methods detected. Includes total analysis time.");
    println!();
    println!("{}", report_line);
    println!();
    
    log!("\n{}", report_line);
    log!("1. OVERVIEW");
    log!("{}", report_line);
    log!("");
    log!("MIR (Mid-level Intermediate Representation) is Rust's fully-typed intermediate");
    log!("representation generated during compilation. It captures all function calls, method");
    log!("invocations, trait implementations, and type instantiations with their complete");
    log!("qualified paths. From MIR we extract: direct method calls like Vec::new() and");
    log!("Result::unwrap(), trait method calls like <Vec as IntoIterator>::into_iter, type");
    log!("annotations like std::result::Result<T, E>, and constructor calls like");
    log!("Option::<T>::Some(...). Each stdlib usage is attributed to the crate that generated");
    log!("the MIR file.");
    log!("");
    log!("MIR has limitations for usage analysis. Pattern matching (match result {{ Ok(x) => ... }})");
    log!("compiles to numeric discriminant checks, not named variant references - we can only");
    log!("detect when Ok/Err/Some/None are constructed, not matched. The ? operator generates");
    log!("FromResidual trait calls rather than explicit Result method calls. Type inference means");
    log!("many uses are implicit. Macros expand before MIR, so macro-generated code appears as");
    log!("regular calls. Consequently, our method and type coverage percentages represent a lower");
    log!("bound - actual stdlib usage is higher than what MIR-based analysis can detect.");
    log!("");
    log!("Greedy cover analysis uses different denominators for accuracy: module coverage uses all");
    log!("crates with stdlib usage, type coverage uses only crates where we detected type usage,");
    log!("and method coverage uses only crates where we detected method calls. This ensures 100%");
    log!("coverage is achievable for each category.");
    log!("\n2. STDLIB MODULES (by crate count)");
    log!("");
    log!("   This helps us answer: Which stdlib modules are most widely used across real codebases?");
    log!("");
    log!("   Lists {} stdlib modules (std::, core::, alloc::) extracted from MIR files.", module_total_crates.len());
    log!("   Each module is counted once per crate that references it. Sorted by the number of");
    log!("   unique crates using each module, then alphabetically. Shows Uses, Uses%%, No-use,");
    log!("   No-use%% relative to the {} crates with detected stdlib usage.", covered_crate_count);
    log!("\n3. CRATES WITHOUT STDLIB USAGE");
    log!("");
    log!("   This helps us answer: How many crates don't use stdlib at all?");
    log!("");
    log!("   Lists {} crates where MIR analysis detected no stdlib calls. These are typically", uncovered_count);
    log!("   proc-macro crates (code runs at compile time), FFI stubs (extern \"C\" only), or");
    log!("   const-only crates with no runtime code. Excluded from coverage percentage calculations");
    log!("   since they cannot be covered by any stdlib item.");
    log!("\n4. DATA TYPES (by crate count)");
    log!("");
    log!("   This helps us answer: Which stdlib types are most frequently used?");
    log!("");
    log!("   Lists {} stdlib types (Result, Option, Vec, HashMap, etc.) found in MIR.", type_total_crates.len());
    log!("   Types are detected from: constructor calls (Some(...), Ok(...)), method receivers");
    log!("   (vec.push()), type annotations in locals, and generic instantiations. Each type is");
    log!("   counted once per crate. Shows Uses, Uses%%, No-use, No-use%% relative to crates with");
    log!("   detected type usage.");
    log!("\n5. STDLIB TRAITS (by crate count)");
    log!("");
    log!("   This helps us answer: Which stdlib traits are most frequently used?");
    log!("");
    log!("   Lists {} stdlib traits (Iterator, Clone, Debug, IntoIterator, etc.) found in MIR.", trait_total_crates.len());
    log!("   Traits are detected from trait method calls (<Type as Trait>::method patterns).");
    log!("   Unlike types which are data structures, traits define behavioral contracts that must");
    log!("   be specified separately for Verus verification. Each trait counted once per crate.");
    log!("\n6. ALL STDLIB METHODS/FUNCTIONS (by crate count)");
    log!("");
    log!("   This helps us answer: Which specific methods are called most often?");
    log!("");
    log!("   Lists all {} stdlib methods and functions found in MIR, sorted by crate count.", method_total_crates.len());
    log!("   Extracted from direct calls (Vec::new()), method calls (result.unwrap()), and trait");
    log!("   method calls (<T as Iterator>::next()). Generic parameters are stripped for grouping");
    log!("   (Vec::<T>::push becomes Vec::push). Type names filtered out to show only methods.");
    log!("\n7. GREEDY COVER: MODULES");
    log!("");
    log!("   This helps us answer: What modules should we verify first for maximum IMPACT?");
    log!("");
    log!("   Applies greedy set cover to find minimum modules to TOUCH 70-100%% of crates.");
    log!("   'Touching' means at least one crate uses that module. Useful for prioritization:");
    log!("   verifying std::result first impacts 84%% of crates. Note: this does NOT mean those");
    log!("   crates are 'fully supported' - they may use other modules too.");
    log!("\n8. GREEDY COVER: DATA TYPES");
    log!("");
    log!("   This helps us answer: What types should we verify first for maximum IMPACT?");
    log!("");
    log!("   Applies greedy set cover to find minimum types to TOUCH 70-100%% of crates.");
    log!("   Like module cover, this shows which types have the widest usage. Result and Option");
    log!("   alone touch 95%%+ of crates. Does NOT mean crates only need those types.");
    log!("\n9. GREEDY COVER: TRAITS");
    log!("");
    log!("   This helps us answer: What traits should we verify first for maximum IMPACT?");
    log!("");
    log!("   Applies greedy set cover to find minimum traits to TOUCH 70-100%% of crates.");
    log!("   Traits like Iterator, Clone, and IntoIterator require separate specs from their");
    log!("   implementing types. This shows which trait specs provide the best coverage ROI.");
    log!("\n10. GREEDY COVER: METHODS/FUNCTIONS");
    log!("");
    log!("   This helps us answer: What methods should we verify first for maximum IMPACT?");
    log!("");
    log!("   Applies greedy set cover to find minimum methods to TOUCH 70-100%% of crates.");
    log!("   Key insight: ~30 methods touch 90%% of crates. For verification prioritization,");
    log!("   this shows which method specs provide the best coverage ROI.");
    log!("\n11. GREEDY COVER: METHODS PER TYPE");
    log!("");
    log!("   This helps us answer: For each type, which methods matter most?");
    log!("");
    log!("   For each type (Vec, Option, Result, etc.), runs greedy cover on its methods.");
    log!("   Shows minimum methods needed per type for 70-100%% coverage of that type's users.");
    log!("\n12. TRAIT IMPLEMENTATIONS AND GREEDY METHOD COVER");
    log!("");
    log!("   This helps us answer: Which trait methods are actually used in practice?");
    log!("");
    log!("   Extracts trait implementations from MIR (e.g., <Vec as IntoIterator>::into_iter).");
    log!("   For key traits (Iterator, Clone, Debug), runs greedy cover on trait methods.");
    log!("   Iterator has 70+ methods but ~7 cover 99%% of real usage.");
    log!("\n13. SUMMARY");
    log!("");
    log!("   Final statistics: total projects, crates analyzed (with/without stdlib), modules,");
    log!("   types, traits, methods detected. Includes total analysis time.");
    log!("\n{}", report_line);
    
    // =========================================================================
    // SECTION 2: STDLIB MODULES
    // =========================================================================
    
    println!("Total projects: {}", projects_with_mir.len());
    println!("Unique crates: {} ({} with stdlib, {} without)", 
        unique_crates.len(), covered_crate_count, uncovered_count);
    println!("MIR files analyzed: {}", total_mir_files);
    log!("\nTotal projects: {}", projects_with_mir.len());
    log!("Unique crates: {} ({} with stdlib, {} without)", 
        unique_crates.len(), covered_crate_count, uncovered_count);
    log!("MIR files analyzed: {}", total_mir_files);
    
    // Helper to compute stats over crate counts per project
    fn compute_crate_stats(project_crates: &BTreeMap<String, HashSet<String>>) -> (usize, usize, f64) {
        if project_crates.is_empty() {
            return (0, 0, 0.0);
        }
        let values: Vec<usize> = project_crates.values().map(|s| s.len()).collect();
        let min = *values.iter().min().unwrap_or(&0);
        let max = *values.iter().max().unwrap_or(&0);
        let avg = values.iter().sum::<usize>() as f64 / values.len() as f64;
        (min, max, avg)
    }
    
    let total_crate_count = unique_crates.len();
    
    // Print module stats - Uses / No-use format
    println!("\n=== 2. STDLIB MODULES (by crate count) ===");
    println!("{:40} {:>8} {:>8} {:>8} {:>8}", "Module", "Crates", "Crates%", "No-use", "No-use%");
    println!("{}", "-".repeat(76));
    log!("\n=== 2. STDLIB MODULES (by crate count) ===");
    log!("{:40} {:>8} {:>8} {:>8} {:>8}", "Module", "Crates", "Crates%", "No-use", "No-use%");
    
    let mut module_stats: Vec<_> = module_total_crates.iter().map(|(name, crates)| {
        let uses = crates.len();
        let uses_pct = (uses as f64 / total_crate_count as f64) * 100.0;
        let no_use = total_crate_count - uses;
        let no_use_pct = (no_use as f64 / total_crate_count as f64) * 100.0;
        (name.clone(), uses, uses_pct, no_use, no_use_pct)
    }).collect();
    module_stats.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    
    for (name, uses, uses_pct, no_use, no_use_pct) in &module_stats {
        println!("{:40} {:>8} {:>10.4}% {:>8} {:>10.4}%", name, uses, uses_pct, no_use, no_use_pct);
        log!("{:40} {:>8} {:>10.4}% {:>8} {:>10.4}%", name, uses, uses_pct, no_use, no_use_pct);
    }
    
    // Section 3: Crates without stdlib usage
    println!("\n=== 3. CRATES WITHOUT STDLIB USAGE ===");
    println!("These {} crates ({:.4}% of total) are excluded from greedy cover calculations.", 
        uncovered_count, uncovered_pct);
    println!("Typically: proc-macros, FFI bindings, const-only, or build scripts.");
    println!("{}", "-".repeat(76));
    log!("\n=== 3. CRATES WITHOUT STDLIB USAGE ===");
    log!("These {} crates ({:.4}% of total) are excluded from greedy cover calculations.", 
        uncovered_count, uncovered_pct);
    log!("Typically: proc-macros, FFI bindings, const-only, or build scripts.");
    
    let mut sorted_uncovered = uncovered_crates.clone();
    sorted_uncovered.sort();
    for name in &sorted_uncovered {
        println!("  {}", name);
        log!("  {}", name);
    }
    
    // Print type stats
    println!("\n=== 4. DATA TYPES (by crate count) ===");
    println!("{:40} {:>8} {:>8} {:>8} {:>8}", "Type", "Crates", "Crates%", "No-use", "No-use%");
    println!("{}", "-".repeat(76));
    log!("\n=== 4. DATA TYPES (by crate count) ===");
    log!("{:40} {:>8} {:>8} {:>8} {:>8}", "Type", "Crates", "Crates%", "No-use", "No-use%");
    
    let mut type_stats: Vec<_> = type_total_crates.iter().map(|(name, crates)| {
        let uses = crates.len();
        let uses_pct = (uses as f64 / total_crate_count as f64) * 100.0;
        let no_use = total_crate_count - uses;
        let no_use_pct = (no_use as f64 / total_crate_count as f64) * 100.0;
        (name.clone(), uses, uses_pct, no_use, no_use_pct)
    }).collect();
    type_stats.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    
    for (name, uses, uses_pct, no_use, no_use_pct) in &type_stats {
        println!("{:40} {:>8} {:>10.4}% {:>8} {:>10.4}%", name, uses, uses_pct, no_use, no_use_pct);
        log!("{:40} {:>8} {:>10.4}% {:>8} {:>10.4}%", name, uses, uses_pct, no_use, no_use_pct);
    }
    
    // Print trait stats - NEW SECTION 5
    println!("\n=== 5. STDLIB TRAITS (by crate count) ===");
    println!("This helps us answer: Which stdlib traits are most frequently used?");
    println!("{:50} {:>8} {:>10} {:>8} {:>10}", "Trait", "Crates", "Crates%", "No-use", "No-use%");
    println!("{}", "-".repeat(90));
    log!("\n=== 5. STDLIB TRAITS (by crate count) ===");
    log!("This helps us answer: Which stdlib traits are most frequently used?");
    log!("{:50} {:>8} {:>10} {:>8} {:>10}", "Trait", "Crates", "Crates%", "No-use", "No-use%");
    log!("{}", "-".repeat(90));
    
    let mut trait_stats: Vec<_> = trait_total_crates.iter().map(|(name, crates)| {
        let uses = crates.len();
        let uses_pct = (uses as f64 / covered_crate_count as f64) * 100.0;
        let no_use = covered_crate_count - uses;
        let no_use_pct = (no_use as f64 / covered_crate_count as f64) * 100.0;
        (name.clone(), uses, uses_pct, no_use, no_use_pct)
    }).collect();
    trait_stats.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    
    for (name, uses, uses_pct, no_use, no_use_pct) in &trait_stats {
        println!("{:50} {:>8} {:>10.4}% {:>8} {:>10.4}%", name, uses, uses_pct, no_use, no_use_pct);
        log!("{:50} {:>8} {:>10.4}% {:>8} {:>10.4}%", name, uses, uses_pct, no_use, no_use_pct);
    }
    
    // Print ALL method stats (percentages based on crates WITH stdlib usage)
    println!("\n=== 6. ALL STDLIB METHODS/FUNCTIONS (by crate count) ===");
    println!("This helps us answer: Which specific methods are called most often?");
    println!("({} entries, percentages of {} crates with stdlib)", method_total_crates.len(), covered_crate_count);
    println!("{:60} {:>8} {:>8} {:>8} {:>8}", "Method/Function", "Crates", "Crates%", "No-use", "No-use%");
    println!("{}", "-".repeat(96));
    log!("\n=== 6. ALL STDLIB METHODS/FUNCTIONS (by crate count) ===");
    log!("This helps us answer: Which specific methods are called most often?");
    log!("({} entries, percentages of {} crates with stdlib)", method_total_crates.len(), covered_crate_count);
    log!("{:60} {:>8} {:>8} {:>8} {:>8}", "Method/Function", "Crates", "Crates%", "No-use", "No-use%");
    
    let mut method_stats: Vec<_> = method_total_crates.iter().map(|(name, crates)| {
        let calls = crates.len();
        let calls_pct = (calls as f64 / covered_crate_count as f64) * 100.0;
        let no_call = covered_crate_count - calls;
        let no_call_pct = (no_call as f64 / covered_crate_count as f64) * 100.0;
        (name.clone(), calls, calls_pct, no_call, no_call_pct)
    }).collect();
    method_stats.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    
    for (name, calls, calls_pct, no_call, no_call_pct) in &method_stats {
        let display_name = if name.len() > 60 { format!("{}...", &name[..57]) } else { name.clone() };
        println!("{:60} {:>8} {:>10.4}% {:>8} {:>10.4}%", display_name, calls, calls_pct, no_call, no_call_pct);
        log!("{:60} {:>8} {:>10.4}% {:>8} {:>10.4}%", name, calls, calls_pct, no_call, no_call_pct);
    }
    
    // =========================================================================
    // === GREEDY SET COVER ANALYSIS ===
    // Description: Uses greedy set cover algorithm to find minimum items needed
    // to cover target percentages (70%, 80%, 90%, 95%, 99%) of crates.
    // Three separate analyses:
    // 1. Modules only - which stdlib modules to prioritize
    // 2. Methods only - which stdlib methods/functions to prioritize  
    // 3. Per-module methods - within each module, which methods matter most
    // =========================================================================
    
    let greedy_start = std::time::Instant::now();
    let targets = [70.0, 80.0, 90.0, 95.0, 99.0, 100.0];
    
    // Helper function for greedy set cover
    fn greedy_cover(
        items: &BTreeMap<String, HashSet<String>>,
        target_pct: f64,
        total_count: usize,
    ) -> Vec<(String, usize, f64)> {
        let target_count = (total_count as f64 * target_pct / 100.0).ceil() as usize;
        let mut covered: HashSet<String> = HashSet::new();
        let mut selected: Vec<(String, usize, f64)> = Vec::new();
        let mut remaining: Vec<_> = items.iter()
            .map(|(name, crates)| (name.clone(), crates.clone()))
            .collect();
        
        while covered.len() < target_count && !remaining.is_empty() {
            let mut best_idx = 0;
            let mut best_new = 0;
            
            for (idx, (_, crates)) in remaining.iter().enumerate() {
                let new_cov = crates.difference(&covered).count();
                if new_cov > best_new {
                    best_new = new_cov;
                    best_idx = idx;
                }
            }
            
            if best_new == 0 { break; }
            
            let (name, crates) = remaining.remove(best_idx);
            for c in &crates { covered.insert(c.clone()); }
            
            let cum_pct = (covered.len() as f64 / total_count as f64) * 100.0;
            selected.push((name, best_new, cum_pct));
        }
        selected
    }
    
    // Full support greedy cover: how many items must be verified to FULLY SUPPORT X% of crates?
    // A crate is "fully supported" when ALL items it uses are verified.
    #[allow(dead_code)]
    fn greedy_cover_full_support(
        crate_to_items: &BTreeMap<String, HashSet<String>>,
        target_pct: f64,
    ) -> Vec<(String, usize, f64)> {
        let total_crates = crate_to_items.len();
        let target_count = (total_crates as f64 * target_pct / 100.0).ceil() as usize;
        
        // Collect all unique items
        let all_items: HashSet<String> = crate_to_items.values()
            .flat_map(|s| s.iter().cloned())
            .collect();
        
        let mut verified_items: HashSet<String> = HashSet::new();
        let mut selected: Vec<(String, usize, f64)> = Vec::new();
        
        // Count crates fully supported
        let count_fully_supported = |verified: &HashSet<String>| -> usize {
            crate_to_items.iter()
                .filter(|(_, items)| items.iter().all(|i| verified.contains(i)))
                .count()
        };
        
        let mut current_supported = count_fully_supported(&verified_items);
        
        while current_supported < target_count {
            // Find item that maximizes newly fully-supported crates
            let mut best_item = None;
            let mut best_delta = 0;
            
            for item in all_items.iter() {
                if verified_items.contains(item) { continue; }
                
                let mut test = verified_items.clone();
                test.insert(item.clone());
                let new_supported = count_fully_supported(&test);
                let delta = new_supported - current_supported;
                
                if delta > best_delta {
                    best_delta = delta;
                    best_item = Some(item.clone());
                }
            }
            
            if let Some(item) = best_item {
                verified_items.insert(item.clone());
                current_supported += best_delta;
                let pct = (current_supported as f64 / total_crates as f64) * 100.0;
                selected.push((item, best_delta, pct));
            } else {
                // No single item improves support - add most common remaining
                let remaining: Vec<_> = all_items.iter()
                    .filter(|i| !verified_items.contains(*i))
                    .collect();
                if let Some(item) = remaining.first() {
                    verified_items.insert((*item).clone());
                    let pct = (current_supported as f64 / total_crates as f64) * 100.0;
                    selected.push(((*item).clone(), 0, pct));
                } else {
                    break;
                }
            }
        }
        selected
    }
    
    // =========================================================================
    // === GREEDY COVER: MODULES ONLY ===
    // Description: Find minimum set of stdlib modules to cover N% of crates.
    // Useful for prioritizing which module documentation/testing to focus on.
    // =========================================================================
    
    // Calculate crates that use at least one stdlib module
    let mut crates_with_modules: HashSet<String> = HashSet::new();
    for crates in module_total_crates.values() {
        crates_with_modules.extend(crates.iter().cloned());
    }
    let module_covered_count = crates_with_modules.len();
    let module_uncovered_count = covered_crate_count - module_covered_count;
    
    println!("\n=== 7. GREEDY COVER: MODULES ===");
    println!("This helps us answer: What modules should we verify first for maximum IMPACT?");
    println!("Description: Minimum stdlib modules to cover (70/80/90/95/99/100)% of {} crates WITH stdlib module usage.", module_covered_count);
    println!("Excludes {} crates with no stdlib usage, {} more with no module usage.", uncovered_count, module_uncovered_count);
    println!("{}", "=".repeat(80));
    log!("\n=== 7. GREEDY COVER: MODULES ===");
    log!("This helps us answer: What modules should we verify first for maximum IMPACT?");
    log!("Description: Minimum stdlib modules to cover (70/80/90/95/99/100)% of {} crates WITH stdlib module usage.", module_covered_count);
    log!("Excludes {} crates with no stdlib usage, {} more with no module usage.", uncovered_count, module_uncovered_count);
    
    for &target in &targets {
        let result = greedy_cover(&module_total_crates, target, module_covered_count);
        let achieved = result.last().map(|(_, _, p)| *p).unwrap_or(0.0);
        
        println!("\n--- Target: {:.0}% ({} crates) ---", target, 
            (module_covered_count as f64 * target / 100.0).ceil() as usize);
        log!("\n--- Target: {:.0}% ({} crates) ---", target,
            (module_covered_count as f64 * target / 100.0).ceil() as usize);
        
        for (i, (name, new_cov, cum_pct)) in result.iter().enumerate() {
            println!("  {:3}. {:40} +{:>5} ({:>8.4}%)", i + 1, name, new_cov, cum_pct);
            log!("  {:3}. {:40} +{:>5} ({:>8.4}%)", i + 1, name, new_cov, cum_pct);
        }
        println!("  => {} modules achieve {:.4}%", result.len(), achieved);
        log!("  => {} modules achieve {:.4}%", result.len(), achieved);
    }
    
    // =========================================================================
    // === GREEDY COVER: DATA TYPES ===
    // Description: Find minimum set of stdlib types to cover N% of crates.
    // =========================================================================
    
    // Calculate crates that use at least one stdlib type
    let mut crates_with_types: HashSet<String> = HashSet::new();
    for crates in type_total_crates.values() {
        crates_with_types.extend(crates.iter().cloned());
    }
    let type_covered_count = crates_with_types.len();
    let type_uncovered_count = covered_crate_count - type_covered_count;
    
    println!("\n\n=== 8. GREEDY COVER: DATA TYPES ===");
    println!("This helps us answer: What types should we verify first for maximum IMPACT?");
    println!("Description: Minimum data types to cover (70/80/90/95/99/100)% of {} crates WITH stdlib type usage.", type_covered_count);
    println!("Excludes {} crates with no stdlib usage, {} more with no type usage.", uncovered_count, type_uncovered_count);
    println!("{}", "=".repeat(80));
    log!("\n\n=== 8. GREEDY COVER: DATA TYPES ===");
    log!("This helps us answer: What types should we verify first for maximum IMPACT?");
    log!("Description: Minimum data types to cover (70/80/90/95/99/100)% of {} crates WITH stdlib type usage.", type_covered_count);
    log!("Excludes {} crates with no stdlib usage, {} more with no type usage.", uncovered_count, type_uncovered_count);
    
    // List all data types upfront
    println!("\nAll {} data types:", type_total_crates.len());
    log!("\nAll {} data types:", type_total_crates.len());
    let mut sorted_types: Vec<_> = type_total_crates.iter()
        .map(|(name, crates)| (name.clone(), crates.len()))
        .collect();
    sorted_types.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    for (name, count) in &sorted_types {
        println!("  {:50} {:>5} crates", name, count);
        log!("  {:50} {:>5} crates", name, count);
    }
    println!();
    log!("");
    
    for &target in &targets {
        let result = greedy_cover(&type_total_crates, target, type_covered_count);
        let achieved = result.last().map(|(_, _, p)| *p).unwrap_or(0.0);
        
        println!("\n--- Target: {:.0}% ({} crates) ---", target,
            (type_covered_count as f64 * target / 100.0).ceil() as usize);
        log!("\n--- Target: {:.0}% ({} crates) ---", target,
            (type_covered_count as f64 * target / 100.0).ceil() as usize);
        
        for (i, (name, new_cov, cum_pct)) in result.iter().enumerate() {
            println!("  {:3}. {:40} +{:>5} ({:>8.4}%)", i + 1, name, new_cov, cum_pct);
            log!("  {:3}. {:40} +{:>5} ({:>8.4}%)", i + 1, name, new_cov, cum_pct);
        }
        println!("  => {} types achieve {:.4}%", result.len(), achieved);
        log!("  => {} types achieve {:.4}%", result.len(), achieved);
    }
    
    // =========================================================================
    // === GREEDY COVER: METHODS ONLY ===
    // Description: Find minimum set of stdlib methods/functions to cover N% of crates.
    // Useful for prioritizing which functions to test/verify/document.
    // =========================================================================
    
    // Calculate crates that use at least one stdlib method
    let mut crates_with_methods: HashSet<String> = HashSet::new();
    for crates in filtered_methods.values() {
        crates_with_methods.extend(crates.iter().cloned());
    }
    let method_covered_count = crates_with_methods.len();
    let method_uncovered_count = covered_crate_count - method_covered_count;
    
    // =========================================================================
    // === GREEDY COVER: TRAITS ===
    // Description: Find minimum set of stdlib traits to cover N% of crates.
    // =========================================================================
    
    // Calculate crates that use at least one stdlib trait
    let mut crates_with_traits: HashSet<String> = HashSet::new();
    for crates in trait_total_crates.values() {
        crates_with_traits.extend(crates.iter().cloned());
    }
    let trait_covered_count = crates_with_traits.len();
    let trait_uncovered_count = covered_crate_count - trait_covered_count;
    
    println!("\n\n=== 9. GREEDY COVER: TRAITS ===");
    println!("This helps us answer: What traits should we verify first for maximum IMPACT?");
    println!("Description: Minimum stdlib traits to cover (70/80/90/95/99/100)% of {} crates WITH stdlib trait usage.", trait_covered_count);
    println!("Excludes {} crates with no stdlib usage, {} more with no trait usage. {} traits total.", 
        uncovered_count, trait_uncovered_count, trait_total_crates.len());
    println!("{}", "=".repeat(80));
    log!("\n\n=== 9. GREEDY COVER: TRAITS ===");
    log!("This helps us answer: What traits should we verify first for maximum IMPACT?");
    log!("Description: Minimum stdlib traits to cover (70/80/90/95/99/100)% of {} crates WITH stdlib trait usage.", trait_covered_count);
    log!("Excludes {} crates with no stdlib usage, {} more with no trait usage. {} traits total.", 
        uncovered_count, trait_uncovered_count, trait_total_crates.len());
    
    for &target in &targets {
        let result = greedy_cover(&trait_total_crates, target, trait_covered_count);
        let achieved = result.last().map(|(_, _, p)| *p).unwrap_or(0.0);
        
        println!("\n--- Target: {:.0}% ({} crates) ---", target,
            (trait_covered_count as f64 * target / 100.0).ceil() as usize);
        log!("\n--- Target: {:.0}% ({} crates) ---", target,
            (trait_covered_count as f64 * target / 100.0).ceil() as usize);
        
        for (i, (name, new_cov, cum_pct)) in result.iter().enumerate() {
            let display = if name.len() > 50 { format!("{}...", &name[..47]) } else { name.clone() };
            if i < 20 {
                println!("  {:3}. {:50} +{:>5} ({:>8.4}%)", i + 1, display, new_cov, cum_pct);
            }
            log!("  {:3}. {:50} +{:>5} ({:>8.4}%)", i + 1, name, new_cov, cum_pct);
        }
        if result.len() > 20 {
            println!("  ... ({} more in log)", result.len() - 20);
        }
        println!("  => {} traits achieve {:.4}%", result.len(), achieved);
        log!("  => {} traits achieve {:.4}%", result.len(), achieved);
    }
    
    // =========================================================================
    // === GREEDY COVER: METHODS/FUNCTIONS ===
    // Description: Find minimum set of stdlib methods to cover N% of crates.
    // =========================================================================
    
    println!("\n\n=== 10. GREEDY COVER: METHODS/FUNCTIONS ===");
    println!("This helps us answer: What methods should we verify first for maximum IMPACT?");
    println!("Description: Minimum methods to cover (70/80/90/95/99/100)% of {} crates WITH stdlib method usage.", method_covered_count);
    println!("Excludes {} crates with no stdlib, {} more with no method usage. {} methods total.", 
        uncovered_count, method_uncovered_count, filtered_methods.len());
    println!("{}", "=".repeat(80));
    log!("\n\n=== 10. GREEDY COVER: METHODS/FUNCTIONS ===");
    log!("This helps us answer: What methods should we verify first for maximum IMPACT?");
    log!("Description: Minimum methods to cover (70/80/90/95/99/100)% of {} crates WITH stdlib method usage.", method_covered_count);
    log!("Excludes {} crates with no stdlib, {} more with no method usage. {} methods total.", 
        uncovered_count, method_uncovered_count, filtered_methods.len());
    
    for &target in &targets {
        let result = greedy_cover(&filtered_methods, target, method_covered_count);
        let achieved = result.last().map(|(_, _, p)| *p).unwrap_or(0.0);
        
        println!("\n--- Target: {:.0}% ({} crates) ---", target,
            (method_covered_count as f64 * target / 100.0).ceil() as usize);
        log!("\n--- Target: {:.0}% ({} crates) ---", target,
            (method_covered_count as f64 * target / 100.0).ceil() as usize);
        
        // Show first 20 for each target, full list in log
        for (i, (name, new_cov, cum_pct)) in result.iter().enumerate() {
            let display = if name.len() > 50 { format!("{}...", &name[..47]) } else { name.clone() };
            if i < 20 {
                println!("  {:3}. {:50} +{:>5} ({:>8.4}%)", i + 1, display, new_cov, cum_pct);
            }
            log!("  {:3}. {:50} +{:>5} ({:>8.4}%)", i + 1, name, new_cov, cum_pct);
        }
        if result.len() > 20 {
            println!("  ... ({} more in log)", result.len() - 20);
        }
        println!("  => {} methods achieve {:.4}%", result.len(), achieved);
        log!("  => {} methods achieve {:.4}%", result.len(), achieved);
    }
    
    // =========================================================================
    // === GREEDY COVER: METHODS PER TYPE ===
    // Description: For each stdlib type, find minimum methods to cover 90% of 
    // crates that use that type. Shows which methods matter most per type.
    // =========================================================================
    
    println!("\n\n=== 11. GREEDY COVER: METHODS PER TYPE ===");
    println!("This helps us answer: For each type, which methods matter most?");
    println!("Description: For each data type, minimum methods to cover 70/80/90/95/99/100% of its users");
    println!("{}", "=".repeat(80));
    log!("\n\n=== 11. GREEDY COVER: METHODS PER TYPE ===");
    log!("This helps us answer: For each type, which methods matter most?");
    log!("Description: For each data type, minimum methods to cover 70/80/90/95/99/100% of its users");
    
    // List all types with their method counts upfront
    println!("\nTypes to analyze (sorted by crate count, min 10 users):");
    log!("\nTypes to analyze (sorted by crate count, min 10 users):");
    
    // Group methods by their qualified type (e.g., "std::result::Result" from "std::result::Result::unwrap")
    let mut methods_by_type: BTreeMap<String, BTreeMap<String, HashSet<String>>> = BTreeMap::new();
    for (method, crates) in &filtered_methods {
        let parts: Vec<&str> = method.split("::").collect();
        // For stdlib methods, extract qualified type: first 3 segments (std::module::Type)
        if parts.len() >= 4 && (parts[0] == "std" || parts[0] == "core" || parts[0] == "alloc") {
            // Check if 3rd part is a type (PascalCase)
            if parts[2].chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                let qualified_type = format!("{}::{}::{}", parts[0], parts[1], parts[2]);
                methods_by_type
                    .entry(qualified_type)
                    .or_default()
                    .insert(method.clone(), crates.clone());
            }
        }
    }
    
    // Sort types by crate count (most used first)
    let mut sorted_types: Vec<_> = type_total_crates.iter()
        .map(|(t, c)| (t.clone(), c.len()))
        .collect();
    sorted_types.sort_by(|a, b| b.1.cmp(&a.1));
    
    // Print the types to analyze upfront (showing crates that call methods)
    for (type_name, _) in &sorted_types {
        if let Some(type_methods) = methods_by_type.get(type_name) {
            let method_count = type_methods.len();
            if method_count > 0 {
                // Count crates that actually call methods
                let mut callers: HashSet<String> = HashSet::new();
                for crates in type_methods.values() {
                    callers.extend(crates.iter().cloned());
                }
                let caller_count = callers.len();
                if caller_count >= 10 {
                    println!("  {:50} {:>5} crates call, {:>4} methods", type_name, caller_count, method_count);
                    log!("  {:50} {:>5} crates call, {:>4} methods", type_name, caller_count, method_count);
                }
            }
        }
    }
    println!();
    log!("");
    
    let type_targets = [70.0, 80.0, 90.0, 95.0, 99.0, 100.0];
    
    for (type_name, _) in sorted_types.iter() {
        if let Some(type_methods) = methods_by_type.get(type_name) {
            if type_methods.is_empty() { continue; }
            
            // Calculate crates that actually CALL methods on this type (not just type annotations)
            let mut crates_calling_methods: HashSet<String> = HashSet::new();
            for crates in type_methods.values() {
                crates_calling_methods.extend(crates.iter().cloned());
            }
            let method_caller_count = crates_calling_methods.len();
            
            if method_caller_count < 10 { continue; } // Skip rarely-used types
            
            // Get annotation count for context
            let annotation_count = type_total_crates.get(type_name).map(|c| c.len()).unwrap_or(0);
            let annotation_only = annotation_count.saturating_sub(method_caller_count);
            
            println!("\n{}", "=".repeat(60));
            println!("TYPE: {} ({} crates call methods, {} methods)", type_name, method_caller_count, type_methods.len());
            println!("      ({} more use type in annotations only)", annotation_only);
            println!("{}", "=".repeat(60));
            log!("\n{}", "=".repeat(60));
            log!("TYPE: {} ({} crates call methods, {} methods)", type_name, method_caller_count, type_methods.len());
            log!("      ({} more use type in annotations only)", annotation_only);
            log!("{}", "=".repeat(60));
            
            for &target in &type_targets {
                let target_count = (method_caller_count as f64 * target / 100.0).ceil() as usize;
                let result = greedy_cover(type_methods, target, method_caller_count);
                let achieved = result.last().map(|(_, _, p)| *p).unwrap_or(0.0);
                
                println!("\n  --- Target: {:.0}% ({} crates) ---", target, target_count);
                log!("\n  --- Target: {:.0}% ({} crates) ---", target, target_count);
                
                // Show up to 10 methods for stdout, all in log
                for (i, (name, new_cov, cum_pct)) in result.iter().enumerate() {
                    let short_name = name.split("::").last().unwrap_or(name);
                    if i < 10 {
                        println!("    {:3}. {:40} +{:>5} ({:>8.4}%)", i + 1, short_name, new_cov, cum_pct);
                    }
                    log!("    {:3}. {:40} +{:>5} ({:>8.4}%)", i + 1, name, new_cov, cum_pct);
                }
                if result.len() > 10 {
                    println!("    ... ({} more in log)", result.len() - 10);
                }
                println!("    => {} methods achieve {:.4}%", result.len(), achieved);
                log!("    => {} methods achieve {:.4}%", result.len(), achieved);
            }
        }
    }
    
    let greedy_elapsed = greedy_start.elapsed();
    
    // =========================================================================
    // === UNCOVERED CRATES ANALYSIS ===
    // Description: Identify crates with no detected stdlib usage.
    // =========================================================================
    
    // =========================================================================
    // === SECTION 10: TRAIT IMPLEMENTATIONS ===
    // Description: Which stdlib traits are implemented for which types.
    // Critical for Verus: Iterator, Clone, Debug, IntoIterator, etc.
    // =========================================================================
    
    println!("\n\n=== 12. TRAIT IMPLEMENTATIONS AND GREEDY METHOD COVER ===");
    println!("This helps us answer: Which trait methods are actually used in practice?");
    println!("Description: Stdlib trait usage by implementing type.");
    println!("{}", "=".repeat(80));
    log!("\n\n=== 12. TRAIT IMPLEMENTATIONS AND GREEDY METHOD COVER ===");
    log!("This helps us answer: Which trait methods are actually used in practice?");
    log!("Description: Stdlib trait usage by implementing type.");
    log!("{}", "=".repeat(80));
    
    // Sort traits by total crate usage
    let mut trait_usage: Vec<(&String, usize, usize)> = trait_impl_total.iter()
        .map(|(trait_name, impls)| {
            let mut all_crates: HashSet<String> = HashSet::new();
            for crates in impls.values() {
                all_crates.extend(crates.iter().cloned());
            }
            (trait_name, all_crates.len(), impls.len())
        })
        .collect();
    trait_usage.sort_by(|a, b| b.1.cmp(&a.1));
    
    println!("\nTraits by crate count (top 30):");
    log!("\nTraits by crate count (top 30):");
    println!("{:40} {:>10} {:>10} {:>10} {:>10}", "Trait", "Uses", "Uses%", "No-use%", "Impls");
    println!("{}", "-".repeat(82));
    log!("{:40} {:>10} {:>10} {:>10} {:>10}", "Trait", "Uses", "Uses%", "No-use%", "Impls");
    log!("{}", "-".repeat(82));
    
    for (trait_name, crate_count, impl_count) in trait_usage.iter().take(30) {
        let use_pct = (*crate_count as f64 / covered_crate_count as f64) * 100.0;
        let no_use_pct = 100.0 - use_pct;
        println!("{:40} {:>10} {:>9.4}% {:>9.4}% {:>10}", trait_name, crate_count, use_pct, no_use_pct, impl_count);
        log!("{:40} {:>10} {:>9.4}% {:>9.4}% {:>10}", trait_name, crate_count, use_pct, no_use_pct, impl_count);
    }
    
    // For key traits (Iterator, Clone, IntoIterator), show detailed breakdown
    let key_traits = [
        "core::iter::Iterator", 
        "core::iter::IntoIterator", 
        "core::clone::Clone", 
        "core::fmt::Debug", 
        "core::fmt::Display", 
        "core::convert::From", 
        "core::convert::Into", 
        "core::ops::Deref",
        "std::io::Read",
        "std::io::Write",
    ];
    
    for key_trait in &key_traits {
        if let Some(impls) = trait_impl_total.get(*key_trait) {
            let mut all_crates: HashSet<String> = HashSet::new();
            for crates in impls.values() {
                all_crates.extend(crates.iter().cloned());
            }
            let total_crates = all_crates.len();
            
            if total_crates < 10 { continue; }
            
            println!("\n{}", "=".repeat(60));
            println!("TRAIT: {} ({} crates, {} type::method impls)", key_trait, total_crates, impls.len());
            println!("{}", "=".repeat(60));
            log!("\n{}", "=".repeat(60));
            log!("TRAIT: {} ({} crates, {} type::method impls)", key_trait, total_crates, impls.len());
            log!("{}", "=".repeat(60));
            
            // Sort impls by crate count (excluding __METHOD__ entries for display)
            let mut impl_list: Vec<_> = impls.iter()
                .filter(|(name, _)| !name.starts_with("__METHOD__"))
                .map(|(impl_name, crates)| (impl_name, crates.len()))
                .collect();
            impl_list.sort_by(|a, b| b.1.cmp(&a.1));
            
            println!("\nTop implementations by type:");
            log!("\nTop implementations by type:");
            for (impl_name, count) in impl_list.iter().take(20) {
                // Simplify long generic type names
                let short_name = if impl_name.len() > 50 {
                    format!("{}...", &impl_name[..47])
                } else {
                    impl_name.to_string()
                };
                println!("  {:50} {:>6} crates", short_name, count);
                log!("  {:50} {:>6} crates", impl_name, count);
            }
            if impl_list.len() > 20 {
                println!("  ... ({} more in log)", impl_list.len() - 20);
            }
            
            // Greedy cover on trait METHODS (not types)
            // Extract method-only entries and build a map: method_name -> crates
            let mut method_crates: BTreeMap<String, HashSet<String>> = BTreeMap::new();
            for (impl_name, crates) in impls.iter() {
                if impl_name.starts_with("__METHOD__::") {
                    let method_name = impl_name.strip_prefix("__METHOD__::").unwrap_or(impl_name);
                    method_crates.entry(method_name.to_string())
                        .or_default()
                        .extend(crates.iter().cloned());
                }
            }
            
            if !method_crates.is_empty() {
                println!("\nGreedy cover: {} methods to verify", method_crates.len());
                log!("\nGreedy cover: {} methods to verify", method_crates.len());
                
                let trait_targets = [70.0, 80.0, 90.0, 95.0, 99.0, 100.0];
                for &target in &trait_targets {
                    let result = greedy_cover(&method_crates, target, total_crates);
                    let achieved = result.last().map(|(_, _, p)| *p).unwrap_or(0.0);
                    
                    println!("\n  --- Target: {:.0}% ({} crates) ---", target,
                        (total_crates as f64 * target / 100.0).ceil() as usize);
                    log!("\n  --- Target: {:.0}% ({} crates) ---", target,
                        (total_crates as f64 * target / 100.0).ceil() as usize);
                    
                    for (i, (name, new_cov, cum_pct)) in result.iter().enumerate() {
                        if i < 15 {
                            println!("    {:3}. {:30} +{:>5} ({:>8.4}%)", i + 1, name, new_cov, cum_pct);
                        }
                        log!("    {:3}. {:30} +{:>5} ({:>8.4}%)", i + 1, name, new_cov, cum_pct);
                    }
                    if result.len() > 15 {
                        println!("    ... ({} more in log)", result.len() - 15);
                    }
                    println!("    => {} methods achieve {:.4}%", result.len(), achieved);
                    log!("    => {} methods achieve {:.4}%", result.len(), achieved);
                }
            }
        }
    }
    
    // =========================================================================
    // === GREEDY COVER SUMMARY ===
    // Description: Time taken for all greedy set cover computations.
    // =========================================================================
    println!("\n\n=== GREEDY COVER SUMMARY ===");
    println!("Greedy set cover analysis time: {} ms", greedy_elapsed.as_millis());
    log!("\n\n=== GREEDY COVER SUMMARY ===");
    log!("Greedy set cover analysis time: {} ms", greedy_elapsed.as_millis());
    
    let elapsed = start.elapsed();
    let end_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z");
    
    println!("\n=== 13. SUMMARY ===\n");
    println!("Projects analyzed: {}", projects_with_mir.len());
    println!("Crates analyzed: {} ({} with stdlib, {} without)", unique_crates.len(), covered_crate_count, uncovered_count);
    println!("Std Library modules used: {}", module_stats.len());
    println!("Std Library types used: {}", type_stats.len());
    println!("Std Library traits used: {}", trait_total_crates.len());
    println!("Std Library methods used: {} ({} actual methods, excluding type names)", method_stats.len(), filtered_methods.len());
    println!("\nTOTAL TIME: {} ms ({:.2} seconds)", elapsed.as_millis(), elapsed.as_secs_f64());
    println!("Ended at: {}", end_time);
    
    log!("\n=== 13. SUMMARY ===\n");
    log!("Projects analyzed: {}", projects_with_mir.len());
    log!("Crates analyzed: {} ({} with stdlib, {} without)", unique_crates.len(), covered_crate_count, uncovered_count);
    log!("Std Library modules used: {}", module_stats.len());
    log!("Std Library types used: {}", type_stats.len());
    log!("Std Library traits used: {}", trait_total_crates.len());
    log!("Std Library methods used: {} ({} actual methods, excluding type names)", method_stats.len(), filtered_methods.len());
    log!("\nTOTAL TIME: {} ms ({:.2} seconds)", elapsed.as_millis(), elapsed.as_secs_f64());
    log!("Ended at: {}", end_time);
    
    // Ensure log is flushed
    log_file.flush()?;
    
    println!("\nLog written to: analyses/rusticate-analyze-modules-mir.log");
    
    Ok(())
}

fn main() -> Result<()> {
    let start = std::time::Instant::now();
    
    // Parse args with fail-fast validation
    let args = Args::parse()?;

    // Handle -R mode separately
    if args.rust_libs {
        return count_stdlib_functions(args.jobs);
    }
    
    // Handle -M mode (MIR analysis of single crate)
    if let Some(mir_path) = args.mir_analysis {
        return analyze_mir_crate(&mir_path);
    }

    // Must have codebase for normal mode
    let codebase = args.codebase.context("Codebase required for non -R mode")?;
    
    // Handle -U mode (usage analysis with compilation)
    if args.usage_analysis {
        return analyze_usage(&codebase, args.max_codebases, args.jobs);
    }

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

