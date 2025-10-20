// Copyright (C) Brian G. Milnes 2025

//! Parse and display the AST of a Rust file

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

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
    let args = Args::parse();
    
    println!("Parsing file: {:?}", args.path);
    println!("Format: {}", args.format);
    
    rusticate::parse(&args.path)?;
    
    Ok(())
}

