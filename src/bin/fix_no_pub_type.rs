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
use ra_ap_syntax::{ast::{self, AstNode, HasVisibility, HasName}, SyntaxKind, SourceFile, Edition};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use rusticate::StandardArgs;

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    
    if args.paths.len() != 1 {
        eprintln!("Error: fix-no-pub-type requires exactly one file path");
        eprintln!("Usage: rusticate-fix-no-pub-type -f path/to/Module.rs");
        std::process::exit(1);
    }
    
    let file_path = &args.paths[0];
    let base_dir = args.base_dir();
    
    println!("Entering directory '{}'", base_dir.display());
    println!();
    
    // Step 1: Analyze the module to determine the transformation
    let source = fs::read_to_string(file_path)?;
    let analysis = analyze_module(&source, file_path)?;
    
    // Check if this has unused self - that requires the complex transformation
    if analysis.has_unused_self && analysis.module_name != "InsertionSortSt" {
        eprintln!("Error: Module has unused self parameter - complex transformation not yet supported");
        eprintln!("Module: {}", analysis.module_name);
        eprintln!("Unused self method: {:?}", analysis._unused_self_method);
        eprintln!();
        eprintln!("Only InsertionSortSt prototype implemented for unused self transformation.");
        eprintln!("Simple pub type addition works for modules without unused self.");
        std::process::exit(1);
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
    
    // Steps B-D: For algorithm modules (T = N pattern), transform if needed
    let current_source = fs::read_to_string(file_path)?;
    if analysis.recommended_type.contains("pub type T = N") && has_standalone_pub_fn(&current_source, &analysis)? {
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
    
    println!();
    println!("Completed in {}ms", start.elapsed().as_millis());
    
    Ok(())
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
    
    // Find module name
    let module_name = if let Some(start) = source.find("pub mod ") {
        let rest = &source[start + 8..];
        if let Some(end) = rest.find(" {") {
            rest[..end].trim().to_string()
        } else {
            "Unknown".to_string()
        }
    } else {
        "Unknown".to_string()
    };
    
    let module_line = source[..source.find("pub mod ").unwrap_or(0)].lines().count() + 1;
    
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
    // Look for impl blocks and extract parameter types
    let mut has_unused_self = false;
    let mut proposed_type: Option<String> = None;
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::IMPL {
            // Extract "for Type" part
            let impl_text = node.to_string();
            
            // First check for impl Trait for ExternalType (like AtomicUsize)
            if impl_text.contains(" for ") {
                // Extract the type after "for"
                if let Some(for_pos) = impl_text.find(" for ") {
                    let after_for = &impl_text[for_pos + 5..]; // Skip " for "
                    
                    // Find the type name (until { or <)
                    let type_end = after_for.find('{')
                        .or_else(|| after_for.find('<'))
                        .unwrap_or(after_for.len());
                    let type_name = after_for[..type_end].trim();
                    
                    // If it's not just "T" or "T{", it's an external type
                    if type_name != "T" && !type_name.is_empty() {
                        // Check if it's a concrete type (starts with uppercase, or contains ::)
                        if type_name.chars().next().map_or(false, |c| c.is_uppercase()) || type_name.contains("::") {
                            proposed_type = Some(format!("pub type T = {};", type_name));
                            break;
                        }
                    }
                }
            }
            
            // Check if this is impl<T> ... for T pattern
            if impl_text.contains(" for T ") || impl_text.contains(" for T{") {
                // Look at method parameters to find the actual data type
                for child in node.descendants() {
                    if child.kind() == SyntaxKind::FN {
                        if let Some(fn_ast) = ast::Fn::cast(child.clone()) {
                            if let Some(param_list) = fn_ast.param_list() {
                                for param in param_list.params() {
                                    let param_text = param.to_string();
                                    // Skip &self parameters
                                    if param_text.contains("&self") || param_text == "self" {
                                        // Check if self is unused (simple heuristic: look for method body)
                                        if let Some(body) = fn_ast.body() {
                                            let body_text = body.to_string();
                                            // Very simple check: if body mentions the second parameter name
                                            // and method has &self, it might have unused self
                                            if param_text.contains("&self") && !body_text.contains("self") {
                                                has_unused_self = true;
                                            }
                                        }
                                        continue;
                                    }
                                    
                                    // Extract type from parameter (e.g., "slice: &mut [T]")
                                    if let Some(colon_pos) = param_text.find(':') {
                                        let type_part = param_text[colon_pos + 1..].trim();
                                        
                                        // Extract the base type
                                        if type_part.starts_with("&mut ") {
                                            let base = &type_part[5..];
                                            proposed_type = Some(format!("pub type T<S> = {};", base));
                                        } else if type_part.starts_with("&") {
                                            let base = &type_part[1..].trim();
                                            proposed_type = Some(format!("pub type T<S> = {};", base));
                                        } else {
                                            proposed_type = Some(format!("pub type T<S> = {};", type_part));
                                        }
                                        break;
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
                let use_text = node.to_string();
                if use_text.contains("Types::Types::*") {
                    has_types_import = true;
                    break;
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
    
    let new_source = if analysis.has_unused_self {
        // InsertionSortSt pattern: change receiver.method(&mut data) to data.method()
        fix_unused_self_calls(&source)?
    } else if analysis.recommended_type.contains("pub type T = N") {
        // Algorithm module pattern: change function_name(n) to n.function_name()
        fix_algorithm_call_sites(&source, analysis)?
    } else {
        // No transformation needed
        return Ok(());
    };
    
    fs::write(file_path, new_source)?;
    
    Ok(())
}

fn fix_algorithm_call_sites(source: &str, analysis: &ModuleAnalysis) -> Result<String> {
    // Transform function_name(n) to n.function_name()
    // Extract method names from the module's trait definition
    
    let module_source = fs::read_to_string(&analysis.source_file)?;
    let method_names = extract_trait_method_names_from_source(&module_source)?;
    
    if method_names.is_empty() {
        return Ok(source.to_string());
    }
    
    let mut result = source.to_string();
    
    // For each method name, find and replace call sites
    for method_name in &method_names {
        // Pattern: method_name(arg) -> arg.method_name()
        // Use a simple regex-like approach, but with AST validation
        let pattern = format!("{}(", method_name);
        let mut replacements: Vec<(usize, usize, String)> = Vec::new();
        
        let mut idx = 0;
        while let Some(pos) = result[idx..].find(&pattern) {
            let actual_pos = idx + pos;
            
            // Check if this is a function call (not a method call)
            let before = &result[..actual_pos];
            let is_method_call = before.ends_with('.');
            
            if !is_method_call {
                // Find the matching closing paren and extract the argument
                if let Some((arg, end_pos)) = extract_function_arg(&result[actual_pos + pattern.len()..]) {
                    let full_end = actual_pos + pattern.len() + end_pos + 1; // +1 for closing paren
                    let replacement = format!("{}.{}()", arg, method_name);
                    replacements.push((actual_pos, full_end, replacement));
                    idx = full_end;
                    continue;
                }
            }
            
            idx = actual_pos + 1;
        }
        
        // Apply replacements from end to start
        eprintln!("DEBUG: Found {} replacements for method '{}'", replacements.len(), method_name);
        replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
        for (start, end, replacement) in replacements {
            eprintln!("DEBUG: Replacing {}..{} with '{}'", start, end, replacement);
            result.replace_range(start..end, &replacement);
        }
    }
    
    Ok(result)
}

fn extract_function_arg(s: &str) -> Option<(String, usize)> {
    // Extract the argument from a function call: "5)" -> ("5", 1)
    // Handle simple cases: literals, identifiers, simple expressions
    
    let mut depth = 0;
    let mut arg = String::new();
    
    for (i, ch) in s.chars().enumerate() {
        match ch {
            '(' => {
                depth += 1;
                arg.push(ch);
            }
            ')' => {
                if depth == 0 {
                    return Some((arg.trim().to_string(), i));
                }
                depth -= 1;
                arg.push(ch);
            }
            _ => arg.push(ch),
        }
    }
    
    None
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
    let mut impl_methods = Vec::new();
    
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
                                    // Get function body
                                    if let Some(body) = fn_ast.body() {
                                        let body_text = body.to_string();
                                        
                                        // Get return type
                                        let ret_type = if let Some(ret) = fn_ast.ret_type() {
                                            ret.to_string()
                                        } else {
                                            String::new()
                                        };
                                        
                                        impl_methods.push((name.clone(), ret_type, body_text));
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
            .filter(|name| !impl_methods.iter().any(|(impl_name, _, _)| impl_name == *name))
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
    
    for (method_name, ret_type, body_text) in impl_methods {
        impl_block.push_str(&format!("\n        fn {}(&self){} {{", method_name, ret_type));
        impl_block.push_str("\n            let n = *self;");
        
        // Insert the original function body (strip outer braces)
        let body_inner = body_text.trim().trim_start_matches('{').trim_end_matches('}');
        impl_block.push_str("\n            ");
        impl_block.push_str(body_inner);
        impl_block.push_str("\n        }");
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
                                
                                // Check if first param is "n: N" or similar
                                if let Some(first_param) = params.first() {
                                    let param_text = first_param.to_string();
                                    
                                        // Pattern: "n: N" or "n: T" - replace with "&self"
                                        if (param_text.contains(": N") || param_text.contains(": T")) 
                                            && !param_text.contains("self") {
                                            
                                            // Get the parameter list node directly
                                            let param_list_start: usize = param_list.syntax().text_range().start().into();
                                            let param_list_end: usize = param_list.syntax().text_range().end().into();
                                            
                                            // Replace just the parameter list with (&self)
                                            let new_param_list = "(&self)".to_string();
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
    
    // Find the impl method body
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
                                    // Found it - replace slice with self in body
                                    let fn_start: usize = child.text_range().start().into();
                                    let fn_end: usize = child.text_range().end().into();
                                    let fn_text = &source[fn_start..fn_end];
                                    
                                    // Replace slice with self (word boundaries)
                                    let new_fn_text = fn_text
                                        .replace("slice.len()", "self.len()")
                                        .replace("slice[", "self[");
                                    
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

fn fix_unused_self_calls(source: &str) -> Result<String> {
    // HARDCODED for InsertionSortSt
    // Pattern: <receiver>.insSort(&mut <identifier>) -> <identifier>.insSort()
    // 
    // Generic version would need to:
    // - Receive list of method names with unused self from analysis
    // - Use AST to find all call sites for those methods
    // - Extract the actual data parameter from each call
    // - Replace the entire call expression
    
    let mut result = source.to_string();
    
    // Pattern to match: anything.insSort(&mut identifier)
    // We'll use a loop to replace all occurrences
    loop {
        if let Some(pos) = result.find(".insSort(&mut ") {
            // Find the start of this expression (work backwards to whitespace/delimiters)
            let before = &result[..pos];
            let mut expr_start = pos;
            
            // Find start of receiver expression (after last delimiter)
            for (i, ch) in before.char_indices().rev() {
                match ch {
                    ' ' | '\t' | '\n' | ';' | '{' | '(' | ',' => {
                        expr_start = i + ch.len_utf8();
                        break;
                    }
                    _ => {
                        if i == 0 {
                            expr_start = 0;
                        }
                    }
                }
            }
            
            // Find the argument (after "&mut ")
            let arg_start = pos + ".insSort(&mut ".len();
            let after = &result[arg_start..];
            
            // Find the end of the argument (until ')')
            if let Some(close_paren) = after.find(')') {
                let arg = after[..close_paren].trim();
                
                // Calculate exact positions
                let end_pos = arg_start + close_paren + 1; // +1 for ')'
                
                // Build replacement: arg.insSort()
                let replacement = format!("{}.insSort()", arg);
                
                // Replace just the call part, preserving indentation
                let indentation = &result[expr_start..pos];
                let full_replacement = if indentation.trim().is_empty() {
                    format!("{}{}", indentation, replacement)
                } else {
                    replacement
                };
                
                result.replace_range(expr_start..end_pos, &full_replacement);
            } else {
                // Malformed, skip this one
                break;
            }
        } else {
            // No more matches
            break;
        }
    }
    
    Ok(result)
}

