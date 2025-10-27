// Copyright (C) Brian G. Milnes 2025

//! Review: All benchmark modules compile successfully
//! 
//! Replaces: scripts/rust/benches/review_bench_modules.py
//! Rule: All benchmark files should be discoverable and compile
//! Git commit: 584a672b6a34782766863c5f76a461d3297a741a
//! Binary: rusticate-review-bench-modules

use anyhow::Result;
use std::process::Command;
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
            .open("analyses/review_bench_modules.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn main() -> Result<()> {
    let start = Instant::now();
    
    // Run cargo bench to check if benchmarks compile
    let output = Command::new("cargo")
        .args(&["bench", "--benches", "--no-run", "--quiet"])
        .output()?;
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    if !output.status.success() {
        log!("❌ Benchmark compilation check failed:");
        
        // Show error lines
        for line in stderr.lines() {
            let line_lower = line.to_lowercase();
            if line_lower.contains("error") || line_lower.contains("warning") {
                log!("   {}", line);
            }
        }
        
        let elapsed = start.elapsed().as_millis();
        log!("Completed in {}ms", elapsed);
        
        std::process::exit(1);
    }
    
    log!("✓ All benchmark modules compile successfully");
    
    let elapsed = start.elapsed().as_millis();
    log!("Completed in {}ms", elapsed);
    
    Ok(())
}

