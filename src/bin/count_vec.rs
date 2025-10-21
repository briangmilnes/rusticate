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
                    // Check if any segment in the path is "Vec"
                    return path.segments().any(|seg| seg.to_string() == "Vec");
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
    let paths = args.paths;
    
    tool_runner::run_simple("count-vec", base_dir.clone(), || {
        count_helper::run_count(&paths, &base_dir, count_vec_in_file, "Vec usages")
    })
}

