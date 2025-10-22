use ra_ap_syntax::{ast::{self, AstNode}, Edition, SyntaxKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::fs;

fn process_file(file_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(file_path)?;
    let parse = ra_ap_syntax::SourceFile::parse(&content, Edition::Edition2021);
    let root = parse.syntax_node();
    
    // Find all use statements
    let mut module_imports: HashMap<String, Vec<String>> = HashMap::new();
    let mut has_wildcard: HashSet<String> = HashSet::new();
    
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            if let Some(use_item) = ast::Use::cast(node) {
                if let Some(use_tree) = use_item.use_tree() {
                    let use_text = use_item.to_string();
                    
                    // Check if this is a grouped import (use foo::{a, b})
                    if use_tree.use_tree_list().is_some() {
                        continue;
                    }
                    
                    if let Some(path) = use_tree.path() {
                        let path_str = path.to_string();
                        
                        // Check if this is a wildcard import
                        if use_tree.star_token().is_some() {
                            // Wildcard import - store the module path
                            has_wildcard.insert(path_str);
                        } else if path_str.contains("Chap") {
                            // Type-specific import like: apas_ai::Chap37::Foo::Foo::Bar
                            // Extract module path (everything before the last segment)
                            
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
                            
                            // Pattern: apas_ai::ChapXX::ModuleName::ModuleName::TypeName
                            // We want: apas_ai::ChapXX::ModuleName::ModuleName
                            if segments.len() >= 4 {
                                // Check if we have the doubled module name pattern
                                let mut module_segments = segments.clone();
                                if segments.len() >= 5 && segments[2] == segments[3] {
                                    // Pattern with doubled module name - remove the last (TypeName)
                                    module_segments = segments[..segments.len()-1].to_vec();
                                }
                                
                                if module_segments.len() >= 3 {
                                    let module = module_segments.join("::");
                                    module_imports.entry(module.clone()).or_insert_with(Vec::new).push(use_text.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Find modules that need wildcard imports
    let mut imports_to_add = Vec::new();
    for (module, _uses) in &module_imports {
        if !has_wildcard.contains(module) {
            imports_to_add.push(format!("use {}::*;", module));
        }
    }
    
    if imports_to_add.is_empty() {
        return Ok(false);
    }
    
    // Find insertion point (after the last MODULE-LEVEL use statement)
    let mut last_use_end = None;
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            // Check if this use is at module level (parent is SOURCE_FILE or MODULE)
            if let Some(parent) = node.parent() {
                if parent.kind() == SyntaxKind::SOURCE_FILE || 
                   (parent.kind() == SyntaxKind::ITEM_LIST && 
                    parent.parent().map_or(false, |gp| gp.kind() == SyntaxKind::MODULE)) {
                    let range = node.text_range();
                    last_use_end = Some(range.end().into());
                }
            }
        }
    }
    
    if let Some(insert_pos) = last_use_end {
        // Find the newline after the last use statement
        let insert_pos = content[insert_pos..]
            .find('\n')
            .map(|offset| insert_pos + offset + 1)
            .unwrap_or(insert_pos);
        
        let mut new_content = String::new();
        new_content.push_str(&content[..insert_pos]);
        for import in &imports_to_add {
            new_content.push_str(import);
            new_content.push('\n');
        }
        new_content.push_str(&content[insert_pos..]);
        
        fs::write(file_path, new_content)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args[1] != "-d" {
        eprintln!("Usage: {} -d <directory>", args[0]);
        std::process::exit(1);
    }
    
    let target_dir = &args[2];
    let start = std::time::Instant::now();
    
    println!("Adding trait imports to test/bench files in: {}", target_dir);
    println!();
    
    let files = rusticate::find_rust_files(&[PathBuf::from(target_dir)]);
    
    let mut total_files = 0;
    let mut modified_files = 0;
    
    for path in &files {
        total_files += 1;
        
        match process_file(path) {
            Ok(true) => {
                modified_files += 1;
                println!("{}: added trait imports", path.display());
            }
            Ok(false) => {
                // No changes needed
            }
            Err(e) => {
                eprintln!("Error processing {}: {}", path.display(), e);
            }
        }
    }
    
    println!();
    println!("Summary:");
    println!("  Files processed: {}", total_files);
    println!("  Files modified: {}", modified_files);
    println!("Completed in {}ms", start.elapsed().as_millis());
}

