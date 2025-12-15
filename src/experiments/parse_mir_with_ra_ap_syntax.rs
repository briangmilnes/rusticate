//! Experiment: Can ra_ap_syntax parse MIR text?
//!
//! MIR (Mid-level Intermediate Representation) is dumped by rustc as human-readable text.
//! This experiment tests whether ra_ap_syntax can parse any of it, since MIR has
//! Rust-like syntax in places (type annotations, function signatures, etc.)
//!
//! Run with: cargo run --bin parse_mir_experiment -- <mir_file>

use ra_ap_syntax::{SourceFile, Edition, SyntaxKind, SyntaxNode, ast::{self, AstNode, HasName}};
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <mir_file>", args[0]);
        eprintln!("Example: {} /path/to/foo.mir", args[0]);
        std::process::exit(1);
    }
    
    let mir_path = &args[1];
    let content = fs::read_to_string(mir_path)
        .expect("Failed to read MIR file");
    
    println!("=== Attempting to parse MIR file: {} ===", mir_path);
    println!("File size: {} bytes, {} lines\n", content.len(), content.lines().count());
    
    // Try parsing the whole file
    println!("--- Parsing entire file as Rust source ---");
    let parse = SourceFile::parse(&content, Edition::Edition2021);
    let syntax = parse.syntax_node();
    
    println!("Parse errors: {}", parse.errors().len());
    if !parse.errors().is_empty() {
        println!("First 5 errors:");
        for err in parse.errors().iter().take(5) {
            println!("  {:?}", err);
        }
    }
    
    // Count what we found
    let mut counts: std::collections::HashMap<SyntaxKind, usize> = std::collections::HashMap::new();
    for node in syntax.descendants() {
        *counts.entry(node.kind()).or_insert(0) += 1;
    }
    
    println!("\nSyntax node kinds found (top 20):");
    let mut count_vec: Vec<_> = counts.into_iter().collect();
    count_vec.sort_by(|a, b| b.1.cmp(&a.1));
    for (kind, count) in count_vec.iter().take(20) {
        println!("  {:?}: {}", kind, count);
    }
    
    // Try to find any recognizable Rust constructs
    println!("\n--- Looking for specific constructs ---");
    
    // Functions
    let fns: Vec<_> = syntax.descendants()
        .filter_map(ast::Fn::cast)
        .collect();
    println!("Functions found: {}", fns.len());
    for f in fns.iter().take(3) {
        if let Some(name) = f.name() {
            println!("  fn {}", name.text());
        }
    }
    
    // Structs
    let structs: Vec<_> = syntax.descendants()
        .filter_map(ast::Struct::cast)
        .collect();
    println!("Structs found: {}", structs.len());
    
    // Type aliases
    let type_aliases: Vec<_> = syntax.descendants()
        .filter_map(ast::TypeAlias::cast)
        .collect();
    println!("Type aliases found: {}", type_aliases.len());
    
    // Let statements
    let lets: Vec<_> = syntax.descendants()
        .filter_map(ast::LetStmt::cast)
        .collect();
    println!("Let statements found: {}", lets.len());
    for l in lets.iter().take(3) {
        println!("  {:?}", l.syntax().text().to_string().chars().take(60).collect::<String>());
    }
    
    // Path expressions (like std::result::Result)
    let paths: Vec<_> = syntax.descendants()
        .filter_map(ast::Path::cast)
        .collect();
    println!("Path expressions found: {}", paths.len());
    
    // Filter for stdlib paths
    let stdlib_paths: Vec<_> = paths.iter()
        .filter(|p| {
            let text = p.syntax().text().to_string();
            text.starts_with("std::") || text.starts_with("core::") || text.starts_with("alloc::")
        })
        .collect();
    println!("Stdlib paths found: {}", stdlib_paths.len());
    for p in stdlib_paths.iter().take(10) {
        println!("  {}", p.syntax().text());
    }
    
    // Now try parsing individual MIR lines that look like Rust
    println!("\n--- Parsing individual MIR lines ---");
    
    let mut parsed_lines = 0;
    let mut successful_lines = 0;
    
    for line in content.lines().take(100) {
        let trimmed = line.trim();
        
        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        
        // Try lines that look like let statements
        if trimmed.starts_with("let ") {
            parsed_lines += 1;
            let parse = SourceFile::parse(&format!("fn _() {{ {} }}", trimmed), Edition::Edition2021);
            if parse.errors().is_empty() {
                successful_lines += 1;
            }
        }
        
        // Try lines that look like function signatures  
        if trimmed.starts_with("fn ") {
            parsed_lines += 1;
            let parse = SourceFile::parse(&format!("{} {{}}", trimmed.trim_end_matches(" {")), Edition::Edition2021);
            if parse.errors().is_empty() {
                successful_lines += 1;
            }
        }
    }
    
    println!("Tried to parse {} MIR lines individually", parsed_lines);
    println!("Successfully parsed: {} ({:.1}%)", 
             successful_lines, 
             if parsed_lines > 0 { 100.0 * successful_lines as f64 / parsed_lines as f64 } else { 0.0 });
    
    println!("\n=== Conclusion ===");
    if stdlib_paths.len() > 0 {
        println!("ra_ap_syntax CAN extract some stdlib paths from MIR!");
        println!("This could be useful for type-directed extraction.");
    } else {
        println!("ra_ap_syntax cannot meaningfully parse MIR text.");
        println!("A custom MIR parser would be needed.");
    }
}

