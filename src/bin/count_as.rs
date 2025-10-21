// Copyright (C) Brian G. Milnes 2025

//! Count 'as' type cast expressions in Rust code
//! 
//! Replaces: scripts/analyze/count_as.sh
//! Uses AST parsing to find AS_EXPR nodes (type casts)
//! Binary: rusticate-count-as

use anyhow::Result;
use rusticate::{StandardArgs, parse_source, find_nodes};
use rusticate::count_helper::count_helper;
use rusticate::tool_runner::tool_runner;
use ra_ap_syntax::{SyntaxKind, ast::AstNode};
use std::fs;
use std::path::Path;

fn count_as_in_file(file_path: &Path) -> Result<usize> {
    let content = fs::read_to_string(file_path)?;
    let source_file = parse_source(&content)?;
    let root = source_file.syntax();
    
    // Find all AS_EXPR (cast expressions) nodes
    let as_exprs = find_nodes(root, SyntaxKind::CAST_EXPR);
    
    Ok(as_exprs.len())
}

fn main() -> Result<()> {
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    let paths = args.paths;
    
    tool_runner::run_simple("count-as", base_dir.clone(), || {
        count_helper::run_count(&paths, &base_dir, count_as_in_file, "'as' expressions")
    })
}

