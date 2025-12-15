// Copyright (C) Brian G. Milnes 2025
//! rusticate-analyze-libs - Inventory Rust standard library (std, core, alloc)
//!
//! Creates a complete structured inventory of Rust's standard library including:
//! - Libraries (std, core, alloc)
//! - Files per library
//! - Types (struct/enum) with methods
//! - Traits with associated types
//! - Trait methods
//! - Free functions
//! - Impls (inherent and trait)
//! - Macros
//! - Constants/Statics
//! - Type aliases
//! - Blanket impls

use anyhow::{Context, Result, bail};
use ra_ap_syntax::{ast, AstNode, SyntaxKind, SyntaxNode};
use ra_ap_syntax::ast::{HasName, HasGenericParams, HasTypeBounds};
use rusticate::parse_file;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StdlibInventory {
    #[serde(rename = "$schema")]
    schema: String,
    generated: String,
    rust_version: String,
    sysroot: String,
    libraries: BTreeMap<String, LibraryInfo>,
    summary: Summary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LibraryInfo {
    path: String,
    files: Vec<FileInfo>,
    modules: Vec<ModuleInfo>,         // Module tree from lib.rs
    prelude: Option<PreludeInfo>,     // What's auto-imported
    types: Vec<TypeInfo>,
    traits: Vec<TraitInfo>,
    functions: Vec<FunctionInfo>,
    macros: Vec<MacroInfo>,
    constants: Vec<ConstantInfo>,
    type_aliases: Vec<TypeAliasInfo>,
    impls: Vec<ImplInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreludeInfo {
    module_path: String,              // e.g., "std::prelude::rust_2021"
    source_file: String,
    types: Vec<PreludeItem>,          // Types auto-imported
    traits: Vec<PreludeItem>,         // Traits auto-imported
    macros: Vec<PreludeItem>,         // Macros auto-imported
    functions: Vec<PreludeItem>,      // Functions auto-imported
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreludeItem {
    name: String,                     // Public name (e.g., "Option")
    source_path: String,              // Where it comes from (e.g., "core::option::Option")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileInfo {
    path: String,
    module: String,
    line_count: usize,
    type_count: usize,
    trait_count: usize,
    function_count: usize,
    impl_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TypeInfo {
    name: String,
    qualified_path: String,
    kind: String, // "struct" or "enum"
    is_generic: bool,
    is_unsafe: bool,
    derives: Vec<String>,
    methods: Vec<MethodInfo>,
    source_file: String,
    source_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MethodInfo {
    name: String,
    is_generic: bool,
    is_unsafe: bool,
    can_panic: bool,
    must_use: bool,
    is_const: bool,
    takes_self: String, // "self", "&self", "&mut self", "none" (associated)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TraitInfo {
    name: String,
    qualified_path: String,
    is_unsafe: bool,
    is_auto: bool,
    supertraits: Vec<String>,
    associated_types: Vec<String>,
    associated_consts: Vec<String>,
    methods: Vec<TraitMethodInfo>,
    source_file: String,
    source_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TraitMethodInfo {
    name: String,
    is_generic: bool,
    is_unsafe: bool,
    has_default: bool,
    can_panic: bool,
    must_use: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionInfo {
    name: String,
    qualified_path: String,
    is_generic: bool,
    is_unsafe: bool,
    can_panic: bool,
    must_use: bool,
    is_const: bool,
    source_file: String,
    source_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MacroInfo {
    name: String,
    qualified_path: String,
    kind: String, // "declarative" or "procedural"
    is_exported: bool,
    source_file: String,
    source_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConstantInfo {
    name: String,
    qualified_path: String,
    const_type: String,
    value: Option<String>,
    source_file: String,
    source_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TypeAliasInfo {
    name: String,
    qualified_path: String,
    target: String,
    source_file: String,
    source_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImplInfo {
    impl_type: String,        // The type being impl'd
    trait_name: Option<String>, // None for inherent impl
    is_unsafe: bool,
    is_blanket: bool,         // Generic impl applying to multiple types
    is_forwarding: bool,      // Blanket impl for wrapper types (&T, Box<T>, etc.)
    is_bridge: bool,          // Blanket impl that gives trait B if you have trait A
    where_clause: Option<String>,
    methods: Vec<String>,     // Method names
    source_file: String,
    source_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModuleInfo {
    name: String,                    // Module name (e.g., "option")
    path: String,                    // Full path (e.g., "core::option")
    is_public: bool,                 // pub mod vs mod
    source_file: String,             // File that defines this module
    child_modules: Vec<String>,      // Names of child modules
    re_exports: Vec<ReExportInfo>,   // pub use statements
    items: ModuleItems,              // Items defined directly in this module
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ModuleItems {
    types: Vec<String>,              // Type names defined here
    traits: Vec<String>,             // Trait names defined here
    functions: Vec<String>,          // Function names defined here
    macros: Vec<String>,             // Macro names defined here
    constants: Vec<String>,          // Constant names defined here
    type_aliases: Vec<String>,       // Type alias names defined here
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReExportInfo {
    name: String,                    // Public name (e.g., "Option")
    source_path: String,             // Where it comes from (e.g., "core::option::Option")
    kind: String,                    // "type", "trait", "function", "macro", "module", "all"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Summary {
    total_libraries: usize,
    total_files: usize,
    total_modules: usize,
    total_public_modules: usize,
    total_re_exports: usize,
    total_prelude_items: usize,
    total_types: usize,
    total_traits: usize,
    total_type_methods: usize,
    total_trait_methods: usize,
    total_functions: usize,
    total_macros: usize,
    total_constants: usize,
    total_type_aliases: usize,
    total_impls: usize,
    total_blanket_impls: usize,
    total_forwarding_impls: usize,
    total_bridge_impls: usize,
}

// ============================================================================
// Args
// ============================================================================

struct Args {
    output_json: Option<PathBuf>,
    jobs: usize,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args_iter = std::env::args().skip(1);
        let mut output_json = None;
        let mut jobs = 4;

        while let Some(arg) = args_iter.next() {
            match arg.as_str() {
                "-o" | "--output" => {
                    output_json = Some(PathBuf::from(
                        args_iter
                            .next()
                            .context("Expected path after -o/--output")?
                    ));
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

        Ok(Args { output_json, jobs })
    }
}

fn print_help() {
    println!(
        r#"rusticate-analyze-libs - Inventory Rust standard library

USAGE:
    rusticate-analyze-libs [-o <PATH>] [-j <N>]

OPTIONS:
    -o, --output <PATH>     Output JSON inventory to PATH (default: analyses/rusticate-analyze-libs.json)
    -j, --jobs <N>          Number of parallel threads (default: 4)
    -h, --help              Print this help message

DESCRIPTION:
    Creates a complete inventory of Rust's standard library (std, core, alloc).
    
    Output includes:
    - A) Libraries: std, core, alloc with module counts
    - B) Files: All .rs source files per library
    - C) Types: struct/enum with methods, generics, derives
    - D) Traits: with associated types, supertraits
    - E) Trait Methods: generic, unsafe, has_default
    - F) Free Functions: top-level functions
    - G) Impls: inherent and trait implementations
    - H) Macros: declarative and procedural
    - I) Constants/Statics: with types and values
    - J) Type Aliases: with targets
    - K) Blanket Impls: automatic trait implementations

OUTPUT:
    - analyses/rusticate-analyze-libs.json (machine-readable)
    - analyses/rusticate-analyze-libs.log (human-readable)

EXAMPLES:
    # Generate inventory with defaults
    rusticate-analyze-libs

    # Generate with custom output path and 8 threads
    rusticate-analyze-libs -o /tmp/stdlib.json -j 8
"#
    );
}

// ============================================================================
// Main
// ============================================================================

fn main() -> Result<()> {
    let start = std::time::Instant::now();
    let args = Args::parse()?;

    // Set up logging
    let log_path = PathBuf::from("analyses/rusticate-analyze-libs.log");
    let json_path = args.output_json.unwrap_or_else(|| PathBuf::from("analyses/rusticate-analyze-libs.json"));
    fs::create_dir_all("analyses")?;
    let mut log_file = fs::File::create(&log_path)
        .context("Failed to create log file")?;

    macro_rules! log {
        ($($arg:tt)*) => {{
            writeln!(log_file, $($arg)*).ok();
            println!($($arg)*);
        }};
    }


    // Header
    log!("rusticate-analyze-libs");
    log!("======================");
    log!("Command: {}", std::env::args().collect::<Vec<_>>().join(" "));
    log!("Jobs: {}", args.jobs);
    
    use chrono::Local;
    let datetime = Local::now();
    let datetime_str = datetime.format("%Y-%m-%d %H:%M:%S %Z").to_string();
    log!("Started at: {}", datetime_str);
    log!("");

    // Find stdlib
    log!("Finding Rust stdlib source...");
    let (stdlib_path, rust_version, sysroot) = find_rust_stdlib()?;
    log!("Sysroot: {}", sysroot);
    log!("Stdlib path: {}", stdlib_path.display());
    log!("Rust version: {}", rust_version);
    log!("");

    // Libraries to analyze (known ahead of time)
    let lib_names = ["core", "alloc", "std"];
    
    // ========================================================================
    // TABLE OF CONTENTS
    // ========================================================================
    log!("=== TABLE OF CONTENTS ===");
    log!("");
    log!("1. INTRODUCTION");
    log!("2. ANALYSIS PROGRESS");
    log!("3. SUMMARY");
    for (i, lib_name) in lib_names.iter().enumerate() {
        let section = i + 4;
        log!("{}. {} LIBRARY", section, lib_name.to_uppercase());
        log!("   {}.1 Modules", section);
        log!("   {}.2 Prelude", section);
        log!("   {}.3 Types", section);
        log!("   {}.4 Traits", section);
        log!("   {}.5 Free Functions", section);
        log!("   {}.6 Macros", section);
        log!("   {}.7 Constants", section);
        log!("   {}.8 Type Aliases", section);
        log!("   {}.9 Impls", section);
    }
    log!("");
    
    // ========================================================================
    // 1. INTRODUCTION
    // ========================================================================
    log!("=== 1. INTRODUCTION ===");
    log!("");
    log!("This inventory catalogs Rust's standard library (core, alloc, std).");
    log!("Generated by rusticate-analyze-libs using AST parsing of stdlib source.");
    log!("");
    log!("This helps us answer:");
    log!("  Q1. What modules exist in stdlib? How are they organized? -> Sections X.1");
    log!("  Q2. What's in the prelude (auto-imported into every program)? -> Sections X.2");
    log!("  Q3. What types (structs/enums) does stdlib provide? -> Sections X.3");
    log!("  Q4. What traits are defined? What methods do they have? -> Sections X.4");
    log!("  Q5. What free functions are available? -> Sections X.5");
    log!("  Q6. What macros are exported (vec!, format!, etc.)? -> Sections X.6");
    log!("  Q7. What constants are defined (MAX, MIN, PI, etc.)? -> Sections X.7");
    log!("  Q8. What type aliases exist (io::Result, etc.)? -> Sections X.8");
    log!("  Q9. What impls exist? Which are blanket/forwarding/bridge? -> Sections X.9");
    log!("  Q10. How do std, core, and alloc relate via re-exports? -> Sections X.1 re-exports");
    log!("");
    log!("Definitions of some non-standard Rust terms:");
    log!("  Blanket impl: An impl with generic params applying to multiple types.");
    log!("                Example: impl<T: Clone> Clone for Vec<T>");
    log!("  Forwarding impl: A blanket impl for wrapper types (&T, Box<T>, Arc<T>, etc.)");
    log!("                   that propagates a trait through the wrapper.");
    log!("                   Example: impl<I: Iterator> Iterator for &mut I");
    log!("  Bridge impl: A blanket impl where having trait A gives you trait B for free.");
    log!("               The impl type is just T (bare type param), not wrapped.");
    log!("               Example: impl<T: Display> ToString for T");
    log!("");
    
    // ========================================================================
    // 2. ANALYSIS PROGRESS
    // ========================================================================
    log!("=== 2. ANALYSIS PROGRESS ===");
    log!("");

    // Libraries to analyze
    let libs = vec![
        ("core", stdlib_path.join("core")),
        ("alloc", stdlib_path.join("alloc")),
        ("std", stdlib_path.join("std")),
    ];

    let mut inventory = StdlibInventory {
        schema: "https://github.com/rusticate/schemas/rusticate-analyze-libs.schema.json".to_string(),
        generated: datetime_str,
        rust_version: rust_version.clone(),
        sysroot,
        libraries: BTreeMap::new(),
        summary: Summary::default(),
    };

    // Analyze each library
    for (lib_name, lib_path) in &libs {
        if !lib_path.exists() {
            log!("Warning: {} not found at {}", lib_name, lib_path.display());
            continue;
        }

        log!("=== Analyzing {} ===", lib_name.to_uppercase());
        log!("Path: {}", lib_path.display());

        let lib_info = analyze_library(lib_name, lib_path, args.jobs, &mut log_file)?;
        
        // Update summary
        inventory.summary.total_libraries += 1;
        inventory.summary.total_files += lib_info.files.len();
        inventory.summary.total_modules += lib_info.modules.len();
        inventory.summary.total_public_modules += lib_info.modules.iter().filter(|m| m.is_public).count();
        inventory.summary.total_re_exports += lib_info.modules.iter().map(|m| m.re_exports.len()).sum::<usize>();
        if let Some(ref prelude) = lib_info.prelude {
            inventory.summary.total_prelude_items += prelude.types.len() + prelude.traits.len() + prelude.macros.len() + prelude.functions.len();
        }
        inventory.summary.total_types += lib_info.types.len();
        inventory.summary.total_traits += lib_info.traits.len();
        inventory.summary.total_type_methods += lib_info.types.iter().map(|t| t.methods.len()).sum::<usize>();
        inventory.summary.total_trait_methods += lib_info.traits.iter().map(|t| t.methods.len()).sum::<usize>();
        inventory.summary.total_functions += lib_info.functions.len();
        inventory.summary.total_macros += lib_info.macros.len();
        inventory.summary.total_constants += lib_info.constants.len();
        inventory.summary.total_type_aliases += lib_info.type_aliases.len();
        inventory.summary.total_impls += lib_info.impls.len();
        inventory.summary.total_blanket_impls += lib_info.impls.iter().filter(|i| i.is_blanket).count();
        inventory.summary.total_forwarding_impls += lib_info.impls.iter().filter(|i| i.is_forwarding).count();
        inventory.summary.total_bridge_impls += lib_info.impls.iter().filter(|i| i.is_bridge).count();

        log!("  Files: {}", lib_info.files.len());
        log!("  Modules: {} ({} public, {} re-exports)", 
             lib_info.modules.len(),
             lib_info.modules.iter().filter(|m| m.is_public).count(),
             lib_info.modules.iter().map(|m| m.re_exports.len()).sum::<usize>());
        if let Some(ref prelude) = lib_info.prelude {
            log!("  Prelude re-exports: {} types, {} traits, {} macros, {} functions",
                 prelude.types.len(), prelude.traits.len(), prelude.macros.len(), prelude.functions.len());
        }
        log!("  Types: {} ({} methods)", lib_info.types.len(), 
             lib_info.types.iter().map(|t| t.methods.len()).sum::<usize>());
        log!("  Traits: {} ({} methods)", lib_info.traits.len(),
             lib_info.traits.iter().map(|t| t.methods.len()).sum::<usize>());
        log!("  Functions: {}", lib_info.functions.len());
        log!("  Macros: {}", lib_info.macros.len());
        log!("  Constants: {}", lib_info.constants.len());
        log!("  Type aliases: {}", lib_info.type_aliases.len());
        let blanket_count = lib_info.impls.iter().filter(|i| i.is_blanket).count();
        let forwarding_count = lib_info.impls.iter().filter(|i| i.is_forwarding).count();
        let bridge_count = lib_info.impls.iter().filter(|i| i.is_bridge).count();
        log!("  Impls: {} ({} blanket: {} forwarding, {} bridge)", 
             lib_info.impls.len(), blanket_count, forwarding_count, bridge_count);
        log!("");

        inventory.libraries.insert(lib_name.to_string(), lib_info);
    }
    
    // Second pass: resolve cross-library re-exports
    // Build global index of all trait/type/function/macro names
    let mut global_traits: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut global_types: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut global_functions: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut global_macros: std::collections::HashSet<String> = std::collections::HashSet::new();
    
    for lib in inventory.libraries.values() {
        for t in &lib.traits {
            global_traits.insert(t.name.clone());
        }
        for t in &lib.types {
            global_types.insert(t.name.clone());
        }
        for f in &lib.functions {
            global_functions.insert(f.name.clone());
        }
        for m in &lib.macros {
            global_macros.insert(m.name.clone());
        }
    }
    
    // Now resolve any remaining "type/trait" re-exports
    for lib in inventory.libraries.values_mut() {
        for module in &mut lib.modules {
            for re_export in &mut module.re_exports {
                if re_export.kind == "type/trait" {
                    if global_traits.contains(&re_export.name) && !global_types.contains(&re_export.name) {
                        re_export.kind = "trait".to_string();
                    } else if global_types.contains(&re_export.name) && !global_traits.contains(&re_export.name) {
                        re_export.kind = "type".to_string();
                    }
                    // If in both or neither, leave as "type/trait"
                }
            }
        }
    }

    // Write human-readable detailed report
    log!("");
    log!("=== 3. SUMMARY ===");
    log!("");
    log!("Libraries: {}", inventory.summary.total_libraries);
    log!("Files: {}", inventory.summary.total_files);
    log!("Modules: {}", inventory.summary.total_modules);
    log!("  Public modules: {}", inventory.summary.total_public_modules);
    log!("  Re-exports: {}", inventory.summary.total_re_exports);
    log!("Prelude items: {}", inventory.summary.total_prelude_items);
    log!("Types: {}", inventory.summary.total_types);
    log!("  Type methods: {}", inventory.summary.total_type_methods);
    log!("Traits: {}", inventory.summary.total_traits);
    log!("  Trait methods: {}", inventory.summary.total_trait_methods);
    log!("Functions: {}", inventory.summary.total_functions);
    log!("Macros: {}", inventory.summary.total_macros);
    log!("Constants: {}", inventory.summary.total_constants);
    log!("Type aliases: {}", inventory.summary.total_type_aliases);
    log!("Impls: {}", inventory.summary.total_impls);
    log!("  Blanket impls: {}", inventory.summary.total_blanket_impls);
    log!("    Forwarding impls: {}", inventory.summary.total_forwarding_impls);
    log!("    Bridge impls: {}", inventory.summary.total_bridge_impls);
    log!("");

    // Write detailed sections to log only
    write_detailed_report(&inventory, &mut log_file)?;

    // Write JSON
    let json = serde_json::to_string_pretty(&inventory)?;
    
    // Validate against schema
    let schema_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("schemas/rusticate-analyze-libs.schema.json");
    if schema_path.exists() {
        let schema_str = fs::read_to_string(&schema_path)?;
        let schema: serde_json::Value = serde_json::from_str(&schema_str)?;
        let json_value: serde_json::Value = serde_json::from_str(&json)?;
        
        let validator = jsonschema::validator_for(&schema)
            .context("Failed to compile JSON schema")?;
        
        let errors: Vec<_> = validator.iter_errors(&json_value).collect();
        if errors.is_empty() {
            log!("✓ JSON validates against schema");
        } else {
            log!("✗ JSON schema validation failed:");
            for error in &errors {
                log!("  - {}: {}", error.instance_path(), error);
            }
            bail!("JSON schema validation failed with {} errors", errors.len());
        }
    } else {
        log!("⚠ Schema not found at {}, skipping validation", schema_path.display());
    }
    
    fs::write(&json_path, &json)?;

    let elapsed = start.elapsed();
    log!("");
    log!("Completed in {} ms.", elapsed.as_millis());
    log!("JSON output: {}", json_path.display());
    log!("Log output: {}", log_path.display());

    log_file.flush()?;

    Ok(())
}

// ============================================================================
// Module Tree Building
// ============================================================================

/// Build the module tree starting from lib.rs
fn build_module_tree(lib_name: &str, lib_path: &Path) -> Result<Vec<ModuleInfo>> {
    let mut modules = Vec::new();
    
    // Find lib.rs or mod.rs at root
    let lib_rs = lib_path.join("src/lib.rs");
    if !lib_rs.exists() {
        // Some stdlib crates have different structure
        return Ok(modules);
    }
    
    // Parse the root module
    let root_module = parse_module_file(&lib_rs, lib_name, lib_name, lib_path)?;
    
    // Recursively collect all modules
    collect_modules_recursive(&root_module, &mut modules, lib_name, lib_path)?;
    
    // Sort by path for consistent output
    modules.sort_by(|a, b| a.path.cmp(&b.path));
    
    Ok(modules)
}

/// Recursively collect modules from the tree
fn collect_modules_recursive(
    module: &ModuleInfo,
    all_modules: &mut Vec<ModuleInfo>,
    lib_name: &str,
    lib_path: &Path,
) -> Result<()> {
    all_modules.push(module.clone());
    
    // Process child modules
    for child_name in &module.child_modules {
        // Find the child module file
        let child_path = find_module_file(lib_path, &module.path, child_name);
        
        if let Some(child_file) = child_path {
            let child_module_path = format!("{}::{}", module.path, child_name);
            if let Ok(child_module) = parse_module_file(&child_file, &child_module_path, lib_name, lib_path) {
                collect_modules_recursive(&child_module, all_modules, lib_name, lib_path)?;
            }
        }
    }
    
    Ok(())
}

/// Find the file for a module (handles both foo.rs and foo/mod.rs)
fn find_module_file(lib_path: &Path, parent_path: &str, module_name: &str) -> Option<PathBuf> {
    // Convert module path to file path
    // e.g., "core::option" -> "src/option.rs" or "src/option/mod.rs"
    let path_parts: Vec<&str> = parent_path.split("::").skip(1).collect(); // skip lib name
    
    let mut dir = lib_path.join("src");
    for part in &path_parts {
        dir = dir.join(part);
    }
    
    // Try module_name.rs first
    let as_file = dir.join(format!("{}.rs", module_name));
    if as_file.exists() {
        return Some(as_file);
    }
    
    // Try module_name/mod.rs
    let as_dir = dir.join(module_name).join("mod.rs");
    if as_dir.exists() {
        return Some(as_dir);
    }
    
    // Also check without the src prefix for some structures
    let mut dir2 = lib_path.to_path_buf();
    for part in &path_parts {
        dir2 = dir2.join(part);
    }
    
    let as_file2 = dir2.join(format!("{}.rs", module_name));
    if as_file2.exists() {
        return Some(as_file2);
    }
    
    let as_dir2 = dir2.join(module_name).join("mod.rs");
    if as_dir2.exists() {
        return Some(as_dir2);
    }
    
    None
}

/// Parse a module file to extract module info
fn parse_module_file(
    file_path: &Path,
    module_path: &str,
    lib_name: &str,
    lib_root: &Path,
) -> Result<ModuleInfo> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read: {}", file_path.display()))?;
    
    let parse = match parse_file(&content) {
        Ok(p) => p,
        Err(_) => {
            // Return minimal info for unparseable files
            let rel_path = file_path.strip_prefix(lib_root).unwrap_or(file_path);
            return Ok(ModuleInfo {
                name: module_path.rsplit("::").next().unwrap_or(module_path).to_string(),
                path: module_path.to_string(),
                is_public: true, // assume lib root is public
                source_file: rel_path.display().to_string(),
                child_modules: vec![],
                re_exports: vec![],
                items: ModuleItems::default(),
            });
        }
    };
    
    let root = parse.syntax();
    let rel_path = file_path.strip_prefix(lib_root).unwrap_or(file_path);
    
    let mut child_modules = Vec::new();
    let mut re_exports = Vec::new();
    let mut items = ModuleItems::default();
    
    // Walk top-level items
    for item in root.children() {
        match item.kind() {
            // mod declarations
            SyntaxKind::MODULE => {
                if let Some(mod_info) = extract_mod_declaration(&item) {
                    child_modules.push(mod_info);
                }
            }
            // pub use re-exports
            SyntaxKind::USE => {
                if is_public(&item) {
                    if let Some(exports) = extract_use_reexports(&item, lib_name) {
                        re_exports.extend(exports);
                    }
                }
            }
            // Track items defined in this module
            SyntaxKind::STRUCT => {
                if is_public(&item) {
                    if let Some(name) = extract_item_name(&item) {
                        items.types.push(name);
                    }
                }
            }
            SyntaxKind::ENUM => {
                if is_public(&item) {
                    if let Some(name) = extract_item_name(&item) {
                        items.types.push(name);
                    }
                }
            }
            SyntaxKind::TRAIT => {
                if is_public(&item) {
                    if let Some(name) = extract_item_name(&item) {
                        items.traits.push(name);
                    }
                }
            }
            SyntaxKind::FN => {
                if is_public(&item) && !is_in_impl_or_trait(&item) {
                    if let Some(name) = extract_item_name(&item) {
                        items.functions.push(name);
                    }
                }
            }
            SyntaxKind::MACRO_RULES => {
                if let Some(name) = extract_item_name(&item) {
                    items.macros.push(name);
                }
            }
            SyntaxKind::CONST => {
                if is_public(&item) {
                    if let Some(name) = extract_item_name(&item) {
                        items.constants.push(name);
                    }
                }
            }
            SyntaxKind::TYPE_ALIAS => {
                if is_public(&item) {
                    if let Some(name) = extract_item_name(&item) {
                        items.type_aliases.push(name);
                    }
                }
            }
            _ => {}
        }
    }
    
    Ok(ModuleInfo {
        name: module_path.rsplit("::").next().unwrap_or(module_path).to_string(),
        path: module_path.to_string(),
        is_public: true, // Will be refined by parent
        source_file: rel_path.display().to_string(),
        child_modules,
        re_exports,
        items,
    })
}

/// Extract module name from a `mod foo;` declaration
fn extract_mod_declaration(node: &SyntaxNode) -> Option<String> {
    // Look for NAME token in the module declaration
    for child in node.children_with_tokens() {
        if child.kind() == SyntaxKind::NAME {
            return Some(child.to_string());
        }
    }
    None
}

/// Extract re-exports from a `pub use` statement
fn extract_use_reexports(node: &SyntaxNode, _lib_name: &str) -> Option<Vec<ReExportInfo>> {
    let use_node = ast::Use::cast(node.clone())?;
    let use_tree = use_node.use_tree()?;
    
    let mut exports = Vec::new();
    extract_use_tree_exports(&use_tree, "", &mut exports);
    
    if exports.is_empty() {
        None
    } else {
        Some(exports)
    }
}

/// Recursively extract exports from a use tree
fn extract_use_tree_exports(tree: &ast::UseTree, prefix: &str, exports: &mut Vec<ReExportInfo>) {
    // Get the path part
    let path_str = tree.path()
        .map(|p| p.syntax().text().to_string())
        .unwrap_or_default();
    
    let full_path = if prefix.is_empty() {
        path_str.clone()
    } else if path_str.is_empty() {
        prefix.to_string()
    } else {
        format!("{}::{}", prefix, path_str)
    };
    
    // Check for rename (as)
    let rename = tree.rename().map(|r| {
        r.name().map(|n| n.text().to_string()).unwrap_or_default()
    });
    
    // Check for nested use tree list
    if let Some(tree_list) = tree.use_tree_list() {
        for subtree in tree_list.use_trees() {
            extract_use_tree_exports(&subtree, &full_path, exports);
        }
    } else if tree.star_token().is_some() {
        // glob import: pub use foo::*
        exports.push(ReExportInfo {
            name: "*".to_string(),
            source_path: full_path,
            kind: "all".to_string(),
        });
    } else {
        // Single item
        let name = rename.unwrap_or_else(|| {
            full_path.rsplit("::").next().unwrap_or(&full_path).to_string()
        });
        
        // Determine kind heuristically from syntax alone
        // - Uppercase first char = type or trait (can't distinguish from pub use alone)
        // - Known module patterns (core::X, alloc::X, std::X where X is lowercase) = module
        // - Has underscore or starts lowercase = likely function or module
        let kind = if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            "type/trait".to_string() // Can't distinguish types from traits in pub use
        } else {
            // Check if this looks like a module re-export
            // Patterns like "core::option", "alloc::vec" are module re-exports
            let parts: Vec<&str> = full_path.split("::").collect();
            if parts.len() == 2 && 
               (parts[0] == "core" || parts[0] == "alloc" || parts[0] == "alloc_crate" || parts[0] == "std" || parts[0] == "self") {
                "module".to_string()
            } else {
                "function".to_string()
            }
        };
        
        if !full_path.is_empty() {
            exports.push(ReExportInfo {
                name,
                source_path: full_path,
                kind,
            });
        }
    }
}

/// Extract item name using HasName trait
fn extract_item_name(node: &SyntaxNode) -> Option<String> {
    for child in node.children_with_tokens() {
        if child.kind() == SyntaxKind::NAME {
            return Some(child.to_string());
        }
    }
    None
}

// ============================================================================
// Prelude Parsing
// ============================================================================

/// Parse the prelude module to extract what's auto-imported
fn parse_prelude(lib_name: &str, lib_path: &Path) -> Option<PreludeInfo> {
    // Look for prelude module - different editions have different preludes
    // std/prelude/mod.rs re-exports from rust_2021, rust_2024, etc.
    let prelude_paths = [
        lib_path.join("src/prelude/rust_2021.rs"),
        lib_path.join("src/prelude/rust_2024.rs"),
        lib_path.join("src/prelude/v1.rs"),
        lib_path.join("src/prelude/mod.rs"),
    ];
    
    for prelude_path in &prelude_paths {
        if prelude_path.exists() {
            if let Ok(info) = parse_prelude_file(lib_name, prelude_path, lib_path) {
                if !info.types.is_empty() || !info.traits.is_empty() {
                    return Some(info);
                }
            }
        }
    }
    
    None
}

fn parse_prelude_file(lib_name: &str, file_path: &Path, lib_root: &Path) -> Result<PreludeInfo> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read prelude: {}", file_path.display()))?;
    
    let parse = parse_file(&content)?;
    let root = parse.syntax();
    let rel_path = file_path.strip_prefix(lib_root).unwrap_or(file_path);
    
    // Derive module path from file path
    let module_path = format!("{}::prelude::{}", lib_name, 
        file_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("v1"));
    
    let mut types = Vec::new();
    let mut traits = Vec::new();
    let mut macros = Vec::new();
    let mut functions = Vec::new();
    
    // Walk top-level items looking for pub use statements
    for item in root.children() {
        if item.kind() == SyntaxKind::USE && is_public(&item) {
            if let Some(use_node) = ast::Use::cast(item.clone()) {
                if let Some(use_tree) = use_node.use_tree() {
                    extract_prelude_items(&use_tree, "", &mut types, &mut traits, &mut macros, &mut functions);
                }
            }
        }
    }
    
    Ok(PreludeInfo {
        module_path,
        source_file: rel_path.display().to_string(),
        types,
        traits,
        macros,
        functions,
    })
}

fn extract_prelude_items(
    tree: &ast::UseTree,
    prefix: &str,
    types: &mut Vec<PreludeItem>,
    traits: &mut Vec<PreludeItem>,
    macros: &mut Vec<PreludeItem>,
    functions: &mut Vec<PreludeItem>,
) {
    let path_str = tree.path()
        .map(|p| p.syntax().text().to_string())
        .unwrap_or_default();
    
    let full_path = if prefix.is_empty() {
        path_str.clone()
    } else if path_str.is_empty() {
        prefix.to_string()
    } else {
        format!("{}::{}", prefix, path_str)
    };
    
    // Check for rename
    let rename = tree.rename().and_then(|r| r.name().map(|n| n.text().to_string()));
    
    // Check for nested use tree list
    if let Some(tree_list) = tree.use_tree_list() {
        for subtree in tree_list.use_trees() {
            extract_prelude_items(&subtree, &full_path, types, traits, macros, functions);
        }
    } else if tree.star_token().is_none() {
        // Single item (not glob)
        let name = rename.unwrap_or_else(|| {
            full_path.rsplit("::").next().unwrap_or(&full_path).to_string()
        });
        
        if !name.is_empty() && !full_path.is_empty() {
            let item = PreludeItem {
                name: name.clone(),
                source_path: full_path.clone(),
            };
            
            // Categorize based on naming conventions and known items
            if is_known_trait(&name) || (name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) 
                && (full_path.contains("::ops::") || full_path.contains("::marker::") 
                    || full_path.contains("::clone::") || full_path.contains("::cmp::") 
                    || full_path.contains("::default::") || full_path.contains("::convert::")
                    || full_path.contains("::iter::") || full_path.contains("::fmt::"))) {
                traits.push(item);
            } else if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                // Uppercase = type or enum variant
                types.push(item);
            } else if name.ends_with('!') || is_known_macro(&name) {
                macros.push(item);
            } else {
                functions.push(item);
            }
        }
    }
}

fn is_known_trait(name: &str) -> bool {
    matches!(name, 
        "Clone" | "Copy" | "Send" | "Sync" | "Sized" | "Unpin" |
        "Drop" | "Default" | "Eq" | "PartialEq" | "Ord" | "PartialOrd" |
        "Iterator" | "IntoIterator" | "ExactSizeIterator" | "DoubleEndedIterator" |
        "Extend" | "FromIterator" |
        "AsRef" | "AsMut" | "From" | "Into" | "TryFrom" | "TryInto" |
        "ToOwned" | "ToString" | "Borrow" | "BorrowMut" |
        "Fn" | "FnMut" | "FnOnce" |
        "Add" | "Sub" | "Mul" | "Div" | "Rem" | "Neg" |
        "Deref" | "DerefMut" | "Index" | "IndexMut" |
        "Debug" | "Display" | "Write" |
        "Future" | "IntoFuture"
    )
}

fn is_known_macro(name: &str) -> bool {
    matches!(name,
        "vec" | "format" | "print" | "println" | "eprint" | "eprintln" |
        "write" | "writeln" | "panic" | "assert" | "assert_eq" | "assert_ne" |
        "debug_assert" | "debug_assert_eq" | "debug_assert_ne" |
        "todo" | "unimplemented" | "unreachable" |
        "cfg" | "compile_error" | "concat" | "env" | "include" |
        "matches" | "format_args"
    )
}

// ============================================================================
// Library Analysis
// ============================================================================

fn analyze_library(lib_name: &str, lib_path: &Path, jobs: usize, _log_file: &mut fs::File) -> Result<LibraryInfo> {
    let files = find_rust_files(lib_path);
    
    // Process files (could parallelize with rayon for large libraries)
    let chunk_size = (files.len() + jobs - 1) / jobs.max(1);
    let chunks: Vec<_> = files.chunks(chunk_size.max(1)).map(|c| c.to_vec()).collect();
    
    let lib_path_clone = lib_path.to_path_buf();
    let lib_name_clone = lib_name.to_string();
    
    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            let lib_root = lib_path_clone.clone();
            let lib_name = lib_name_clone.clone();
            std::thread::spawn(move || {
                let mut file_infos = Vec::new();
                let mut types = Vec::new();
                let mut traits = Vec::new();
                let mut functions = Vec::new();
                let mut macros = Vec::new();
                let mut constants = Vec::new();
                let mut type_aliases = Vec::new();
                let mut impls = Vec::new();
                
                for file in chunk {
                    if let Ok(result) = analyze_file(&file, &lib_root, &lib_name) {
                        file_infos.push(result.0);
                        types.extend(result.1);
                        traits.extend(result.2);
                        functions.extend(result.3);
                        macros.extend(result.4);
                        constants.extend(result.5);
                        type_aliases.extend(result.6);
                        impls.extend(result.7);
                    }
                }
                
                (file_infos, types, traits, functions, macros, constants, type_aliases, impls)
            })
        })
        .collect();
    
    // Merge results
    let mut all_files = Vec::new();
    let mut all_types = Vec::new();
    let mut all_traits = Vec::new();
    let mut all_functions = Vec::new();
    let mut all_macros = Vec::new();
    let mut all_constants = Vec::new();
    let mut all_type_aliases = Vec::new();
    let mut all_impls = Vec::new();
    
    for handle in handles {
        let (files, types, traits, functions, macros, constants, type_aliases, impls) = handle.join().unwrap();
        all_files.extend(files);
        all_types.extend(types);
        all_traits.extend(traits);
        all_functions.extend(functions);
        all_macros.extend(macros);
        all_constants.extend(constants);
        all_type_aliases.extend(type_aliases);
        all_impls.extend(impls);
    }
    
    // Sort for consistent output
    all_types.sort_by(|a, b| a.qualified_path.cmp(&b.qualified_path));
    all_traits.sort_by(|a, b| a.qualified_path.cmp(&b.qualified_path));
    all_functions.sort_by(|a, b| a.qualified_path.cmp(&b.qualified_path));
    all_macros.sort_by(|a, b| a.qualified_path.cmp(&b.qualified_path));
    all_constants.sort_by(|a, b| a.qualified_path.cmp(&b.qualified_path));
    all_type_aliases.sort_by(|a, b| a.qualified_path.cmp(&b.qualified_path));
    all_impls.sort_by(|a, b| a.impl_type.cmp(&b.impl_type));
    
    // Build module tree from lib.rs
    let mut modules = build_module_tree(lib_name, lib_path)?;
    
    // Post-process re-exports: resolve "type/trait" to actual kind
    // by looking up names in our collected traits and types
    let trait_names: std::collections::HashSet<_> = all_traits.iter()
        .map(|t| t.name.clone())
        .collect();
    let type_names: std::collections::HashSet<_> = all_types.iter()
        .map(|t| t.name.clone())
        .collect();
    let macro_names: std::collections::HashSet<_> = all_macros.iter()
        .map(|m| m.name.clone())
        .collect();
    let function_names: std::collections::HashSet<_> = all_functions.iter()
        .map(|f| f.name.clone())
        .collect();
    
    for module in &mut modules {
        for re_export in &mut module.re_exports {
            if re_export.kind == "type/trait" {
                // Try to resolve the actual kind
                if trait_names.contains(&re_export.name) {
                    re_export.kind = "trait".to_string();
                } else if type_names.contains(&re_export.name) {
                    re_export.kind = "type".to_string();
                } else if macro_names.contains(&re_export.name) {
                    re_export.kind = "macro".to_string();
                } else if function_names.contains(&re_export.name) {
                    re_export.kind = "function".to_string();
                }
                // If still not found, it might be re-exported from another crate
                // Leave as "type/trait" for honesty
            }
        }
    }
    
    // Parse prelude (if this library has one)
    let prelude = parse_prelude(lib_name, lib_path);
    
    Ok(LibraryInfo {
        path: lib_path.display().to_string(),
        files: all_files,
        modules,
        prelude,
        types: all_types,
        traits: all_traits,
        functions: all_functions,
        macros: all_macros,
        constants: all_constants,
        type_aliases: all_type_aliases,
        impls: all_impls,
    })
}

type FileAnalysisResult = (
    FileInfo,
    Vec<TypeInfo>,
    Vec<TraitInfo>,
    Vec<FunctionInfo>,
    Vec<MacroInfo>,
    Vec<ConstantInfo>,
    Vec<TypeAliasInfo>,
    Vec<ImplInfo>,
);

fn analyze_file(file: &Path, lib_root: &Path, lib_name: &str) -> Result<FileAnalysisResult> {
    let content = fs::read_to_string(file)
        .with_context(|| format!("Failed to read: {}", file.display()))?;
    
    let parse = match parse_file(&content) {
        Ok(p) => p,
        Err(_) => {
            // Return empty results for unparseable files
            let rel_path = file.strip_prefix(lib_root).unwrap_or(file);
            return Ok((
                FileInfo {
                    path: rel_path.display().to_string(),
                    module: file_to_module_path(file, lib_root),
                    line_count: content.lines().count(),
                    type_count: 0,
                    trait_count: 0,
                    function_count: 0,
                    impl_count: 0,
                },
                vec![], vec![], vec![], vec![], vec![], vec![], vec![]
            ));
        }
    };
    
    let root = parse.syntax();
    let rel_path = file.strip_prefix(lib_root).unwrap_or(file);
    let module_path = file_to_module_path(file, lib_root);
    let source_file = rel_path.display().to_string();
    
    let mut types = Vec::new();
    let mut traits = Vec::new();
    let mut functions = Vec::new();
    let mut macros = Vec::new();
    let mut constants = Vec::new();
    let mut type_aliases = Vec::new();
    let mut impls = Vec::new();
    
    // Walk top-level items
    for item in root.children() {
        match item.kind() {
            SyntaxKind::STRUCT => {
                if let Some(type_info) = analyze_struct(&item, lib_name, &module_path, &source_file) {
                    types.push(type_info);
                }
            }
            SyntaxKind::ENUM => {
                if let Some(type_info) = analyze_enum(&item, lib_name, &module_path, &source_file) {
                    types.push(type_info);
                }
            }
            SyntaxKind::TRAIT => {
                if let Some(trait_info) = analyze_trait(&item, lib_name, &module_path, &source_file) {
                    traits.push(trait_info);
                }
            }
            SyntaxKind::FN => {
                if let Some(fn_info) = analyze_function(&item, lib_name, &module_path, &source_file) {
                    // Only include if it's public and not in an impl/trait
                    if is_public(&item) && !is_in_impl_or_trait(&item) {
                        functions.push(fn_info);
                    }
                }
            }
            SyntaxKind::MACRO_RULES => {
                if let Some(macro_info) = analyze_macro_rules(&item, lib_name, &module_path, &source_file) {
                    macros.push(macro_info);
                }
            }
            SyntaxKind::MACRO_DEF => {
                if let Some(macro_info) = analyze_macro_def(&item, lib_name, &module_path, &source_file) {
                    macros.push(macro_info);
                }
            }
            SyntaxKind::CONST => {
                if let Some(const_info) = analyze_const(&item, lib_name, &module_path, &source_file) {
                    if is_public(&item) {
                        constants.push(const_info);
                    }
                }
            }
            SyntaxKind::STATIC => {
                if let Some(const_info) = analyze_static(&item, lib_name, &module_path, &source_file) {
                    if is_public(&item) {
                        constants.push(const_info);
                    }
                }
            }
            SyntaxKind::TYPE_ALIAS => {
                if let Some(alias_info) = analyze_type_alias(&item, lib_name, &module_path, &source_file) {
                    if is_public(&item) {
                        type_aliases.push(alias_info);
                    }
                }
            }
            SyntaxKind::IMPL => {
                if let Some(impl_info) = analyze_impl(&item, lib_name, &module_path, &source_file) {
                    impls.push(impl_info);
                }
            }
            SyntaxKind::MODULE => {
                // Recursively analyze nested modules
                let nested = analyze_module(&item, lib_name, &module_path, &source_file);
                types.extend(nested.0);
                traits.extend(nested.1);
                functions.extend(nested.2);
                macros.extend(nested.3);
                constants.extend(nested.4);
                type_aliases.extend(nested.5);
                impls.extend(nested.6);
            }
            _ => {}
        }
    }
    
    let file_info = FileInfo {
        path: source_file.clone(),
        module: module_path,
        line_count: content.lines().count(),
        type_count: types.len(),
        trait_count: traits.len(),
        function_count: functions.len(),
        impl_count: impls.len(),
    };
    
    Ok((file_info, types, traits, functions, macros, constants, type_aliases, impls))
}

fn analyze_module(
    node: &SyntaxNode,
    lib_name: &str,
    parent_module: &str,
    source_file: &str,
) -> (Vec<TypeInfo>, Vec<TraitInfo>, Vec<FunctionInfo>, Vec<MacroInfo>, Vec<ConstantInfo>, Vec<TypeAliasInfo>, Vec<ImplInfo>) {
    let mut types = Vec::new();
    let mut traits = Vec::new();
    let mut functions = Vec::new();
    let mut macros = Vec::new();
    let mut constants = Vec::new();
    let mut type_aliases = Vec::new();
    let mut impls = Vec::new();
    
    // Get module name
    let mod_name = node.children_with_tokens()
        .find(|c| c.kind() == SyntaxKind::NAME)
        .map(|n| n.to_string())
        .unwrap_or_default();
    
    let module_path = if parent_module.is_empty() {
        format!("{}::{}", lib_name, mod_name)
    } else {
        format!("{}::{}", parent_module, mod_name)
    };
    
    // Find ITEM_LIST inside module
    for child in node.children() {
        if child.kind() == SyntaxKind::ITEM_LIST {
            for item in child.children() {
                match item.kind() {
                    SyntaxKind::STRUCT => {
                        if let Some(type_info) = analyze_struct(&item, lib_name, &module_path, source_file) {
                            if is_public(&item) {
                                types.push(type_info);
                            }
                        }
                    }
                    SyntaxKind::ENUM => {
                        if let Some(type_info) = analyze_enum(&item, lib_name, &module_path, source_file) {
                            if is_public(&item) {
                                types.push(type_info);
                            }
                        }
                    }
                    SyntaxKind::TRAIT => {
                        if let Some(trait_info) = analyze_trait(&item, lib_name, &module_path, source_file) {
                            if is_public(&item) {
                                traits.push(trait_info);
                            }
                        }
                    }
                    SyntaxKind::FN => {
                        if let Some(fn_info) = analyze_function(&item, lib_name, &module_path, source_file) {
                            if is_public(&item) {
                                functions.push(fn_info);
                            }
                        }
                    }
                    SyntaxKind::MACRO_RULES => {
                        if let Some(macro_info) = analyze_macro_rules(&item, lib_name, &module_path, source_file) {
                            macros.push(macro_info);
                        }
                    }
                    SyntaxKind::CONST => {
                        if let Some(const_info) = analyze_const(&item, lib_name, &module_path, source_file) {
                            if is_public(&item) {
                                constants.push(const_info);
                            }
                        }
                    }
                    SyntaxKind::STATIC => {
                        if let Some(const_info) = analyze_static(&item, lib_name, &module_path, source_file) {
                            if is_public(&item) {
                                constants.push(const_info);
                            }
                        }
                    }
                    SyntaxKind::TYPE_ALIAS => {
                        if let Some(alias_info) = analyze_type_alias(&item, lib_name, &module_path, source_file) {
                            if is_public(&item) {
                                type_aliases.push(alias_info);
                            }
                        }
                    }
                    SyntaxKind::IMPL => {
                        if let Some(impl_info) = analyze_impl(&item, lib_name, &module_path, source_file) {
                            impls.push(impl_info);
                        }
                    }
                    SyntaxKind::MODULE => {
                        let nested = analyze_module(&item, lib_name, &module_path, source_file);
                        types.extend(nested.0);
                        traits.extend(nested.1);
                        functions.extend(nested.2);
                        macros.extend(nested.3);
                        constants.extend(nested.4);
                        type_aliases.extend(nested.5);
                        impls.extend(nested.6);
                    }
                    _ => {}
                }
            }
        }
    }
    
    (types, traits, functions, macros, constants, type_aliases, impls)
}

// ============================================================================
// Item Analyzers
// ============================================================================

fn analyze_struct(node: &SyntaxNode, _lib_name: &str, module_path: &str, source_file: &str) -> Option<TypeInfo> {
    let struct_node = ast::Struct::cast(node.clone())?;
    let name = struct_node.name()?.text().to_string();
    
    let qualified_path = format!("{}::{}", module_path, name);
    let is_generic = struct_node.generic_param_list().is_some();
    let derives = extract_derives(node);
    
    // Get line number
    let source_line = node.text_range().start().into();
    
    Some(TypeInfo {
        name,
        qualified_path,
        kind: "struct".to_string(),
        is_generic,
        is_unsafe: false, // structs themselves aren't unsafe
        derives,
        methods: vec![], // Methods come from impl blocks
        source_file: source_file.to_string(),
        source_line,
    })
}

fn analyze_enum(node: &SyntaxNode, _lib_name: &str, module_path: &str, source_file: &str) -> Option<TypeInfo> {
    let enum_node = ast::Enum::cast(node.clone())?;
    let name = enum_node.name()?.text().to_string();
    
    let qualified_path = format!("{}::{}", module_path, name);
    let is_generic = enum_node.generic_param_list().is_some();
    let derives = extract_derives(node);
    
    let source_line = node.text_range().start().into();
    
    Some(TypeInfo {
        name,
        qualified_path,
        kind: "enum".to_string(),
        is_generic,
        is_unsafe: false,
        derives,
        methods: vec![],
        source_file: source_file.to_string(),
        source_line,
    })
}

fn analyze_trait(node: &SyntaxNode, _lib_name: &str, module_path: &str, source_file: &str) -> Option<TraitInfo> {
    let trait_node = ast::Trait::cast(node.clone())?;
    let name = trait_node.name()?.text().to_string();
    
    let qualified_path = format!("{}::{}", module_path, name);
    let is_unsafe = trait_node.unsafe_token().is_some();
    let is_auto = has_attribute(node, "auto");
    
    // Extract supertraits
    let supertraits = if let Some(bounds) = trait_node.type_bound_list() {
        bounds.bounds()
            .filter_map(|b| Some(b.syntax().text().to_string()))
            .collect()
    } else {
        vec![]
    };
    
    // Extract associated types and consts
    let mut associated_types = Vec::new();
    let mut associated_consts = Vec::new();
    let mut methods = Vec::new();
    
    if let Some(item_list) = trait_node.assoc_item_list() {
        for item in item_list.assoc_items() {
            match item {
                ast::AssocItem::TypeAlias(ta) => {
                    if let Some(name) = ta.name() {
                        associated_types.push(name.text().to_string());
                    }
                }
                ast::AssocItem::Const(c) => {
                    if let Some(name) = c.name() {
                        associated_consts.push(name.text().to_string());
                    }
                }
                ast::AssocItem::Fn(f) => {
                    if let Some(name) = f.name() {
                        let method = TraitMethodInfo {
                            name: name.text().to_string(),
                            is_generic: f.generic_param_list().is_some(),
                            is_unsafe: f.unsafe_token().is_some(),
                            has_default: f.body().is_some(),
                            can_panic: check_can_panic_fn(&f),
                            must_use: has_attribute(f.syntax(), "must_use"),
                        };
                        methods.push(method);
                    }
                }
                _ => {}
            }
        }
    }
    
    let source_line = node.text_range().start().into();
    
    Some(TraitInfo {
        name,
        qualified_path,
        is_unsafe,
        is_auto,
        supertraits,
        associated_types,
        associated_consts,
        methods,
        source_file: source_file.to_string(),
        source_line,
    })
}

fn analyze_function(node: &SyntaxNode, _lib_name: &str, module_path: &str, source_file: &str) -> Option<FunctionInfo> {
    let fn_node = ast::Fn::cast(node.clone())?;
    let name = fn_node.name()?.text().to_string();
    
    let qualified_path = format!("{}::{}", module_path, name);
    let is_generic = fn_node.generic_param_list().is_some();
    let is_unsafe = fn_node.unsafe_token().is_some();
    let is_const = fn_node.const_token().is_some();
    let can_panic = check_can_panic_fn(&fn_node);
    let must_use = has_attribute(node, "must_use");
    
    let source_line = node.text_range().start().into();
    
    Some(FunctionInfo {
        name,
        qualified_path,
        is_generic,
        is_unsafe,
        can_panic,
        must_use,
        is_const,
        source_file: source_file.to_string(),
        source_line,
    })
}

fn analyze_macro_rules(node: &SyntaxNode, _lib_name: &str, module_path: &str, source_file: &str) -> Option<MacroInfo> {
    let macro_node = ast::MacroRules::cast(node.clone())?;
    let name = macro_node.name()?.text().to_string();
    
    let qualified_path = format!("{}::{}", module_path, name);
    let is_exported = has_attribute(node, "macro_export");
    
    let source_line = node.text_range().start().into();
    
    Some(MacroInfo {
        name,
        qualified_path,
        kind: "declarative".to_string(),
        is_exported,
        source_file: source_file.to_string(),
        source_line,
    })
}

fn analyze_macro_def(node: &SyntaxNode, _lib_name: &str, module_path: &str, source_file: &str) -> Option<MacroInfo> {
    let macro_node = ast::MacroDef::cast(node.clone())?;
    let name = macro_node.name()?.text().to_string();
    
    let qualified_path = format!("{}::{}", module_path, name);
    
    let source_line = node.text_range().start().into();
    
    Some(MacroInfo {
        name,
        qualified_path,
        kind: "procedural".to_string(),
        is_exported: is_public(node),
        source_file: source_file.to_string(),
        source_line,
    })
}

fn analyze_const(node: &SyntaxNode, _lib_name: &str, module_path: &str, source_file: &str) -> Option<ConstantInfo> {
    let const_node = ast::Const::cast(node.clone())?;
    let name = const_node.name()?.text().to_string();
    
    let qualified_path = format!("{}::{}", module_path, name);
    let const_type = const_node.ty()
        .map(|t| t.syntax().text().to_string())
        .unwrap_or_else(|| "?".to_string());
    
    // Try to get simple literal value
    let value = const_node.body()
        .and_then(|b| {
            let text = b.syntax().text().to_string();
            if text.len() < 50 && !text.contains('\n') {
                Some(text)
            } else {
                None
            }
        });
    
    let source_line = node.text_range().start().into();
    
    Some(ConstantInfo {
        name,
        qualified_path,
        const_type,
        value,
        source_file: source_file.to_string(),
        source_line,
    })
}

fn analyze_static(node: &SyntaxNode, _lib_name: &str, module_path: &str, source_file: &str) -> Option<ConstantInfo> {
    let static_node = ast::Static::cast(node.clone())?;
    let name = static_node.name()?.text().to_string();
    
    let qualified_path = format!("{}::{}", module_path, name);
    let const_type = static_node.ty()
        .map(|t| t.syntax().text().to_string())
        .unwrap_or_else(|| "?".to_string());
    
    let source_line = node.text_range().start().into();
    
    Some(ConstantInfo {
        name,
        qualified_path,
        const_type,
        value: None, // Statics usually have complex initializers
        source_file: source_file.to_string(),
        source_line,
    })
}

fn analyze_type_alias(node: &SyntaxNode, _lib_name: &str, module_path: &str, source_file: &str) -> Option<TypeAliasInfo> {
    let alias_node = ast::TypeAlias::cast(node.clone())?;
    let name = alias_node.name()?.text().to_string();
    
    let qualified_path = format!("{}::{}", module_path, name);
    let target = alias_node.ty()
        .map(|t| t.syntax().text().to_string())
        .unwrap_or_else(|| "?".to_string());
    
    let source_line = node.text_range().start().into();
    
    Some(TypeAliasInfo {
        name,
        qualified_path,
        target,
        source_file: source_file.to_string(),
        source_line,
    })
}

fn analyze_impl(node: &SyntaxNode, _lib_name: &str, _module_path: &str, source_file: &str) -> Option<ImplInfo> {
    let impl_node = ast::Impl::cast(node.clone())?;
    
    // Get the type being implemented
    let impl_type = impl_node.self_ty()
        .map(|t| t.syntax().text().to_string())
        .unwrap_or_else(|| "?".to_string());
    
    // Check if this is a trait impl
    let trait_name = impl_node.trait_()
        .map(|t| t.syntax().text().to_string());
    
    let is_unsafe = impl_node.unsafe_token().is_some();
    
    // Check if blanket impl (has generic params that appear in Self type)
    let is_blanket = if let Some(params) = impl_node.generic_param_list() {
        // Check if any generic type param is used in the impl type
        let mut has_type_param_in_self = false;
        for param in params.generic_params() {
            if let ast::GenericParam::TypeParam(tp) = param {
                if let Some(name) = tp.name() {
                    let param_name = name.text().to_string();
                    // Check if this type param appears in the Self type
                    if impl_type.contains(&param_name) {
                        has_type_param_in_self = true;
                        break;
                    }
                }
            }
        }
        has_type_param_in_self
    } else {
        false
    };
    
    // Forwarding impl: blanket impl for wrapper/reference types
    // These propagate traits through &T, &mut T, Box<T>, Arc<T>, etc.
    let is_forwarding = is_blanket && {
        let trimmed = impl_type.trim();
        trimmed.starts_with('&') ||
        trimmed.starts_with("Box<") ||
        trimmed.starts_with("Arc<") ||
        trimmed.starts_with("Rc<") ||
        trimmed.starts_with("Pin<") ||
        trimmed.starts_with("Cell<") ||
        trimmed.starts_with("RefCell<") ||
        trimmed.starts_with("Mutex<") ||
        trimmed.starts_with("RwLock<") ||
        trimmed.starts_with("MaybeUninit<") ||
        trimmed.starts_with("ManuallyDrop<") ||
        trimmed.starts_with("Cow<") ||
        trimmed.starts_with("Option<") ||
        trimmed.starts_with("Result<")
    };
    
    // Bridge impl: blanket impl where having trait A gives you trait B
    // Detected by: impl<T: TraitA> TraitB for T (T directly, not wrapped)
    let is_bridge = is_blanket && trait_name.is_some() && {
        let trimmed = impl_type.trim();
        // The impl type is just T (a bare type parameter), not T wrapped in something
        // This means "if you have the bound, you get this trait for free"
        trimmed.len() <= 2 || // Single letter like T or bare param
        (!trimmed.contains('<') && !trimmed.starts_with('&'))
    };
    
    let where_clause = impl_node.where_clause()
        .map(|w| w.syntax().text().to_string());
    
    // Get method names
    let mut methods = Vec::new();
    if let Some(assoc_items) = impl_node.assoc_item_list() {
        for item in assoc_items.assoc_items() {
            if let ast::AssocItem::Fn(f) = item {
                if let Some(name) = f.name() {
                    methods.push(name.text().to_string());
                }
            }
        }
    }
    
    let source_line = node.text_range().start().into();
    
    Some(ImplInfo {
        impl_type,
        trait_name,
        is_unsafe,
        is_blanket,
        is_forwarding,
        is_bridge,
        where_clause,
        methods,
        source_file: source_file.to_string(),
        source_line,
    })
}

// ============================================================================
// Helper Functions
// ============================================================================

fn find_rust_stdlib() -> Result<(PathBuf, String, String)> {
    let output = std::process::Command::new("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        .context("Failed to run rustc")?;
    
    if !output.status.success() {
        bail!("rustc --print sysroot failed");
    }
    
    let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stdlib_path = PathBuf::from(&sysroot)
        .join("lib/rustlib/src/rust/library");
    
    if !stdlib_path.exists() {
        bail!("Stdlib source not found at {}. Run: rustup component add rust-src", stdlib_path.display());
    }
    
    // Get rust version
    let version_output = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .context("Failed to get rust version")?;
    
    let rust_version = String::from_utf8_lossy(&version_output.stdout).trim().to_string();
    
    Ok((stdlib_path, rust_version, sysroot))
}

fn find_rust_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map_or(false, |ext| ext == "rs")
                && !e.path().to_string_lossy().contains("/tests/")
                && !e.path().to_string_lossy().contains("/benches/")
        })
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn file_to_module_path(file: &Path, lib_root: &Path) -> String {
    let rel = file.strip_prefix(lib_root).unwrap_or(file);
    let parts: Vec<_> = rel.components()
        .filter_map(|c| c.as_os_str().to_str())
        .map(|s| s.trim_end_matches(".rs"))
        .filter(|s| *s != "mod" && *s != "lib")
        .collect();
    
    parts.join("::")
}

fn is_public(node: &SyntaxNode) -> bool {
    node.children_with_tokens()
        .any(|child| child.kind() == SyntaxKind::VISIBILITY)
}

fn is_in_impl_or_trait(node: &SyntaxNode) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::IMPL || parent.kind() == SyntaxKind::TRAIT {
            return true;
        }
        current = parent.parent();
    }
    false
}

fn has_attribute(node: &SyntaxNode, attr_name: &str) -> bool {
    // Use AST to find attributes and check their path
    for attr in node.children().filter_map(ast::Attr::cast) {
        if let Some(meta) = attr.meta() {
            if let Some(path) = meta.path() {
                // Check the path segments for the attribute name
                for segment in path.segments() {
                    if let Some(name) = segment.name_ref() {
                        if name.text() == attr_name {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

fn extract_derives(node: &SyntaxNode) -> Vec<String> {
    let mut derives = Vec::new();
    
    // Use AST to find Attr nodes and check for derive
    for attr in node.children().filter_map(ast::Attr::cast) {
        // Check if this is a derive attribute by examining the path
        if let Some(meta) = attr.meta() {
            if let Some(path) = meta.path() {
                let path_text = path.syntax().text().to_string();
                if path_text == "derive" {
                    // Extract the token tree contents
                    if let Some(token_tree) = meta.token_tree() {
                        // Walk the token tree to find identifiers
                        for token in token_tree.syntax().descendants_with_tokens() {
                            if let ra_ap_syntax::NodeOrToken::Token(t) = token {
                                if t.kind() == SyntaxKind::IDENT {
                                    derives.push(t.text().to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    derives
}

fn check_can_panic_fn(fn_node: &ast::Fn) -> bool {
    if let Some(body) = fn_node.body() {
        // Use AST to find panic-inducing patterns
        for node in body.syntax().descendants() {
            match node.kind() {
                // Check for macro calls: panic!, unreachable!, unimplemented!, todo!, assert!
                SyntaxKind::MACRO_CALL => {
                    if let Some(macro_call) = ast::MacroCall::cast(node.clone()) {
                        if let Some(path) = macro_call.path() {
                            if let Some(segment) = path.segment() {
                                if let Some(name) = segment.name_ref() {
                                    let macro_name = name.text().to_string();
                                    if matches!(macro_name.as_str(), 
                                        "panic" | "unreachable" | "unimplemented" | "todo" | "assert") {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
                // Check for method calls: .unwrap(), .expect()
                SyntaxKind::METHOD_CALL_EXPR => {
                    if let Some(method_call) = ast::MethodCallExpr::cast(node.clone()) {
                        if let Some(name) = method_call.name_ref() {
                            let method_name = name.text().to_string();
                            if matches!(method_name.as_str(), "unwrap" | "expect") {
                                return true;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        false
    } else {
        false
    }
}

// ============================================================================
// Report Writing
// ============================================================================

fn write_detailed_report(inventory: &StdlibInventory, log: &mut fs::File) -> Result<()> {
    // PER-LIBRARY SECTIONS (sections 4, 5, 6 for core, alloc, std)
    // TOC, Introduction, Analysis Progress, Summary are already written in main
    let mut section = 4;
    for (lib_name, lib) in &inventory.libraries {
        let total_reexports: usize = lib.modules.iter().map(|m| m.re_exports.len()).sum();
        let blanket_count = lib.impls.iter().filter(|i| i.is_blanket).count();
        let trait_impl_count = lib.impls.iter().filter(|i| i.trait_name.is_some()).count();
        let inherent_count = lib.impls.iter().filter(|i| i.trait_name.is_none()).count();
        
        // Library header
        writeln!(log, "")?;
        writeln!(log, "=== {}. {} LIBRARY ===", section, lib_name.to_uppercase())?;
        writeln!(log, "")?;
        writeln!(log, "Path: {}", lib.path)?;
        writeln!(log, "Files: {} | Modules: {} | Types: {} | Traits: {} | Functions: {}", 
                 lib.files.len(), lib.modules.len(), lib.types.len(), lib.traits.len(), lib.functions.len())?;
        
        // Modules
        writeln!(log, "")?;
        writeln!(log, "=== {}.1 Modules ({}, {} re-exports) ===", section, lib.modules.len(), total_reexports)?;
        writeln!(log, "")?;
        writeln!(log, "Q1. What modules exist in stdlib? How are they organized?")?;
        writeln!(log, "Q10. How do std, core, and alloc relate via re-exports?")?;
        writeln!(log, "")?;
        for m in &lib.modules {
            let pub_marker = if m.is_public { "pub " } else { "" };
            let children = if m.child_modules.is_empty() {
                String::new()
            } else {
                format!(" children:[{}]", m.child_modules.join(", "))
            };
            writeln!(log, "{}mod {}{}", pub_marker, m.path, children)?;
            for re in &m.re_exports {
                writeln!(log, "  pub use {} as {} ({})", re.source_path, re.name, re.kind)?;
            }
        }
        
        // Prelude
        writeln!(log, "")?;
        if let Some(ref prelude) = lib.prelude {
            let total_items = prelude.types.len() + prelude.traits.len() + prelude.macros.len() + prelude.functions.len();
            writeln!(log, "=== {}.2 Prelude ({} items) ===", section, total_items)?;
            writeln!(log, "")?;
            writeln!(log, "Q2. What's in the prelude (auto-imported into every program)?")?;
            writeln!(log, "")?;
            writeln!(log, "Module: {}", prelude.module_path)?;
            writeln!(log, "Source: {}", prelude.source_file)?;
            writeln!(log, "")?;
            if !prelude.types.is_empty() {
                writeln!(log, "Types ({}):", prelude.types.len())?;
                for item in &prelude.types {
                    writeln!(log, "  {} <- {}", item.name, item.source_path)?;
                }
            }
            if !prelude.traits.is_empty() {
                writeln!(log, "Traits ({}):", prelude.traits.len())?;
                for item in &prelude.traits {
                    writeln!(log, "  {} <- {}", item.name, item.source_path)?;
                }
            }
            if !prelude.macros.is_empty() {
                writeln!(log, "Macros ({}):", prelude.macros.len())?;
                for item in &prelude.macros {
                    writeln!(log, "  {} <- {}", item.name, item.source_path)?;
                }
            }
            if !prelude.functions.is_empty() {
                writeln!(log, "Functions ({}):", prelude.functions.len())?;
                for item in &prelude.functions {
                    writeln!(log, "  {} <- {}", item.name, item.source_path)?;
                }
            }
        } else {
            writeln!(log, "=== {}.2 Prelude (none) ===", section)?;
            writeln!(log, "")?;
            writeln!(log, "This library does not define a prelude.")?;
        }
        
        // Types
        writeln!(log, "")?;
        writeln!(log, "=== {}.3 Types ({}) ===", section, lib.types.len())?;
        writeln!(log, "")?;
        writeln!(log, "Q3. What types (structs/enums) does stdlib provide?")?;
        writeln!(log, "")?;
        for t in &lib.types {
            let generic = if t.is_generic { "<T>" } else { "" };
            let derives = if t.derives.is_empty() { 
                String::new() 
            } else { 
                format!(" [derives: {}]", t.derives.join(", "))
            };
            writeln!(log, "{}{} ({}){}", t.qualified_path, generic, t.kind, derives)?;
        }
        
        // Traits
        writeln!(log, "")?;
        writeln!(log, "=== {}.4 Traits ({}) ===", section, lib.traits.len())?;
        writeln!(log, "")?;
        writeln!(log, "Q4. What traits are defined? What methods do they have?")?;
        writeln!(log, "")?;
        for t in &lib.traits {
            let unsafe_marker = if t.is_unsafe { "unsafe " } else { "" };
            let auto_marker = if t.is_auto { "auto " } else { "" };
            writeln!(log, "{}{}{} ({} methods)", unsafe_marker, auto_marker, t.qualified_path, t.methods.len())?;
            if !t.associated_types.is_empty() {
                writeln!(log, "  associated types: {}", t.associated_types.join(", "))?;
            }
            for m in &t.methods {
                let default = if m.has_default { " [default]" } else { "" };
                let unsafe_m = if m.is_unsafe { "unsafe " } else { "" };
                writeln!(log, "  - {}{}(){}", unsafe_m, m.name, default)?;
            }
        }
        
        // Functions
        writeln!(log, "")?;
        writeln!(log, "=== {}.5 Free Functions ({}) ===", section, lib.functions.len())?;
        writeln!(log, "")?;
        writeln!(log, "Q5. What free functions are available?")?;
        writeln!(log, "")?;
        for f in &lib.functions {
            let unsafe_f = if f.is_unsafe { "unsafe " } else { "" };
            let const_f = if f.is_const { "const " } else { "" };
            writeln!(log, "{}{}{}()", const_f, unsafe_f, f.qualified_path)?;
        }
        
        // Macros
        writeln!(log, "")?;
        writeln!(log, "=== {}.6 Macros ({}) ===", section, lib.macros.len())?;
        writeln!(log, "")?;
        writeln!(log, "Q6. What macros are exported (vec!, format!, etc.)?")?;
        writeln!(log, "")?;
        for m in &lib.macros {
            let exported = if m.is_exported { " [exported]" } else { "" };
            writeln!(log, "{}!{}", m.qualified_path, exported)?;
        }
        
        // Constants
        writeln!(log, "")?;
        writeln!(log, "=== {}.7 Constants ({}) ===", section, lib.constants.len())?;
        writeln!(log, "")?;
        writeln!(log, "Q7. What constants are defined (MAX, MIN, PI, etc.)?")?;
        writeln!(log, "")?;
        for c in &lib.constants {
            let val = c.value.as_ref().map(|v| format!(" = {}", v)).unwrap_or_default();
            writeln!(log, "{}: {}{}", c.qualified_path, c.const_type, val)?;
        }
        
        // Type Aliases
        writeln!(log, "")?;
        writeln!(log, "=== {}.8 Type Aliases ({}) ===", section, lib.type_aliases.len())?;
        writeln!(log, "")?;
        writeln!(log, "Q8. What type aliases exist (io::Result, etc.)?")?;
        writeln!(log, "")?;
        for a in &lib.type_aliases {
            writeln!(log, "{} = {}", a.qualified_path, a.target)?;
        }
        
        // Impls
        let forwarding_count = lib.impls.iter().filter(|i| i.is_forwarding).count();
        let bridge_count = lib.impls.iter().filter(|i| i.is_bridge).count();
        
        writeln!(log, "")?;
        writeln!(log, "=== {}.9 Impls ({}) ===", section, lib.impls.len())?;
        writeln!(log, "")?;
        writeln!(log, "Q9. What impls exist? Which are blanket/forwarding/bridge?")?;
        writeln!(log, "")?;
        writeln!(log, "Inherent impls:   {}", inherent_count)?;
        writeln!(log, "Trait impls:      {}", trait_impl_count)?;
        writeln!(log, "Blanket impls:    {}", blanket_count)?;
        writeln!(log, "  Forwarding:     {} (propagate traits through &T, Box<T>, etc.)", forwarding_count)?;
        writeln!(log, "  Bridge:         {} (trait A gives you trait B for T)", bridge_count)?;
        writeln!(log, "")?;
        
        // Show forwarding impls
        writeln!(log, "Forwarding impls (traits propagated through wrappers):")?;
        for i in lib.impls.iter().filter(|i| i.is_forwarding) {
            if let Some(ref trait_name) = i.trait_name {
                writeln!(log, "  impl {} for {}", trait_name, i.impl_type)?;
            }
        }
        writeln!(log, "")?;
        
        // Show bridge impls
        writeln!(log, "Bridge impls (having trait A gives you trait B):")?;
        for i in lib.impls.iter().filter(|i| i.is_bridge) {
            if let Some(ref trait_name) = i.trait_name {
                writeln!(log, "  impl {} for {}", trait_name, i.impl_type)?;
            }
        }
        
        section += 1;
    }
    
    Ok(())
}

