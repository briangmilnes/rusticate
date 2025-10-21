// Copyright (C) Brian G. Milnes 2025

//! Fix: Add pub type and fix unused self parameters
//! 
//! PARTIAL IMPLEMENTATION: Works for simple algorithm modules with pub fn.
//! Complex cases (impl blocks with generics, external type impls) need more work.
//! 
//! For modules missing pub data types:
//! 1. Add pub type T based on actual usage
//! 2. Fix unused self parameters by making self the primary data
//! 3. Update call sites in src, tests, and benches
//! 
//! Current implementation is InsertionSortSt-specific and performs:
//! - Add `pub type T<S> = [S];`
//! - Transform trait signature: `fn insSort(&self, slice: &mut [T])` -> `fn insSort(&mut self)`
//! - Transform impl header: `impl<T> Trait<T> for T` -> `impl<S> Trait<S> for [S]`
//! - Transform method body: replace `slice` with `self`
//! - Transform call sites: `x.insSort(&mut data)` -> `data.insSort()`
//! 
//! TODO: Make this generic by:
//! 1. Extracting trait/method names from AST analysis
//! 2. Detecting type parameter patterns from impl blocks
//! 3. Using analysis results to drive transformations
//! 4. Supporting multiple methods per trait
//! 
//! Binary: rusticate-fix-no-pub-type

use anyhow::Result;
use ra_ap_syntax::{
    ast::{self, AstNode, HasVisibility, HasName, HasArgList},
    SyntaxKind, SourceFile, Edition
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use rusticate::StandardArgs;

/// Clean parameter type for use in type alias
/// 
/// Removes:
/// - Leading `&` references
/// - Leading `mut` keywords
/// - Substitutes generic `T` with `N` to avoid recursive types
/// 
/// Transforms:
/// - `&mut [T]` -> `[N]`
/// - `&ArraySeqStPerS<T>` -> `ArraySeqStPerS<N>`
/// - `&mut SomeType<T>` -> `SomeType<N>`
fn clean_parameter_type(type_str: &str) -> String {
    let cleaned = type_str
        .trim_start_matches('&')
        .trim()
        .trim_start_matches("mut")
        .trim();
    
    substitute_generic_t(cleaned)
}

/// Substitute generic T with concrete N to avoid recursive type aliases
/// 
/// Transforms:
/// - `ArraySeqStPerS<T>` -> `ArraySeqStPerS<N>`
/// - `BTreeSet<T>` -> `BTreeSet<N>`
/// - `SomeType<T, U>` -> `SomeType<N, U>` (only first generic)
/// 
/// This avoids creating recursive aliases like `pub type T = SomeType<T>;`
fn substitute_generic_t(type_str: &str) -> String {
    // Replace <T> with <N>
    let result = type_str.replace("<T>", "<N>");
    // Also handle <T, ...> patterns
    let result = result.replace("<T,", "<N,");
    // Also handle [T] patterns
    let result = result.replace("[T]", "[N]");
    result
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    
    let base_dir = args.base_dir();
    println!("Entering directory '{}'", base_dir.display());
    println!();
    
    // Get all Rust files to process
    let files = rusticate::find_rust_files(&args.paths);
    
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut error_count = 0;
    
    for file_path in files {
        // Only process files in src/ directory
        if !file_path.to_string_lossy().contains("/src/") {
            continue;
        }
        
        match process_file(&file_path) {
            Ok(did_work) => {
                if did_work {
                    success_count += 1;
                } else {
                    skip_count += 1;
                }
            }
            Err(e) => {
                eprintln!("{}:1: Error: {}", file_path.display(), e);
                error_count += 1;
            }
        }
    }
    
    println!();
    let file_word = if success_count == 1 { "file" } else { "files" };
    let skip_word = if skip_count == 1 { "file" } else { "files" };
    let error_word = if error_count == 1 { "file" } else { "files" };
    println!("Summary: {} {} fixed, {} {} skipped, {} {} with errors", 
             success_count, file_word, skip_count, skip_word, error_count, error_word);
    println!("Completed in {}ms", start.elapsed().as_millis());
    
    if error_count > 0 {
        std::process::exit(1);
    }
    
    Ok(())
}

fn process_file(file_path: &Path) -> Result<bool> {
    // Step 1: Analyze the module to determine the transformation
    let source = fs::read_to_string(file_path)?;
    
    let analysis = match analyze_module(&source, file_path) {
        Ok(a) => a,
        Err(e) => {
            // Special case: if module already has pub struct/enum, skip (not an error)
            let err_msg = e.to_string();
            if err_msg.contains("already has pub struct") || err_msg.contains("already has pub enum") {
                return Ok(false); // Skip this file
            }
            return Err(e); // Real error
        }
    };
    
    // Check if this has unused self - that requires the complex transformation
    if analysis.has_unused_self && analysis.module_name != "InsertionSortSt" {
        return Err(anyhow::anyhow!(
            "Module has unused self parameter - complex transformation not yet supported. \
             Only InsertionSortSt prototype implemented."
        ));
    }
    
    // If no pub type needed and no transformation needed, skip
    if !analysis.needs_pub_type {
        return Ok(false);
    }
    
    let mut did_work = false;
    
    // Step A: Add pub type if needed
    if analysis.needs_pub_type {
        println!("{}:{}:\tAdding pub type: {}", 
            file_path.display(), analysis.module_line, analysis.recommended_type);
        
        let new_source = add_pub_type(&source, &analysis)?;
        fs::write(file_path, &new_source)?;
        println!("{}:{}:\tAdded pub type", file_path.display(), analysis.module_line);
        did_work = true;
    }
    
    // Steps B-D: For modules with trait+pub fn pattern, transform if needed
    let current_source = fs::read_to_string(file_path)?;
    if analysis.recommended_type.contains("pub type T =") && has_standalone_pub_fn(&current_source, &analysis)? {
        let impl_exists = has_trait_impl(&current_source, &analysis)?;
        
        if !impl_exists {
            // Step B: Transform trait signatures
            println!("{}:{}:\tTransforming trait signatures to use &self", file_path.display(), analysis.module_line);
            let mut new_source = transform_algorithm_trait(&current_source, &analysis)?;
            fs::write(file_path, &new_source)?;
            println!("{}:{}:\tTransformed trait signatures", file_path.display(), analysis.module_line);
            
            // Step C: Create impl Trait for T block
            println!("{}:{}:\tCreating impl Trait for T block", file_path.display(), analysis.module_line);
            new_source = fs::read_to_string(file_path)?;
            new_source = create_trait_impl(&new_source, &analysis)?;
            fs::write(file_path, &new_source)?;
            println!("{}:{}:\tCreated impl block", file_path.display(), analysis.module_line);
        }
        
        // Step D: Remove redundant standalone pub fn (always run if standalone fn exists)
        println!("{}:{}:\tRemoving redundant standalone pub fn", file_path.display(), analysis.module_line);
        let mut new_source = fs::read_to_string(file_path)?;
        new_source = remove_standalone_pub_fn(&new_source, &analysis)?;
        fs::write(file_path, &new_source)?;
        println!("{}:{}:\tRemoved standalone pub fn", file_path.display(), analysis.module_line);
        
        // Step F: Fix call sites in test and bench files
        match find_test_file(file_path)? {
            Some(test_file) => {
                println!("{}:1:\tFixing test call sites", test_file.display());
                fix_call_sites(&test_file, &analysis)?;
                println!("{}:1:\tFixed test call sites", test_file.display());
            }
            None => {}
        }
        
        if let Some(bench_file) = find_bench_file(file_path)? {
            println!("{}:1:\tFixing bench call sites", bench_file.display());
            fix_call_sites(&bench_file, &analysis)?;
            println!("{}:1:\tFixed bench call sites", bench_file.display());
        }
        
        did_work = true;
    }
    
    // Step 4: Fix unused self if needed (InsertionSortSt pattern)
        if analysis.has_unused_self {
            println!("{}:{}:\tFixing unused self parameter", file_path.display(), analysis.module_line);
        let mut new_source = fs::read_to_string(file_path)?;
            new_source = fix_unused_self(&new_source, &analysis)?;
            fs::write(file_path, &new_source)?;
            println!("{}:{}:\tFixed method signatures and body", file_path.display(), analysis.module_line);
        did_work = true;
        
        // Step 5: Find and fix test file if it exists
        match find_test_file(file_path)? {
            Some(test_file) => {
                println!("{}:1:\tUpdating test call sites", test_file.display());
                fix_call_sites(&test_file, &analysis)?;
                println!("{}:1:\tUpdated test call sites", test_file.display());
            }
            None => {}
        }
        
        // Step 6: Find and fix bench file if it exists
        if let Some(bench_file) = find_bench_file(file_path)? {
            println!("{}:1:\tUpdating bench call sites", bench_file.display());
            fix_call_sites(&bench_file, &analysis)?;
            println!("{}:1:\tUpdated bench call sites", bench_file.display());
        }
    }
    
    if !did_work {
        println!("{}:{}:\tNo changes needed", file_path.display(), analysis.module_line);
    }
    
    Ok(did_work)
}

#[derive(Debug)]
struct ModuleAnalysis {
    module_name: String,
    module_line: usize,
    needs_pub_type: bool,
    recommended_type: String,
    has_unused_self: bool,
    _unused_self_method: Option<String>,
    source_file: PathBuf,
}

fn analyze_module(source: &str, source_file: &Path) -> Result<ModuleAnalysis> {
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors: {:?}", parsed.errors()));
    }
    let tree = parsed.tree();
    
    // Find module name using AST
    let root = tree.syntax();
    let mut module_name = "Unknown".to_string();
    let mut module_line = 1;
    
    for node in root.children() {
        if node.kind() == SyntaxKind::MODULE {
            if let Some(module) = ast::Module::cast(node.clone()) {
                if let Some(vis) = module.visibility() {
                    if vis.to_string() == "pub" {
                        if let Some(name) = module.name() {
                            module_name = name.to_string();
                            module_line = get_line_number(source, node.text_range().start().into());
                            break;
                        }
                    }
                }
            }
        }
    }
    
    // Check if pub type exists and extract it
    let root = tree.syntax();
    let existing_pub_type = root.descendants()
        .filter(|node| node.kind() == SyntaxKind::TYPE_ALIAS)
        .find_map(|node| {
            if let Some(type_alias) = ast::TypeAlias::cast(node) {
                if type_alias.visibility().is_some() {
                    return Some(type_alias.to_string().trim().to_string());
            }
            }
            None
        });
    
    if let Some(existing_type) = existing_pub_type {
        return Ok(ModuleAnalysis {
            module_name,
            module_line,
            needs_pub_type: false,
            recommended_type: existing_type,
            has_unused_self: false,
            _unused_self_method: None,
            source_file: source_file.to_path_buf(),
        });
    }
    
    // Compute recommended type by analyzing the module
    let (recommended_type, has_unused_self) = compute_recommended_type(&root)?;
    
    Ok(ModuleAnalysis {
        module_name,
        module_line,
        needs_pub_type: true,
        recommended_type,
        has_unused_self,
        _unused_self_method: None,
        source_file: source_file.to_path_buf(),
    })
}

fn compute_recommended_type(root: &ra_ap_syntax::SyntaxNode) -> Result<(String, bool)> {
    // First check: if module already has pub struct or pub enum, no type alias needed
    for node in root.descendants() {
        if node.kind() == SyntaxKind::STRUCT {
            if let Some(struct_ast) = ast::Struct::cast(node.clone()) {
                if struct_ast.visibility().map_or(false, |v| v.to_string() == "pub") {
                    return Err(anyhow::anyhow!("Module already has pub struct - no type alias needed"));
                }
            }
        }
        if node.kind() == SyntaxKind::ENUM {
            if let Some(enum_ast) = ast::Enum::cast(node.clone()) {
                if enum_ast.visibility().map_or(false, |v| v.to_string() == "pub") {
                    return Err(anyhow::anyhow!("Module already has pub enum - no type alias needed"));
                }
            }
        }
    }
    
    // Look for trait methods first - if multi-parameter, use first param type
    let mut has_unused_self = false;
    let mut proposed_type: Option<String> = None;
    
    // Check trait methods for parameter patterns
    for node in root.descendants() {
        if node.kind() == SyntaxKind::TRAIT {
            for child in node.descendants() {
                if child.kind() == SyntaxKind::FN {
                    if let Some(fn_ast) = ast::Fn::cast(child.clone()) {
                        if let Some(param_list) = fn_ast.param_list() {
                            let params: Vec<_> = param_list.params().collect();
                            
                            if params.len() == 1 {
                                // Single parameter: use that parameter's type
                                if let Some(first_param) = params.first() {
                                    let param_text = first_param.to_string();
                                    // Extract type from "name: Type" or "name: &Type" or "name: &mut Type"
                                    if let Some(colon_pos) = param_text.find(':') {
                                        let type_part = param_text[colon_pos + 1..].trim();
                                        // Clean the type (remove &, mut, substitute generics)
                                        let concrete_type = clean_parameter_type(type_part);
                                        proposed_type = Some(format!("pub type T = {};", concrete_type));
                                        return Ok((proposed_type.unwrap(), has_unused_self));
                                    }
                                }
                            } else if params.len() >= 2 {
                                // Multi-parameter: use first parameter's type
                                if let Some(first_param) = params.first() {
                                    let param_text = first_param.to_string();
                                    // Extract type from "name: Type" or "name: &Type" or "name: &mut Type"
                                    if let Some(colon_pos) = param_text.find(':') {
                                        let type_part = param_text[colon_pos + 1..].trim();
                                        // Clean the type (remove &, mut, substitute generics)
                                        let concrete_type = clean_parameter_type(type_part);
                                        proposed_type = Some(format!("pub type T = {};", concrete_type));
                                        return Ok((proposed_type.unwrap(), has_unused_self));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // If no multi-param traits found, check impl blocks
    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            if let Some(impl_ast) = ast::Impl::cast(node.clone()) {
                // First check for impl Trait for ExternalType (like AtomicUsize)
                if let Some(_trait_ref) = impl_ast.trait_() {
                    if let Some(self_ty) = impl_ast.self_ty() {
                        let type_name = self_ty.to_string();
                        
                        // If it's not just "T", it's an external type
                        if type_name != "T" {
                            // Check if it's a concrete type (starts with uppercase, or contains ::)
                            if type_name.chars().next().map_or(false, |c| c.is_uppercase()) || type_name.contains("::") {
                                proposed_type = Some(format!("pub type T = {};", type_name));
                                break;
                            }
                        }
                    }
                }
                
                // Check if this is impl<T> ... for T pattern
                if let Some(self_ty) = impl_ast.self_ty() {
                    if self_ty.to_string().trim() == "T" {
                        // Look at method parameters to find the actual data type
                        for child in node.descendants() {
                            if child.kind() == SyntaxKind::FN {
                                if let Some(fn_ast) = ast::Fn::cast(child.clone()) {
                                    if let Some(param_list) = fn_ast.param_list() {
                                        for param in param_list.params() {
                                            // Check if this is a self parameter
                                            if let Some(_self_param) = ast::SelfParam::cast(param.syntax().clone()) {
                                                // Check if self is unused (simple heuristic: look for method body)
                                                if let Some(body) = fn_ast.body() {
                                                    let body_text = body.to_string();
                                                    // Very simple check: if body doesn't mention "self"
                                                    if !body_text.contains("self") {
                                                        has_unused_self = true;
                                                    }
                                                }
                                                continue;
                                            }
                                            
                                            // Extract type from parameter using AST
                                            if let Some(ty) = param.ty() {
                                                let type_str = ty.to_string();
                                                
                                                // Extract the base type
                                                if type_str.starts_with("&mut ") {
                                                    let base = &type_str[5..];
                                                    proposed_type = Some(format!("pub type T<S> = {};", base));
                                                } else if type_str.starts_with("&") {
                                                    let base = &type_str[1..].trim();
                                                    proposed_type = Some(format!("pub type T<S> = {};", base));
                                                } else {
                                                    proposed_type = Some(format!("pub type T<S> = {};", type_str));
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
                                if proposed_type.is_some() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            
            if proposed_type.is_some() {
                break;
            }
        }
    }
    
    // If no impl found, check for pub fn (algorithm modules)
    if proposed_type.is_none() {
        // First check if Types::Types::* is imported
        let mut has_types_import = false;
        for node in root.descendants() {
            if node.kind() == SyntaxKind::USE {
                if let Some(use_item) = ast::Use::cast(node.clone()) {
                    if let Some(use_tree) = use_item.use_tree() {
                        // Check for Types::Types::* pattern using AST
                        // Handles both "crate::Types::Types::*" and "Types::Types::*"
                        if let Some(path) = use_tree.path() {
                            let segments: Vec<_> = path.segments().map(|s| s.to_string()).collect();
                            // Check for Types::Types or crate::Types::Types
                            let has_types_types = if segments.len() >= 2 && segments[0] == "Types" && segments[1] == "Types" {
                                true
                            } else if segments.len() >= 3 && segments[0] == "crate" && segments[1] == "Types" && segments[2] == "Types" {
                                true
                            } else {
                                false
                            };
                            
                            if has_types_types && use_tree.to_string().ends_with("::*") {
                                has_types_import = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
        
        // Only recommend T = N if Types is imported
        if has_types_import {
            for node in root.descendants() {
                if node.kind() == SyntaxKind::FN {
                    if let Some(fn_ast) = ast::Fn::cast(node.clone()) {
                        if fn_ast.visibility().map_or(false, |v| v.to_string() == "pub") {
                            // Found a pub fn and Types is imported - use N
                            proposed_type = Some("pub type T = N;".to_string());
                            break;
                        }
                    }
                }
            }
        }
    }
    
    // If we still don't have a type, fail - don't add useless types
    let recommended = proposed_type.ok_or_else(|| {
        anyhow::anyhow!("Cannot determine meaningful pub type for this module")
    })?;
    
    Ok((recommended, has_unused_self))
}

fn add_pub_type(source: &str, analysis: &ModuleAnalysis) -> Result<String> {
    // Find the position after "pub mod Name {"
    let mod_start = source.find(&format!("pub mod {} {{", analysis.module_name))
        .ok_or_else(|| anyhow::anyhow!("Could not find module declaration"))?;
    
    let brace_pos = source[mod_start..].find('{')
        .ok_or_else(|| anyhow::anyhow!("Could not find opening brace"))?;
    
    let after_brace_pos = mod_start + brace_pos + 1;
    let after_brace = &source[after_brace_pos..];
    
    // Find the position after all use statements
    let mut insert_pos = after_brace_pos;
    let mut last_use_pos = after_brace_pos;
    
    for line in after_brace.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ") {
            last_use_pos = insert_pos + line.len() + 1; // +1 for newline
        }
        insert_pos += line.len() + 1;
        
        // Stop if we hit non-blank, non-use line
        if !trimmed.is_empty() && !trimmed.starts_with("use ") {
            break;
        }
    }
    
    // Use position after last use statement
    insert_pos = last_use_pos;
    
    // Skip any existing blank lines after use statements
    let remaining = &source[insert_pos..];
    for line in remaining.lines() {
        if line.trim().is_empty() {
            insert_pos += line.len() + 1;
    } else {
            break;
        }
    }
    
    // Get indentation from the next non-empty line
    let indent = if let Some(first_line) = remaining.lines().find(|l| !l.trim().is_empty()) {
        first_line.chars().take_while(|c| c.is_whitespace()).collect::<String>()
    } else {
        "    ".to_string()
    };
    
    // Build the insertion: indent + type + blank line (one blank line already exists before insert_pos)
    let insertion = format!("{}{}\n", indent, analysis.recommended_type);
    
    // Insert the type
    let before = &source[..insert_pos];
    let after = &source[insert_pos..];
    
    Ok(format!("{}{}{}", before, insertion, after))
}

fn find_test_file(src_file: &Path) -> Result<Option<PathBuf>> {
    // src/ChapXX/ModuleName.rs -> tests/ChapXX/TestModuleName.rs
    // Handle naming inconsistencies: Algorithm21_2 -> TestAlgorithm_21_2
    let file_stem = src_file.file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid file name"))?;
    
    let parent = src_file.parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid parent directory"))?;
    
    let project_root = src_file.ancestors().nth(3)
        .ok_or_else(|| anyhow::anyhow!("Could not find project root"))?;
    
    // Try pattern 1: TestModuleName.rs
    let test_file = project_root
        .join("tests")
        .join(parent)
        .join(format!("Test{}.rs", file_stem));
    
    if test_file.exists() {
        return Ok(Some(test_file));
    }
    
    // Try pattern 2: Insert underscore before first digit (Algorithm21_2 -> TestAlgorithm_21_2)
    if let Some(first_digit_pos) = file_stem.find(|c: char| c.is_ascii_digit()) {
        let modified_stem = format!("{}_{}",
            &file_stem[..first_digit_pos],
            &file_stem[first_digit_pos..]);
        let test_file2 = project_root
            .join("tests")
            .join(parent)
            .join(format!("Test{}.rs", modified_stem));
        
        if test_file2.exists() {
            return Ok(Some(test_file2));
        }
    }
    
    Ok(None)
}

fn find_bench_file(src_file: &Path) -> Result<Option<PathBuf>> {
    // src/ChapXX/ModuleName.rs -> benches/ChapXX/BenchModuleName.rs
    let file_stem = src_file.file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid file name"))?;
    
    let parent = src_file.parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid parent directory"))?;
    
    let bench_file = src_file.ancestors().nth(3)
        .ok_or_else(|| anyhow::anyhow!("Could not find project root"))?
        .join("benches")
        .join(parent)
        .join(format!("Bench{}.rs", file_stem));
    
    if bench_file.exists() {
        Ok(Some(bench_file))
    } else {
        Ok(None)
    }
}

fn fix_call_sites(file_path: &Path, analysis: &ModuleAnalysis) -> Result<()> {
    let source = fs::read_to_string(file_path)?;
    
    // Step 1: Fix the imports - replace function imports with wildcard imports
    let mut new_source = fix_imports_to_wildcard(&source, &analysis.module_name)?;
    
    // Step 2: Fix the call sites
    new_source = if analysis.has_unused_self {
        // InsertionSortSt pattern: change receiver.method(&mut data) to data.method()
        fix_unused_self_calls(&new_source)?
    } else if analysis.recommended_type.contains("pub type T =") {
        // All module patterns: change Module::method(arg1, ...) to arg1.method(...)
        fix_algorithm_call_sites(&new_source, analysis)?
    } else {
        // No transformation needed for call sites
        new_source
    };
    
    fs::write(file_path, new_source)?;
    
    Ok(())
}

/// Replace `use Module::function_name;` with `use Module::*;` using AST
fn fix_imports_to_wildcard(source: &str, module_name: &str) -> Result<String> {
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Ok(source.to_string()); // If parse fails, return unchanged
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    
    // Find all USE items that import from our module
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                if let Some(use_tree) = use_item.use_tree() {
                    let use_text = use_tree.to_string();
                    
                    // Check if this is importing from our module (e.g., "Chap55::DFSStEph::DFSStEph::dfs")
                    if use_text.contains(&format!("::{}::{}", module_name, module_name)) && !use_text.ends_with("::*") {
                        // Extract the module path up to the second module name
                        if let Some(path) = use_tree.path() {
                            let segments: Vec<_> = path.segments().map(|s| s.to_string()).collect();
                            
                            // Find where the module name appears twice in sequence
                            let mut module_path_parts = Vec::new();
                            let mut found_double = false;
                            for (i, segment) in segments.iter().enumerate() {
                                module_path_parts.push(segment.clone());
                                if i > 0 && segments[i - 1] == *module_name && segment == module_name {
                                    found_double = true;
                                    break;
                                }
                            }
                            
                            if found_double {
                                // Build new import: path::to::Module::Module::*
                                let new_import = format!("{}::*", module_path_parts.join("::"));
                                
                                // Replace the entire use_tree
                                let start: usize = use_tree.syntax().text_range().start().into();
                                let end: usize = use_tree.syntax().text_range().end().into();
                                replacements.push((start, end, new_import));
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Apply replacements from end to start
    let mut result = source.to_string();
    replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    
    for (start, end, new_text) in replacements {
        result.replace_range(start..end, &new_text);
    }
    
    Ok(result)
}

fn fix_algorithm_call_sites(source: &str, analysis: &ModuleAnalysis) -> Result<String> {
    // Transform call sites from Module::method(arg1, arg2) to arg1.method(arg2)
    // Uses AST traversal to find and transform call expressions
    
    let module_source = fs::read_to_string(&analysis.source_file)?;
    let method_names = extract_trait_method_names_from_source(&module_source)?;
    
    if method_names.is_empty() {
        return Ok(source.to_string());
    }
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors in test/bench file"));
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    
    // Find all CALL_EXPR nodes
    for node in root.descendants() {
        if node.kind() == SyntaxKind::CALL_EXPR {
            if let Some(call_expr) = ast::CallExpr::cast(node.clone()) {
                // Get the callee expression (the thing being called)
                if let Some(callee_expr) = call_expr.expr() {
                    let callee_text = callee_expr.to_string();
                    
                    // Check if this call is for one of our trait methods
                    for method_name in &method_names {
                        // Check if callee is "method_name" or "Module::method_name"
                        // But NOT "receiver.method_call" (already a method call)
                        let is_function_call = callee_text == *method_name || 
                                              callee_text.ends_with(&format!("::{}", method_name));
                        let is_method_call = callee_text.ends_with(&format!(".{}", method_name));
                        
                        if is_function_call && !is_method_call {
                            // This is a function/static call, transform to method call
                            if let Some(arg_list) = call_expr.arg_list() {
                                let args: Vec<String> = arg_list.args().map(|a| a.to_string()).collect();
                                
                                if args.is_empty() {
                                    continue;
                                }
                                
                                // First argument becomes the receiver
                                let receiver = &args[0];
                                
                                let new_call = if args.len() == 1 {
                                    format!("{}.{}()", receiver, method_name)
                                } else {
                                    let remaining_args = args[1..].join(", ");
                                    format!("{}.{}({})", receiver, method_name, remaining_args)
                                };
                                
                                let start: usize = node.text_range().start().into();
                                let end: usize = node.text_range().end().into();
                                replacements.push((start, end, new_call));
                                break; // Only replace once per call expr
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Apply replacements from end to start to preserve offsets
    let mut result = source.to_string();
    replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    for (start, end, replacement) in replacements {
        result.replace_range(start..end, &replacement);
    }
    
    Ok(result)
}

fn extract_trait_method_names_from_source(source: &str) -> Result<Vec<String>> {
    // Parse the source module and extract trait method names
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors"));
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut method_names = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::TRAIT {
            if let Some(_trait_ast) = ast::Trait::cast(node.clone()) {
                // Collect method names from trait
                for child in node.descendants() {
                    if child.kind() == SyntaxKind::FN {
                        if let Some(fn_ast) = ast::Fn::cast(child.clone()) {
                            if let Some(fn_name) = fn_ast.name() {
                                method_names.push(fn_name.text().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(method_names)
}

fn has_trait_impl(source: &str, _analysis: &ModuleAnalysis) -> Result<bool> {
    // Check if there's already an impl Trait for T block
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Ok(false);
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Look for impl blocks that implement a trait (have "for")
    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            let impl_text = node.to_string();
            if impl_text.contains("Trait") && impl_text.contains(" for T") {
                return Ok(true);
            }
        }
    }
    
    Ok(false)
}

fn has_standalone_pub_fn(source: &str, _analysis: &ModuleAnalysis) -> Result<bool> {
    // Check if there are any standalone pub fn declarations that match trait methods
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Ok(false);
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find trait and collect method names
    let mut method_names: Vec<String> = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::TRAIT {
            if let Some(_trait_ast) = ast::Trait::cast(node.clone()) {
                // Collect method names from trait
                for child in node.descendants() {
                    if child.kind() == SyntaxKind::FN {
                        if let Some(fn_ast) = ast::Fn::cast(child.clone()) {
                            if let Some(fn_name) = fn_ast.name() {
                                method_names.push(fn_name.text().to_string());
                            }
                        }
                    }
                }
                break;
            }
        }
    }
    
    // Check if there are any standalone pub fn matching trait method names
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FN {
            // Check if this function is NOT inside an impl block
            let mut in_impl = false;
            let mut parent = node.parent();
            while let Some(p) = parent {
                if p.kind() == SyntaxKind::IMPL {
                    in_impl = true;
                    break;
                }
                parent = p.parent();
            }
            
            // Only check standalone functions (not in impl blocks)
            if !in_impl {
                if let Some(fn_ast) = ast::Fn::cast(node.clone()) {
                    if let Some(fn_name) = fn_ast.name() {
                        let name = fn_name.text().to_string();
                        
                        // Check if this is a pub fn matching a trait method
                        if method_names.contains(&name) {
                            if let Some(vis) = fn_ast.visibility() {
                                if vis.to_string() == "pub" {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(false)
}

fn remove_standalone_pub_fn(source: &str, _analysis: &ModuleAnalysis) -> Result<String> {
    // Step D: Remove standalone pub fn that duplicate trait methods
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors"));
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find trait and collect method names
    let mut method_names: Vec<String> = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::TRAIT {
            if let Some(_trait_ast) = ast::Trait::cast(node.clone()) {
                // Collect method names from trait
                for child in node.descendants() {
                    if child.kind() == SyntaxKind::FN {
                        if let Some(fn_ast) = ast::Fn::cast(child.clone()) {
                            if let Some(fn_name) = fn_ast.name() {
                                method_names.push(fn_name.text().to_string());
                            }
                        }
                    }
                }
                break;
            }
        }
    }
    
    // Find all standalone pub fn that match trait method names and collect their ranges
    let mut to_remove: Vec<(usize, usize)> = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FN {
            // Check if this function is NOT inside an impl block
            let mut in_impl = false;
            let mut parent = node.parent();
            while let Some(p) = parent {
                if p.kind() == SyntaxKind::IMPL {
                    in_impl = true;
                    break;
                }
                parent = p.parent();
            }
            
            // Only remove standalone functions (not in impl blocks)
            if !in_impl {
                if let Some(fn_ast) = ast::Fn::cast(node.clone()) {
                    if let Some(fn_name) = fn_ast.name() {
                        let name = fn_name.text().to_string();
                        
                        // Check if this is a pub fn matching a trait method
                        if method_names.contains(&name) {
                            if let Some(vis) = fn_ast.visibility() {
                                if vis.to_string() == "pub" {
                                    // Get the position including doc comments
                                    let fn_start: usize = node.text_range().start().into();
                                    let fn_end: usize = node.text_range().end().into();
                                    
                                    // Look backwards for doc comments
                                    let mut actual_start = fn_start;
                                    let before = &source[..fn_start];
                                    
                                    // Find the start of the documentation block
                                    let mut lines_before: Vec<&str> = before.lines().collect();
                                    while let Some(last_line) = lines_before.last() {
                                        let trimmed = last_line.trim();
                                        if trimmed.starts_with("///") || trimmed.is_empty() {
                                            if let Some(line_start) = before.rfind(last_line) {
                                                actual_start = line_start;
                                            }
                                            lines_before.pop();
                                        } else {
                                            break;
                                        }
                                    }
                                    
                                    to_remove.push((actual_start, fn_end));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Remove from end to start so offsets don't shift
    let mut result = source.to_string();
    to_remove.sort_by_key(|(start, _)| std::cmp::Reverse(*start));
    
    for (start, end) in to_remove {
        // Also remove trailing newlines
        let mut actual_end = end;
        while actual_end < source.len() && source.chars().nth(actual_end) == Some('\n') {
            actual_end += 1;
            if actual_end < source.len() && source.chars().nth(actual_end) != Some('\n') {
                break; // Only remove one newline
            }
        }
        
        result.replace_range(start..actual_end, "");
    }
    
    Ok(result)
}

/// Replace all references to `old_name` with `new_name` in the given body text using AST
fn replace_identifier_in_body(body_text: &str, old_name: &str, new_name: &str) -> Result<String> {
    if old_name.is_empty() {
        return Ok(body_text.to_string());
    }
    
    // Wrap in a function so we can parse it as a valid Rust fragment
    let wrapped = format!("fn dummy() {{ {} }}", body_text);
    let parsed = SourceFile::parse(&wrapped, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        // If parsing fails, fall back to original text
        return Ok(body_text.to_string());
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Collect all PATH_EXPR nodes that reference old_name
    let mut replacements: Vec<(usize, usize)> = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::PATH_EXPR {
            if let Some(path_expr) = ast::PathExpr::cast(node.clone()) {
                if let Some(path) = path_expr.path() {
                    // Check if this is a simple identifier (not qualified like foo::bar)
                    let segments: Vec<_> = path.segments().collect();
                    if segments.len() == 1 {
                        if let Some(segment) = segments.first() {
                            let ident = segment.to_string();
                            if ident == old_name {
                                // Found a reference - record its position in the wrapped source
                                let start: usize = node.text_range().start().into();
                                let end: usize = node.text_range().end().into();
                                replacements.push((start, end));
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Apply replacements from end to start in the wrapped source
    let mut result = wrapped.clone();
    replacements.sort_by_key(|(start, _)| std::cmp::Reverse(*start));
    
    for (start, end) in replacements {
        result.replace_range(start..end, new_name);
    }
    
    // Extract the body back out (remove "fn dummy() { " and " }")
    let prefix = "fn dummy() { ";
    let suffix = " }";
    if result.starts_with(prefix) && result.ends_with(suffix) {
        Ok(result[prefix.len()..result.len() - suffix.len()].to_string())
    } else {
        Ok(body_text.to_string())
    }
}

fn create_trait_impl(source: &str, _analysis: &ModuleAnalysis) -> Result<String> {
    // Step C: Create impl Trait for T block by moving pub fn implementations
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors"));
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find the trait and collect method names
    let mut trait_name: Option<String> = None;
    let mut trait_end_pos: Option<usize> = None;
    let mut method_names: Vec<String> = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::TRAIT {
            if let Some(trait_ast) = ast::Trait::cast(node.clone()) {
                if let Some(name) = trait_ast.name() {
                    trait_name = Some(name.text().to_string());
                    trait_end_pos = Some(node.text_range().end().into());
                    
                    // Collect method names from trait
                    for child in node.descendants() {
                        if child.kind() == SyntaxKind::FN {
                            if let Some(fn_ast) = ast::Fn::cast(child.clone()) {
                                if let Some(fn_name) = fn_ast.name() {
                                    method_names.push(fn_name.text().to_string());
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
    }
    
    let trait_name = trait_name.ok_or_else(|| anyhow::anyhow!("No trait found"))?;
    let trait_end_pos = trait_end_pos.ok_or_else(|| anyhow::anyhow!("No trait position"))?;
    
    // Find all standalone pub fn that match trait method names
    // Store: (name, ret_type, params, first_param_name, body_text)
    let mut impl_methods: Vec<(String, String, Vec<String>, String, String)> = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FN {
            // Check if this function is NOT inside an impl block
            let mut in_impl = false;
            let mut parent = node.parent();
            while let Some(p) = parent {
                if p.kind() == SyntaxKind::IMPL {
                    in_impl = true;
                    break;
                }
                parent = p.parent();
            }
            
            // Only process standalone functions
            if !in_impl {
                if let Some(fn_ast) = ast::Fn::cast(node.clone()) {
                    if let Some(fn_name) = fn_ast.name() {
                        let name = fn_name.text().to_string();
                        
                        // Check if this is a pub fn matching a trait method
                        if method_names.contains(&name) {
                            if let Some(vis) = fn_ast.visibility() {
                                if vis.to_string() == "pub" {
                                    // Get function body (without braces)
                                    if let Some(body) = fn_ast.body() {
                                        // Get body text without outer braces using AST structure
                                        let body_text = if let Some(stmt_list) = body.stmt_list() {
                                            stmt_list.to_string()
                                        } else {
                                            // Fallback to full body
                                            body.to_string()
                                        };
                                        
                                        // Get return type
                                        let ret_type = if let Some(ret) = fn_ast.ret_type() {
                                            ret.to_string()
                                        } else {
                                            String::new()
                                        };
                                        
                                        // Get parameters
                                        let mut params = Vec::new();
                                        let mut first_param_name = String::new();
                                        if let Some(param_list) = fn_ast.param_list() {
                                            for (i, param) in param_list.params().enumerate() {
                                                let param_str = param.to_string();
                                                params.push(param_str.clone());
                                                // Extract first parameter name
                                                if i == 0 {
                                                    if let Some(colon_pos) = param_str.find(':') {
                                                        first_param_name = param_str[..colon_pos].trim().to_string();
                                                    }
                                                }
                                            }
                                        }
                                        
                                        impl_methods.push((name.clone(), ret_type, params, first_param_name, body_text));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Verify that all trait methods have corresponding implementations
    // (Rust requires complete trait implementations - no partial impls allowed)
    if impl_methods.len() != method_names.len() {
        let missing: Vec<String> = method_names.iter()
            .filter(|name| !impl_methods.iter().any(|(impl_name, _, _, _, _)| impl_name == *name))
            .cloned()
            .collect();
        
        eprintln!("Warning: Not all trait methods have standalone pub fn implementations.");
        eprintln!("Missing: {:?}", missing);
        eprintln!("Skipping transformation for this module (needs manual handling).");
        
        return Err(anyhow::anyhow!(
            "Incomplete trait implementation - cannot create impl block"
        ));
    }
    
    if impl_methods.is_empty() {
        return Err(anyhow::anyhow!("No standalone pub fn implementations found for trait methods"));
    }
    
    // Build the impl block
    let mut impl_block = format!("\n\n    impl {} for T {{", trait_name);
    
    for (method_name, ret_type, params, first_param_name, body_text) in impl_methods {
        if params.len() == 1 {
            // Single parameter: replace param name with self in body using AST
            impl_block.push_str(&format!("\n        fn {}(&self){} {{", method_name, ret_type));
            
            // Use AST-based identifier replacement
            let modified_body = replace_identifier_in_body(&body_text, &first_param_name, "self")?;
            
            // Insert the modified function body (braces already removed by AST extraction)
            impl_block.push_str("\n            ");
            impl_block.push_str(&modified_body);
            impl_block.push_str("\n        }");
        } else {
            // Multi-parameter: keep remaining params, replace first param name with self in body using AST
            let remaining_params: Vec<&str> = params.iter().skip(1).map(|s| s.as_str()).collect();
            let params_str = remaining_params.join(", ");
            
            impl_block.push_str(&format!("\n        fn {}(&self, {}){} {{", method_name, params_str, ret_type));
            
            // Use AST-based identifier replacement
            let modified_body = replace_identifier_in_body(&body_text, &first_param_name, "self")?;
            
            impl_block.push_str("\n            ");
            impl_block.push_str(&modified_body);
            impl_block.push_str("\n        }");
        }
    }
    
    impl_block.push_str("\n    }");
    
    // Insert the impl block after the trait
    let mut result = source.to_string();
    result.insert_str(trait_end_pos, &impl_block);
    
    Ok(result)
}

fn transform_algorithm_trait(source: &str, _analysis: &ModuleAnalysis) -> Result<String> {
    // Step B: Transform trait method signatures from fn(n: N) -> R to fn(&self) -> R
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors"));
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Collect all replacements (offset, old_text, new_text)
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    
    // Find the trait and its methods
    for node in root.descendants() {
        if node.kind() == SyntaxKind::TRAIT {
            if let Some(_trait_ast) = ast::Trait::cast(node.clone()) {
                // Find methods in this trait
                for child in node.descendants() {
                    if child.kind() == SyntaxKind::FN {
                        if let Some(fn_ast) = ast::Fn::cast(child.clone()) {
                            if let Some(param_list) = fn_ast.param_list() {
                                let params: Vec<_> = param_list.params().collect();
                                
                                if params.is_empty() {
                                    // Zero parameters: add &self
                                    let param_list_start: usize = param_list.syntax().text_range().start().into();
                                    let param_list_end: usize = param_list.syntax().text_range().end().into();
                                    let new_param_list = "(&self)".to_string();
                                    replacements.push((param_list_start, param_list_end, new_param_list));
                                } else if let Some(first_param) = params.first() {
                                    // Check if first param needs transformation
                                    let param_text = first_param.to_string();
                                    
                                    if !param_text.contains("self") {
                                        if params.len() == 1 {
                                            // Single parameter: replace entire param list with (&self)
                                            let param_list_start: usize = param_list.syntax().text_range().start().into();
                                            let param_list_end: usize = param_list.syntax().text_range().end().into();
                                            let new_param_list = "(&self)".to_string();
                                            replacements.push((param_list_start, param_list_end, new_param_list));
                                        } else {
                                            // Multi-parameter: replace first param with &self, keep rest
                                            let remaining_params: Vec<String> = params.iter().skip(1).map(|p| p.to_string()).collect();
                                            let new_params = format!("&self, {}", remaining_params.join(", "));
                                            
                                            // Replace entire param list
                                            let param_list_start: usize = param_list.syntax().text_range().start().into();
                                            let param_list_end: usize = param_list.syntax().text_range().end().into();
                                            let new_param_list = format!("({})", new_params);
                                            replacements.push((param_list_start, param_list_end, new_param_list));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Apply replacements from end to start (so offsets don't shift)
    let mut result = source.to_string();
    replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    
    for (start, end, new_text) in replacements {
        result.replace_range(start..end, &new_text);
    }
    
    Ok(result)
}

fn fix_unused_self(source: &str, _analysis: &ModuleAnalysis) -> Result<String> {
    let mut result = source.to_string();
    
    // Step 1: Fix trait method signature
    // fn insSort(&self, slice: &mut [T]) -> fn insSort(&mut self)
    result = fix_trait_signature(&result)?;
    
    // Step 2: Fix impl header
    // impl<T: Ord + Clone> InsertionSortStTrait<T> for T
    // -> impl<S: Ord + Clone> InsertionSortStTrait<S> for [S]
    result = fix_impl_header(&result)?;
    
    // Step 3: Fix impl method signature (same as trait)
    result = fix_impl_method_signature(&result)?;
    
    // Step 4: Fix method body - replace slice with self
    result = fix_method_body(&result)?;
    
    Ok(result)
}

fn fix_trait_signature(source: &str) -> Result<String> {
    // HARDCODED for InsertionSortSt
    // Find: fn insSort(&self, slice: &mut [T]);
    // Replace with: fn insSort(&mut self);
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors"));
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find the trait method
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FN {
            if let Some(fn_ast) = ast::Fn::cast(node.clone()) {
                if let Some(name) = fn_ast.name() {
                    if name.text() == "insSort" {
                        // Found it - get its text range
                        let fn_text = node.to_string();
                        
                        // Build replacement: change signature
                        let new_fn_text = fn_text
                            .replace("fn insSort(&self, slice: &mut [T])", "fn insSort(&mut self)");
                        
                        // Replace in source
                        let start: usize = node.text_range().start().into();
                        let end: usize = node.text_range().end().into();
                        
                        let mut result = String::new();
                        result.push_str(&source[..start]);
                        result.push_str(&new_fn_text);
                        result.push_str(&source[end..]);
                        
                        return Ok(result);
                    }
                }
            }
        }
    }
    
    Ok(source.to_string())
}

fn fix_impl_header(source: &str) -> Result<String> {
    // HARDCODED for InsertionSortSt
    // Find: impl<T: Ord + Clone> InsertionSortStTrait<T> for T {
    // Replace with: impl<S: Ord + Clone> InsertionSortStTrait<S> for [S] {
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors"));
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find the impl block for InsertionSortStTrait
    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            let impl_text = node.to_string();
            if impl_text.contains("InsertionSortStTrait") {
                // Get just the header (first line)
                let lines: Vec<&str> = impl_text.lines().collect();
                if let Some(first_line) = lines.first() {
                    if first_line.contains("impl<T") {
                        // Replace the impl header
                        let new_header = first_line
                            .replace("impl<T:", "impl<S:")
                            .replace("InsertionSortStTrait<T>", "InsertionSortStTrait<S>")
                            .replace(" for T {", " for [S] {");
                        
                        // Replace in the full impl text
                        let new_impl = impl_text.replacen(first_line, &new_header, 1);
                        
                        // Replace in source
                        let start: usize = node.text_range().start().into();
                        let end: usize = node.text_range().end().into();
                        
                        let mut result = String::new();
                        result.push_str(&source[..start]);
                        result.push_str(&new_impl);
                        result.push_str(&source[end..]);
                        
                        return Ok(result);
                    }
                }
            }
        }
    }
    
    Ok(source.to_string())
}

fn fix_impl_method_signature(source: &str) -> Result<String> {
    // HARDCODED for InsertionSortSt
    // Same as trait signature but in the impl block
    // This will be the second occurrence of fn insSort
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors"));
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find impl block first, then find fn inside it
    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            let impl_text = node.to_string();
            if impl_text.contains("InsertionSortStTrait") {
                // Find fn insSort inside this impl
                for child in node.descendants() {
                    if child.kind() == SyntaxKind::FN {
                        if let Some(fn_ast) = ast::Fn::cast(child.clone()) {
                            if let Some(name) = fn_ast.name() {
                                if name.text() == "insSort" {
                                    // Found impl method - change signature
                                    let fn_start: usize = child.text_range().start().into();
                                    let fn_end: usize = child.text_range().end().into();
                                    let fn_text = &source[fn_start..fn_end];
                                    
                                    // Change signature and rename T to S throughout
                                    let new_fn_text = fn_text
                                        .replace("fn insSort(&self, slice: &mut [T])", "fn insSort(&mut self)")
                                        .replace("&mut [T]", "&mut [S]")
                                        .replace("[T]", "[S]")
                                        .replace(": T", ": S")
                                        .replace("<T>", "<S>")
                                        .replace("(T", "(S");
                                    
                                    let mut result = String::new();
                                    result.push_str(&source[..fn_start]);
                                    result.push_str(&new_fn_text);
                                    result.push_str(&source[fn_end..]);
                                    
                                    return Ok(result);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(source.to_string())
}

fn fix_method_body(source: &str) -> Result<String> {
    // HARDCODED for InsertionSortSt
    // Replace all occurrences of `slice` with `self` in the method body
    // But only within the insSort method
    
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors"));
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find the impl method body using AST
    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            if let Some(impl_ast) = ast::Impl::cast(node.clone()) {
                // Check if this impl is for InsertionSortStTrait using AST
                if let Some(trait_type) = impl_ast.trait_() {
                    // Cast the Type to PathType to access the path
                    if let ast::Type::PathType(path_type) = trait_type {
                        if let Some(path) = path_type.path() {
                            if let Some(segment) = path.segment() {
                                if let Some(name_ref) = segment.name_ref() {
                                    if name_ref.text() == "InsertionSortStTrait" {
                                    // Find fn insSort inside this impl
                                    for child in node.descendants() {
                                        if child.kind() == SyntaxKind::FN {
                                            if let Some(fn_ast) = ast::Fn::cast(child.clone()) {
                                                if let Some(name) = fn_ast.name() {
                                                    if name.text() == "insSort" {
                                                        // Found it - replace slice with self in body using AST
                                                        
                                                        // Use AST-based replacement for slice -> self
                                                        // Extract just the body
                                                        if let Some(body) = fn_ast.body() {
                                                            if let Some(stmt_list) = body.stmt_list() {
                                                                let body_text = stmt_list.to_string();
                                                                let modified_body = replace_identifier_in_body(&body_text, "slice", "self")?;
                                                                
                                                                // Reconstruct the function with the new body
                                                                let body_start: usize = stmt_list.syntax().text_range().start().into();
                                                                let body_end: usize = stmt_list.syntax().text_range().end().into();
                                                                
                                                                let mut result = String::new();
                                                                result.push_str(&source[..body_start]);
                                                                result.push_str(&modified_body);
                                                                result.push_str(&source[body_end..]);
                                                                
                                                                return Ok(result);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    }
                }
            }
        }
    }
    
    Ok(source.to_string())
}

fn fix_unused_self_calls(source: &str) -> Result<String> {
    // HARDCODED for InsertionSortSt
    // Pattern: <receiver>.insSort(&mut <identifier>) -> <identifier>.insSort()
    // 
    // Generic version would need to:
    // - Receive list of method names with unused self from analysis
    // - Use AST to find all call sites for those methods
    // - Extract the actual data parameter from each call
    // - Replace the entire call expression
    
    // Use AST to find and transform call sites
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    if !parsed.errors().is_empty() {
        return Err(anyhow::anyhow!("Parse errors"));
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Collect all METHOD_CALL_EXPR nodes for insSort
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::METHOD_CALL_EXPR {
            if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
                if let Some(name_ref) = call.name_ref() {
                    if name_ref.text() == "insSort" {
                        // Found an insSort call - extract argument
                        if let Some(arg_list) = call.arg_list() {
                            let args: Vec<_> = arg_list.args().collect();
                            if args.len() == 1 {
                                // Get the first argument
                                if let Some(first_arg) = args.first() {
                                    // Extract the identifier from &mut identifier or &identifier
                                    let arg_text = first_arg.to_string();
                                    
                                    let identifier = if arg_text.starts_with("&mut ") {
                                        arg_text.trim_start_matches("&mut ").trim()
                                    } else if arg_text.starts_with("&") {
                                        arg_text.trim_start_matches("&").trim()
                                    } else {
                                        arg_text.trim()
                                    };
                                    
                                    // Build new call: identifier.insSort()
                                    let new_call = format!("{}.insSort()", identifier);
                                    
                                    // Record replacement for entire method call expression
                                    let start: usize = node.text_range().start().into();
                                    let end: usize = node.text_range().end().into();
                                    replacements.push((start, end, new_call));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Apply replacements from end to start
    let mut result = source.to_string();
    replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    
    for (start, end, new_text) in replacements {
        result.replace_range(start..end, &new_text);
    }
    
    Ok(result)
}

fn get_line_number(source: &str, byte_offset: usize) -> usize {
    source[..byte_offset].lines().count()
}
