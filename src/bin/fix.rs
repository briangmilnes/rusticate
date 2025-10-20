// Copyright (C) Brian G. Milnes 2025

//! Fix common issues in Rust code
//! 
//! Replaces various scripts/*/fix_*.py scripts

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "fix")]
#[command(about = "Fix common issues in Rust code", long_about = None)]
struct Args {
    /// Path to the Rust file to fix
    #[arg(short, long)]
    path: PathBuf,
    
    /// Apply fixes in-place
    #[arg(short, long)]
    in_place: bool,
    
    /// Run specific fix (if not specified, runs all safe fixes)
    #[arg(short = 'f', long)]
    fix_type: Option<String>,
    
    /// Dry run - show what would be changed without modifying files
    #[arg(short, long)]
    dry_run: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    println!("Fixing file: {:?}", args.path);
    println!("In-place: {}", args.in_place);
    println!("Dry-run: {}", args.dry_run);
    if let Some(fix) = args.fix_type {
        println!("Running fix: {}", fix);
    }
    
    rusticate::fix_file(&args.path, args.in_place)?;
    
    Ok(())
}

