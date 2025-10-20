// Copyright (C) Brian G. Milnes 2025

//! Fix: Add pub type and fix unused self parameters
//! 
//! PROTOTYPE: Currently hardcoded for InsertionSortSt pattern only.
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
    let analysis = analyze_module(&source)?;
    
    // PROTOTYPE: Only works for InsertionSortSt
    if analysis.module_name != "InsertionSortSt" {
        eprintln!("Error: This prototype only supports InsertionSortSt module");
        eprintln!("Module: {}", analysis.module_name);
        eprintln!();
        eprintln!("To make this generic, the tool needs to:");
        eprintln!("  1. Extract trait and method names from AST");
        eprintln!("  2. Detect type parameter patterns");
        eprintln!("  3. Find the actual data parameter");
        eprintln!("  4. Apply transformations based on analysis");
        std::process::exit(1);
    }
    
    if analysis.needs_pub_type {
        println!("{}:{}:\tAdding pub type: {}", 
            file_path.display(), analysis.module_line, analysis.recommended_type);
        
        // Step 2: Transform the module file
        let mut new_source = add_pub_type(&source, &analysis)?;
        fs::write(file_path, &new_source)?;
        println!("{}:{}:\tAdded pub type", file_path.display(), analysis.module_line);
        
        // Step 3: Fix unused self if needed
        if analysis.has_unused_self {
            println!("{}:{}:\tFixing unused self parameter", file_path.display(), analysis.module_line);
            new_source = fs::read_to_string(file_path)?;
            new_source = fix_unused_self(&new_source, &analysis)?;
            fs::write(file_path, &new_source)?;
            println!("{}:{}:\tFixed method signatures and body", file_path.display(), analysis.module_line);
        }
        
        // Step 4: Find and fix test file if it exists
        match find_test_file(file_path)? {
            Some(test_file) => {
                println!("{}:1:\tUpdating test call sites", test_file.display());
                fix_call_sites(&test_file, &analysis)?;
                println!("{}:1:\tUpdated test call sites", test_file.display());
            }
            None => {}
        }
        
        // Step 5: Find and fix bench file if it exists
        if let Some(bench_file) = find_bench_file(file_path)? {
            println!("{}:1:\tUpdating bench call sites", bench_file.display());
            fix_call_sites(&bench_file, &analysis)?;
            println!("{}:1:\tUpdated bench call sites", bench_file.display());
        }
    } else {
        println!("{}:{}:\tNo pub type needed", file_path.display(), analysis.module_line);
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
}

fn analyze_module(source: &str) -> Result<ModuleAnalysis> {
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
    
    // Check if pub type exists
    let root = tree.syntax();
    let has_pub_type = root.descendants()
        .filter(|node| node.kind() == SyntaxKind::TYPE_ALIAS)
        .any(|node| {
            if let Some(type_alias) = ast::TypeAlias::cast(node) {
                type_alias.visibility().is_some()
            } else {
                false
            }
        });
    
    if has_pub_type {
        return Ok(ModuleAnalysis {
            module_name,
            module_line,
            needs_pub_type: false,
            recommended_type: String::new(),
            has_unused_self: false,
            _unused_self_method: None,
        });
    }
    
    // For now, hardcode InsertionSortSt transformation
    // TODO: Extract this from actual analysis like review-typeclasses does
    let is_insertion_sort = module_name == "InsertionSortSt";
    let recommended_type = if is_insertion_sort {
        "pub type T<S> = [S];"
    } else {
        "pub type T = ();"
    };
    
    Ok(ModuleAnalysis {
        module_name,
        module_line,
        needs_pub_type: true,
        recommended_type: recommended_type.to_string(),
        has_unused_self: is_insertion_sort,
        _unused_self_method: if is_insertion_sort {
            Some("insSort".to_string())
        } else {
            None
        },
    })
}

fn add_pub_type(source: &str, analysis: &ModuleAnalysis) -> Result<String> {
    // Find the position after "pub mod Name {"
    let mod_start = source.find(&format!("pub mod {} {{", analysis.module_name))
        .ok_or_else(|| anyhow::anyhow!("Could not find module declaration"))?;
    
    let brace_pos = source[mod_start..].find('{')
        .ok_or_else(|| anyhow::anyhow!("Could not find opening brace"))?;
    
    let insert_pos = mod_start + brace_pos + 1;
    
    // Find the next non-empty line to match indentation
    let after_brace = &source[insert_pos..];
    let next_newline = after_brace.find('\n').unwrap_or(after_brace.len());
    
    // Skip to next line after brace
    let next_line_start = next_newline + 1;
    
    // Find first non-empty line to get indentation
    let remaining = if next_line_start < after_brace.len() {
        &after_brace[next_line_start..]
    } else {
        ""
    };
    
    let indent = if let Some(first_line) = remaining.lines().find(|l| !l.trim().is_empty()) {
        first_line.chars().take_while(|c| c.is_whitespace()).collect::<String>()
    } else {
        "    ".to_string()
    };
    
    // Build the insertion: newline + indent + type + double newline
    let insertion = format!("\n{}{}\n", indent, analysis.recommended_type);
    
    // Insert the type
    let before = &source[..insert_pos];
    let after = &source[insert_pos..];
    
    Ok(format!("{}{}{}", before, insertion, after))
}

fn find_test_file(src_file: &Path) -> Result<Option<PathBuf>> {
    // src/ChapXX/ModuleName.rs -> tests/ChapXX/TestModuleName.rs
    let file_stem = src_file.file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid file name"))?;
    
    let parent = src_file.parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid parent directory"))?;
    
    let project_root = src_file.ancestors().nth(3)
        .ok_or_else(|| anyhow::anyhow!("Could not find project root"))?;
    
    let test_file = project_root
        .join("tests")
        .join(parent)
        .join(format!("Test{}.rs", file_stem));
    
    if test_file.exists() {
        Ok(Some(test_file))
    } else {
        Ok(None)
    }
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

fn fix_call_sites(file_path: &Path, _analysis: &ModuleAnalysis) -> Result<()> {
    let source = fs::read_to_string(file_path)?;
    
    // Generic transformation: change receiver.method(&mut data) to data.method()
    // This handles any method with the "unused self" pattern
    let new_source = fix_unused_self_calls(&source)?;
    fs::write(file_path, new_source)?;
    
    Ok(())
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

