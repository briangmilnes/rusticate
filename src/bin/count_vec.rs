// Copyright (C) Brian G. Milnes 2025

//! Count 'Vec' type usages in Rust code
//! 
//! Replaces: scripts/analyze/count_vec.sh
//! Uses AST parsing to find PATH_TYPE nodes with 'Vec' identifier
//! Binary: rusticate-count-vec

use anyhow::Result;
use rusticate::{StandardArgs, parse_source, find_nodes};
use rusticate::count_helper::count_helper;
use rusticate::tool_runner::tool_runner;
use ra_ap_syntax::{SyntaxKind, ast::{self, AstNode}};
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
            .open("analyses/count_vec.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn count_vec_in_file(file_path: &Path) -> Result<usize> {
    let content = fs::read_to_string(file_path)?;
    let source_file = parse_source(&content)?;
    let root = source_file.syntax();
    
    // Find all PATH_TYPE nodes
    let path_types = find_nodes(root, SyntaxKind::PATH_TYPE);
    
    // Count only those that reference Vec using AST
    let vec_count = path_types.iter()
        .filter(|node| {
            if let Some(path_type) = ast::PathType::cast((*node).clone()) {
                if let Some(path) = path_type.path() {
                    // Check if any segment in the path has "Vec" as its name_ref
                    return path.segments().any(|seg| {
                        seg.name_ref()
                            .is_some_and(|name_ref| name_ref.text() == "Vec")
                    });
                }
            }
            false
        })
        .count();
    
    Ok(vec_count)
}

fn main() -> Result<()> {
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let paths = args.get_search_dirs();
    
    tool_runner::run_simple("count-vec", base_dir.clone(), || {
        count_helper::run_count(&paths, &base_dir, count_vec_in_file, "Vec usages")
    })
}

