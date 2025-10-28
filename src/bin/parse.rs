// Copyright (C) Brian G. Milnes 2025

//! Parse and display the AST of a Rust file

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::time::Instant;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/parse.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
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
    log!("Entering directory '{}'", parent_dir.display());
    log!("");
    
    log!("Parsing file: {:?}", args.path);
    log!("Format: {}", args.format);
    
    rusticate::parse(&args.path)?;
    
    log!("");
    log!("Completed in {}ms", start.elapsed().as_millis());
    
    Ok(())
}

