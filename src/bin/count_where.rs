// Copyright (C) Brian G. Milnes 2025

//! Count 'where' clauses in Rust code
//! 
//! Replaces: scripts/analyze/count_where.sh
//! Uses AST parsing to find WHERE_CLAUSE nodes
//! Binary: rusticate-count-where

use anyhow::Result;
use rusticate::{StandardArgs, parse_source, find_nodes};
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
            .open("analyses/count_where.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn count_where_in_file(file_path: &Path) -> Result<usize> {
    let content = fs::read_to_string(file_path)?;
    let source_file = parse_source(&content)?;
    let root = source_file.syntax();
    
    // Find all WHERE_CLAUSE nodes
    let where_clauses = find_nodes(root, SyntaxKind::WHERE_CLAUSE);
    
    Ok(where_clauses.len())
}

fn main() -> Result<()> {
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let paths = args.paths;
    
    tool_runner::run_simple("count-where", base_dir.clone(), || {
        count_helper::run_count(&paths, &base_dir, count_where_in_file, "where clauses")
    })
}

