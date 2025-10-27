// Copyright (C) Brian G. Milnes 2025

//! Review Rust code and provide feedback
//! 
//! Replaces: scripts/review.py

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::time::Instant;
use std::fs;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
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
    let start = Instant::now();
    let args = Args::parse();
    
    // Print directory context
    let dir = if args.path.is_dir() {
        &args.path
    } else {
        args.path.parent().unwrap_or_else(|| std::path::Path::new("."))
    };
    log!("Entering directory '{}'", dir.display());
    log!("");
    
    log!("Reviewing: {:?}", args.path);
    log!("Format: {}", args.format);
    if let Some(check) = args.check {
        log!("Running check: {}", check);
    }
    
    rusticate::review(&args.path, &args.format)?;
    
    log!("");
    log!("Completed in {}ms", start.elapsed().as_millis());
    
    Ok(())
}

