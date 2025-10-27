// Copyright (C) Brian G. Milnes 2025

//! Fix common issues in Rust code
//! 
//! Replaces various scripts/*/fix_*.py scripts

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
            .open("analyses/fix.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
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
    let start = Instant::now();
    let args = Args::parse();
    
    // Print directory context
    let parent_dir = args.path.parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    log!("Entering directory '{}'", parent_dir.display());
    log!("");
    
    log!("Fixing file: {:?}", args.path);
    log!("In-place: {}", args.in_place);
    log!("Dry-run: {}", args.dry_run);
    if let Some(fix) = args.fix_type {
        log!("Running fix: {}", fix);
    }
    
    rusticate::fix_file(&args.path, args.in_place)?;
    
    log!("");
    log!("Completed in {}ms", start.elapsed().as_millis());
    
    Ok(())
}

