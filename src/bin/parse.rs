// Copyright (C) Brian G. Milnes 2025

//! Parse and display the AST of a Rust file

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "parse")]
#[command(about = "Parse and display the AST of a Rust file", long_about = None)]
struct Args {
    /// Path to the Rust file to parse
    #[arg(short, long)]
    path: PathBuf,
    
    /// Output format (tree, json, debug)
    #[arg(short, long, default_value = "tree")]
    format: String,
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = Args::parse();
    
    // Print directory context
    let parent_dir = args.path.parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    println!("Entering directory '{}'", parent_dir.display());
    println!();
    
    println!("Parsing file: {:?}", args.path);
    println!("Format: {}", args.format);
    
    rusticate::parse(&args.path)?;
    
    println!();
    println!("Completed in {}ms", start.elapsed().as_millis());
    
    Ok(())
}

