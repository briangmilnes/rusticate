// Copyright (C) Brian G. Milnes 2025

//! Review Rust code and provide feedback
//! 
//! Replaces: scripts/review.py

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "review")]
#[command(about = "Review Rust code for APAS conventions", long_about = None)]
struct Args {
    /// Path to the Rust file or directory to review
    #[arg(short, long)]
    path: PathBuf,
    
    /// Output format (text, json)
    #[arg(short, long, default_value = "text")]
    format: String,
    
    /// Run specific review check (if not specified, runs all)
    #[arg(short, long)]
    check: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    println!("Reviewing: {:?}", args.path);
    println!("Format: {}", args.format);
    if let Some(check) = args.check {
        println!("Running check: {}", check);
    }
    
    rusticate::review(&args.path, &args.format)?;
    
    Ok(())
}

