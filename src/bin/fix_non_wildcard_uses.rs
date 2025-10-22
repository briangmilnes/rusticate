use ra_ap_syntax::{ast::{self, AstNode}, Edition, SyntaxKind};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::fs;
use regex::Regex;

fn fix_file(file_path: &PathBuf) -> Result<usize, Box<dyn std::error::Error>> {
    // Skip TestLibIntegration.rs - it tests module structure and should not be modified
    if file_path.ends_with("TestLibIntegration.rs") {
        return Ok(0);
    }
    
    let content = fs::read_to_string(file_path)?;
    let parse = ra_ap_syntax::SourceFile::parse(&content, Edition::Edition2021);
    let root = parse.syntax_node();
    
    // Find all macro invocations in the file (e.g., SomethingLit![...] or SomethingLit!(...))
    let macro_pattern = Regex::new(r"\b([A-Z][A-Za-z0-9]*Lit)!\s*[\[\(]").unwrap();
    let mut macros_used: HashSet<String> = HashSet::new();
    for cap in macro_pattern.captures_iter(&content) {
        macros_used.insert(cap[1].to_string());
    }
    
    // Find already-imported macros
    let mut macros_already_imported: HashSet<String> = HashSet::new();
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                let use_text = use_item.to_string().trim().to_string();
                if use_text.contains("Lit") && use_text.contains("apas_ai::") {
                    // Extract macro name from use statement
                    // e.g., "use apas_ai::AVLTreeSetStEphLit;" -> "AVLTreeSetStEphLit"
                    if let Some(macro_name) = use_text
                        .trim_start_matches("use apas_ai::")
                        .trim_end_matches(';')
                        .trim()
                        .split("::")
                        .last() {
                        macros_already_imported.insert(macro_name.to_string());
                    }
                }
            }
        }
    }
    
    // First, check for and remove bogus top-level wildcard "use apas_ai::*;"
    let mut has_toplevel_wildcard = false;
    let mut content_working = content.clone();
    let mut toplevel_wildcard_line = String::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                let use_text = use_item.to_string().trim().to_string();
                
                // Check for exactly "use apas_ai::*;"
                if use_text == "use apas_ai::*;" {
                    has_toplevel_wildcard = true;
                    toplevel_wildcard_line = use_text.clone();
                    // Don't replace yet - we may need to add macro imports here
                    break;
                }
            }
        }
    }
    
    // If we're removing the top-level wildcard, add any missing macro imports
    if has_toplevel_wildcard {
        let mut macros_to_import: Vec<String> = macros_used
            .difference(&macros_already_imported)
            .cloned()
            .collect();
        
        // Sort for deterministic output
        macros_to_import.sort();
        
        if !macros_to_import.is_empty() {
            // Replace "use apas_ai::*;" with explicit macro imports
            let mut macro_imports = String::new();
            for (i, macro_name) in macros_to_import.iter().enumerate() {
                if i > 0 {
                    macro_imports.push('\n');
                }
                macro_imports.push_str(&format!("use apas_ai::{};", macro_name));
            }
            content_working = content_working.replace(&toplevel_wildcard_line, &macro_imports);
        } else {
            // Just remove it
            content_working = content_working.replace(&toplevel_wildcard_line, "");
        }
    }
    
    // Re-parse if we removed the top-level wildcard
    let parse = if has_toplevel_wildcard {
        ra_ap_syntax::SourceFile::parse(&content_working, Edition::Edition2021)
    } else {
        parse
    };
    let root = parse.syntax_node();
    
    // Find all non-wildcard imports from apas_ai modules
    let mut imports_to_replace: Vec<(String, String)> = Vec::new(); // (old_import, module_path)
    let mut module_to_imports: HashMap<String, Vec<String>> = HashMap::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                let use_text = use_item.to_string().trim().to_string();
                
                // Check if it's importing from apas_ai and NOT a wildcard
                if use_text.contains("apas_ai::") && !use_text.trim_end_matches(';').trim_end().ends_with("::*") {
                    // Skip grouped imports like use apas_ai::{Foo, Bar};
                    // These are typically macro imports and should be left alone
                    if use_text.contains('{') {
                        continue;
                    }
                    
                    // Skip macro imports
                    if use_text.contains("Lit") {
                        let parts: Vec<&str> = use_text.split("::").collect();
                        if let Some(last) = parts.last() {
                            if last.trim_end_matches(';').trim().ends_with("Lit") {
                                continue; // Skip macro imports
                            }
                        }
                    }
                    
                    // Skip imports with "as" renames - these are legitimate type aliases
                    if use_text.contains(" as ") {
                        continue;
                    }
                    
                    // Extract module path (everything up to the second-to-last ::)
                    if let Some(module_path) = extract_module_path(&use_text) {
                        // Skip imports from top-level apas_ai (typically macros)
                        // e.g., use apas_ai::prob; -> module_path would be "apas_ai"
                        if module_path == "apas_ai" {
                            continue;
                        }
                        
                        module_to_imports.entry(module_path.clone())
                            .or_insert_with(Vec::new)
                            .push(use_text.clone());
                        imports_to_replace.push((use_text, module_path));
                    }
                }
            }
        }
    }
    
    // Count if we removed toplevel wildcard
    let mut fixed_count = if has_toplevel_wildcard { 1 } else { 0 };
    
    if imports_to_replace.is_empty() {
        if has_toplevel_wildcard {
            fs::write(file_path, content_working)?;
        }
        return Ok(fixed_count);
    }
    
    // For each module, replace all its imports with a single wildcard import
    let mut new_content = content_working.clone();
    let mut replaced_modules = HashSet::new();
    
    for (module_path, old_imports) in &module_to_imports {
        if replaced_modules.contains(module_path) {
            continue;
        }
        
        let wildcard_import = format!("use {}::*;", module_path);
        
        // Replace the first import with the wildcard
        if let Some(first_import) = old_imports.first() {
            new_content = new_content.replace(first_import, &wildcard_import);
            
            // Remove the rest of the imports for this module
            for old_import in old_imports.iter().skip(1) {
                // Replace with empty line to maintain line numbers for other changes
                new_content = new_content.replace(old_import, "");
            }
        }
        
        replaced_modules.insert(module_path.clone());
    }
    
    // APAS-specific: Remove Chap18 wildcard imports when Chap19 equivalent exists
    // Chap19 re-exports everything from Chap18, so Chap18 imports cause ambiguity
    let mut chap19_wildcards: HashSet<String> = HashSet::new();
    let mut chap18_to_remove: Vec<String> = Vec::new();
    
    // Re-parse to find wildcard imports
    let parse_final = ra_ap_syntax::SourceFile::parse(&new_content, Edition::Edition2021);
    let root_final = parse_final.syntax_node();
    
    for node in root_final.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                let use_text = use_item.to_string().trim().to_string();
                
                // Track Chap19 wildcard imports (e.g., "use apas_ai::Chap19::ArraySeqMtEph::ArraySeqMtEph::*;")
                if use_text.contains("apas_ai::Chap19::") && use_text.ends_with("::*;") {
                    // Extract module name (e.g., "ArraySeqMtEph")
                    if let Some(module_name) = extract_chap_module_name(&use_text) {
                        chap19_wildcards.insert(module_name);
                    }
                }
            }
        }
    }
    
    // Find Chap18 wildcards that have Chap19 equivalents
    for node in root_final.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                let use_text = use_item.to_string().trim().to_string();
                
                // Check for Chap18 wildcard imports
                if use_text.contains("apas_ai::Chap18::") && use_text.ends_with("::*;") {
                    if let Some(module_name) = extract_chap_module_name(&use_text) {
                        // If Chap19 has this module, mark Chap18 import for removal
                        if chap19_wildcards.contains(&module_name) {
                            chap18_to_remove.push(use_text);
                        }
                    }
                }
            }
        }
    }
    
    // Remove redundant Chap18 imports
    let mut final_content = new_content.clone();
    for import_to_remove in &chap18_to_remove {
        final_content = final_content.replace(import_to_remove, "");
        fixed_count += 1;
    }
    
    // Only write if content actually changed
    if final_content != content_working {
        fs::write(file_path, final_content)?;
        Ok(fixed_count + replaced_modules.len())
    } else {
        Ok(fixed_count)
    }
}

fn extract_chap_module_name(use_text: &str) -> Option<String> {
    // Extract module name from "use apas_ai::ChapXX::ModuleName::ModuleName::*;"
    // Returns "ModuleName"
    let parts: Vec<&str> = use_text
        .trim_start_matches("use ")
        .trim_end_matches("::*;")
        .trim()
        .split("::")
        .collect();
    
    // Pattern: apas_ai :: ChapXX :: ModuleName :: ModuleName
    if parts.len() >= 4 && (parts[1].starts_with("Chap18") || parts[1].starts_with("Chap19")) {
        Some(parts[2].to_string())
    } else {
        None
    }
}

fn extract_module_path(use_text: &str) -> Option<String> {
    // Extract module path from use statement
    // e.g., "use apas_ai::Chap37::Foo::Foo::Bar;" -> "apas_ai::Chap37::Foo::Foo"
    
    let cleaned = use_text
        .trim_start_matches("use ")
        .trim_end_matches(';')
        .trim();
    
    // Handle braces: use apas_ai::Foo::Foo::{Bar, Baz};
    let cleaned = if let Some(pos) = cleaned.find('{') {
        &cleaned[..pos]
    } else {
        cleaned
    };
    
    // Handle "as" renames: use apas_ai::Foo::Foo::Bar as Baz;
    let cleaned = if let Some(pos) = cleaned.find(" as ") {
        &cleaned[..pos]
    } else {
        cleaned
    };
    
    let parts: Vec<&str> = cleaned.split("::").collect();
    
    // We want everything except the last component
    // e.g., apas_ai::Chap37::Foo::Foo::Bar -> apas_ai::Chap37::Foo::Foo
    if parts.len() > 1 {
        let module_parts = &parts[..parts.len() - 1];
        Some(module_parts.join("::"))
    } else {
        None
    }
}

fn main() {
    let args = match rusticate::StandardArgs::parse() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    
    let start = std::time::Instant::now();
    
    let files = rusticate::find_rust_files(&args.paths);
    
    let mut total_files_modified = 0;
    let mut total_modules_fixed = 0;
    
    for file_path in &files {
        match fix_file(file_path) {
            Ok(modules_fixed) => {
                if modules_fixed > 0 {
                    total_files_modified += 1;
                    total_modules_fixed += modules_fixed;
                    
                    // Make path relative to CWD
                    let cwd = std::env::current_dir().ok();
                    let display_path = if let Some(ref cwd) = cwd {
                        file_path.strip_prefix(cwd)
                            .unwrap_or(file_path)
                            .display()
                            .to_string()
                    } else {
                        file_path.display().to_string()
                    };
                    
                    println!("{}:{}: fixed {} module(s)", display_path, 0, modules_fixed);
                }
            }
            Err(e) => {
                eprintln!("Error processing {}: {}", file_path.display(), e);
            }
        }
    }
    
    println!();
    println!("{}", "-".repeat(80));
    println!("Files modified: {}", rusticate::format_number(total_files_modified));
    println!("Modules converted to wildcards: {}", rusticate::format_number(total_modules_fixed));
    println!("Completed in {}ms", start.elapsed().as_millis());
    
    if total_files_modified > 0 {
        std::process::exit(1);
    } else {
        std::process::exit(0);
    }
}

