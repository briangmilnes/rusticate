use ra_ap_syntax::{ast::{self, AstNode}, Edition, SyntaxKind};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::fs;
use regex::Regex;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/fix_non_wildcard_uses.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
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
                if let Some(use_tree) = use_item.use_tree() {
                    if let Some(path) = use_tree.path() {
                        let path_str = path.to_string();
                        // Check if this is an apas_ai macro import (ends with "Lit")
                        if path_str.starts_with("apas_ai::") && path_str.ends_with("Lit") {
                            // Extract the last segment (macro name)
                            if let Some(last_segment) = path.segment() {
                                let macro_name = last_segment.to_string();
                                macros_already_imported.insert(macro_name);
                            }
                        }
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
                macro_imports.push_str(&format!("use apas_ai::{macro_name};"));
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
    
    // First, find all existing wildcard imports to avoid conflicts
    let mut existing_wildcards: HashSet<String> = HashSet::new();
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                if let Some(use_tree) = use_item.use_tree() {
                    if use_tree.star_token().is_some() {
                        if let Some(path) = use_tree.path() {
                            let path_str = path.to_string();
                            // Store the full wildcard path
                            existing_wildcards.insert(path_str);
                        }
                    }
                }
            }
        }
    }
    
    // Find all non-wildcard imports from apas_ai modules
    let mut imports_to_replace: Vec<(String, String)> = Vec::new(); // (old_import, module_path)
    let mut module_to_imports: HashMap<String, Vec<String>> = HashMap::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                if let Some(use_tree) = use_item.use_tree() {
                    let use_text = use_item.to_string().trim().to_string();
                    
                    // Skip grouped imports like use apas_ai::{Foo, Bar};
                    if use_tree.use_tree_list().is_some() {
                        continue;
                    }
                    
                    // Check if it has a star (wildcard)
                    let has_star = use_tree.star_token().is_some();
                    
                    if let Some(path) = use_tree.path() {
                        let path_str = path.to_string();
                        
                        // Check if it's importing from apas_ai and NOT a wildcard
                        if path_str.starts_with("apas_ai::") && !has_star {
                            // Skip macro imports (path contains "Lit")
                            if path_str.contains("Lit") {
                                continue;
                            }
                            
                            // Skip imports with "as" renames
                            if use_tree.rename().is_some() {
                                continue;
                            }
                            
                            // Extract module path using AST
                            if let Some(module_path) = extract_module_path_from_path(&path) {
                                // Skip imports from top-level apas_ai (typically macros)
                                if module_path == "apas_ai" {
                                    continue;
                                }
                                
                                // APAS-specific: Skip Chap18 imports if there's already a Chap19 wildcard
                                // e.g., skip "use apas_ai::Chap18::ArraySeqMtEph::ArraySeqMtEph::SomeTrait" 
                                // if "apas_ai::Chap19::ArraySeqMtEph::ArraySeqMtEph" wildcard exists
                                if module_path.contains("::Chap18::") {
                                    let chap19_equivalent = module_path.replace("::Chap18::", "::Chap19::");
                                    if existing_wildcards.contains(&chap19_equivalent) {
                                        // Skip this import - it's covered by Chap19 wildcard
                                        continue;
                                    }
                                }
                                
                                module_to_imports.entry(module_path.clone())
                                    .or_default()
                                    .push(use_text.clone());
                                imports_to_replace.push((use_text, module_path));
                            }
                        }
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
        
        let wildcard_import = format!("use {module_path}::*;");
        
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
                if let Some(use_tree) = use_item.use_tree() {
                    // Check if this is a wildcard import
                    if use_tree.star_token().is_some() {
                        if let Some(path) = use_tree.path() {
                            let path_str = path.to_string();
                            // Track Chap19 wildcard imports (e.g., "apas_ai::Chap19::ArraySeqMtEph::ArraySeqMtEph")
                            if path_str.starts_with("apas_ai::Chap19::") {
                                // Extract module name using AST
                                if let Some(module_name) = extract_chap_module_name_from_path(&path) {
                                    chap19_wildcards.insert(module_name);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Find Chap18 wildcards that have Chap19 equivalents
    for node in root_final.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node.clone()) {
                if let Some(use_tree) = use_item.use_tree() {
                    // Check if this is a wildcard import
                    if use_tree.star_token().is_some() {
                        if let Some(path) = use_tree.path() {
                            let path_str = path.to_string();
                            // Check for Chap18 wildcard imports
                            if path_str.starts_with("apas_ai::Chap18::") {
                                if let Some(module_name) = extract_chap_module_name_from_path(&path) {
                                    // If Chap19 has this module, mark Chap18 import for removal
                                    if chap19_wildcards.contains(&module_name) {
                                        let use_text = use_item.to_string().trim().to_string();
                                        chap18_to_remove.push(use_text);
                                    }
                                }
                            }
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

fn extract_chap_module_name_from_path(path: &ast::Path) -> Option<String> {
    // Extract module name from path like "apas_ai::ChapXX::ModuleName::ModuleName"
    // Returns "ModuleName"
    
    // Collect all path segments
    let mut segments = Vec::new();
    let mut current_path = Some(path.clone());
    
    while let Some(p) = current_path {
        if let Some(segment) = p.segment() {
            segments.push(segment.to_string());
        }
        current_path = p.qualifier();
    }
    
    // Reverse to get segments in order: [apas_ai, ChapXX, ModuleName, ModuleName]
    segments.reverse();
    
    // Pattern: apas_ai :: ChapXX :: ModuleName :: ModuleName
    if segments.len() >= 4 && segments[0] == "apas_ai" && (segments[1].starts_with("Chap18") || segments[1].starts_with("Chap19")) {
        Some(segments[2].clone())
    } else {
        None
    }
}

fn extract_module_path_from_path(path: &ast::Path) -> Option<String> {
    // Extract module path from AST path, excluding the last segment
    // e.g., apas_ai::Chap37::Foo::Foo::Bar -> apas_ai::Chap37::Foo::Foo
    
    // Collect all path segments
    let mut segments = Vec::new();
    let mut current_path = Some(path.clone());
    
    while let Some(p) = current_path {
        if let Some(segment) = p.segment() {
            segments.push(segment.to_string());
        }
        current_path = p.qualifier();
    }
    
    // Reverse to get segments in order
    segments.reverse();
    
    // We want everything except the last segment
    if segments.len() > 1 {
        let module_parts = &segments[..segments.len() - 1];
        Some(module_parts.join("::"))
    } else {
        None
    }
}

fn main() {
    let args = match rusticate::StandardArgs::parse() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {e}");
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
                    
                    log!("{}:{}: fixed {} module(s)", display_path, 0, modules_fixed);
                }
            }
            Err(e) => {
                eprintln!("Error processing {}: {}", file_path.display(), e);
            }
        }
    }
    
    log!("");
    log!("{}", "-".repeat(80));
    log!("Files modified: {}", rusticate::format_number(total_files_modified));
    log!("Modules converted to wildcards: {}", rusticate::format_number(total_modules_fixed));
    log!("Completed in {}ms", start.elapsed().as_millis());
    
    if total_files_modified > 0 {
        std::process::exit(1);
    } else {
        std::process::exit(0);
    }
}

