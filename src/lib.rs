// Copyright (C) Brian G. Milnes 2025

//! Rusticate - AST-based Rust code review and fix tool
//! 
//! This library provides functionality to parse, analyze, and fix Rust code
//! using abstract syntax trees (AST) instead of regex-based approaches.

pub mod parser;
pub mod analyzer;
pub mod fixer;
pub mod visitor;
pub mod args;
pub mod ast_utils;
pub mod logging;
pub mod tool_runner;
pub mod count_helper;
pub mod duplicate_methods;

use anyhow::Result;
use std::path::Path;

// Re-export commonly used items
pub use parser::parser::parse_file;
pub use analyzer::analyzer::{analyze, Issue, IssueKind, Severity};
pub use fixer::fixer::fix;
pub use visitor::visitor::IssueVisitor;
pub use args::args::{StandardArgs, format_number, find_rust_files, get_search_dirs};
pub use ast_utils::ast_utils::*;

/// Review a Rust file and provide feedback
pub fn review(file: &Path, format: &str) -> Result<()> {
    let source = std::fs::read_to_string(file)?;
    let syntax = parse_file(&source)?;
    
    let issues = analyze(&syntax)?;
    
    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&issues)?;
            println!("{json}");
        }
        _ => {
            if issues.is_empty() {
                println!("✓ No issues found!");
            } else {
                println!("Found {} issue(s):", issues.len());
                for issue in issues {
                    println!("  - {issue}");
                }
            }
        }
    }
    
    Ok(())
}

/// Fix common issues in a Rust file
pub fn fix_file(file: &Path, in_place: bool) -> Result<()> {
    let source = std::fs::read_to_string(file)?;
    let syntax = parse_file(&source)?;
    
    let fixed_code = fix(&syntax)?;
    
    if in_place {
        std::fs::write(file, fixed_code)?;
        println!("Fixed and saved to {file:?}");
    } else {
        println!("{fixed_code}");
    }
    
    Ok(())
}

/// Parse a Rust file and display its AST
pub fn parse(file: &Path) -> Result<()> {
    let source = std::fs::read_to_string(file)?;
    let syntax = parse_file(&source)?;
    
    println!("{syntax:#?}");
    
    Ok(())
}

