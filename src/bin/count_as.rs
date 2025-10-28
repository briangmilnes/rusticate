// Copyright (C) Brian G. Milnes 2025

//! Count UFCS (Universal Function Call Syntax) usage in Rust code
//! 
//! Counts occurrences of `<Type as Trait>::` syntax patterns
//! Uses AST parsing to find AS_KW tokens in path contexts
//! Binary: rusticate-count-as

use anyhow::Result;
use rusticate::{StandardArgs, parse_source};
use rusticate::count_helper::count_helper;
use rusticate::tool_runner::tool_runner;
use ra_ap_syntax::{SyntaxKind, ast::AstNode};
use std::fs;
use std::path::Path;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/count_as.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

fn count_as_in_file(file_path: &Path) -> Result<usize> {
    let content = fs::read_to_string(file_path)?;
    let source_file = parse_source(&content)?;
    let root = source_file.syntax();
    
    let mut count = 0;
    
    // Find all AS_KW tokens that are NOT part of import renames
    // This detects UFCS patterns like <Type as Trait>::method
    // but excludes import aliases like `use foo as bar`
    for token in root.descendants_with_tokens() {
        if token.kind() == SyntaxKind::AS_KW {
            // Check if this AS_KW is part of a RENAME node (import alias)
            let is_import_alias = token.parent().is_some_and(|parent| {
                parent.ancestors().any(|ancestor| ancestor.kind() == SyntaxKind::RENAME)
            });
            
            if !is_import_alias {
                count += 1;
            }
        }
    }
    
    Ok(count)
}

fn main() -> Result<()> {
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let paths = args.get_search_dirs();
    
    tool_runner::run_simple("count-as", base_dir.clone(), || {
        count_helper::run_count(&paths, &base_dir, count_as_in_file, "'as' expressions")
    })
}

