// Copyright (C) Brian G. Milnes 2025

use anyhow::Result;
use std::collections::HashMap;
use std::process::Command;
use std::time::Instant;
use rusticate::format_number;
use std::fs;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/analyze_review_typeclasses.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn main() -> Result<()> {
    let start = Instant::now();
    
    // Build the review-typeclasses command
    // Find the binary in the same directory as this binary
    let current_exe = std::env::current_exe()?;
    let bin_dir = current_exe.parent().ok_or_else(|| anyhow::anyhow!("Cannot determine binary directory"))?;
    let review_tool = bin_dir.join("rusticate-review-typeclasses");
    
    let mut cmd = Command::new(review_tool);
    
    // Forward the exact same arguments that were passed to us
    // Skip the first argument (program name)
    for arg in std::env::args().skip(1) {
        cmd.arg(arg);
    }
    
    // Run the command and capture output
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Parse output and count issues
    let mut bug_counts: HashMap<String, usize> = HashMap::new();
    let mut warn_counts: HashMap<String, usize> = HashMap::new();
    
    for line in stdout.lines() {
        if line.contains(" - BUG") {
            // Extract the issue type from the line
            let issue_type = if line.contains("missing module") {
                "missing module"
            } else if line.contains("no pub data type") {
                "no pub data type (struct, enum, or type alias)"
            } else if line.contains("no external trait") {
                "no external trait"
            } else if line.contains("no Trait impl") {
                "no Trait impl"
            } else if line.contains("impl<") && line.contains("{ (for external type) - BUG") {
                "inherent impl with pub methods or only internal"
            } else if line.contains("duplicate method:") {
                "duplicate method"
            } else if line.contains("unused self parameter") {
                "method with unused self parameter"
            } else {
                "other BUG"
            };
            
            *bug_counts.entry(issue_type.to_string()).or_insert(0) += 1;
        } else if line.contains(" - WARNING") {
            let issue_type = if line.contains("trait") {
                "internal trait"
            } else {
                "other WARNING"
            };
            
            *warn_counts.entry(issue_type.to_string()).or_insert(0) += 1;
        }
    }
    
    // Sort by frequency (Pareto principle)
    let mut bug_vec: Vec<_> = bug_counts.iter().collect();
    bug_vec.sort_by(|a, b| b.1.cmp(a.1));
    
    let mut warn_vec: Vec<_> = warn_counts.iter().collect();
    warn_vec.sort_by(|a, b| b.1.cmp(a.1));
    
    // Calculate totals
    let total_bugs: usize = bug_counts.values().sum();
    let total_warnings: usize = warn_counts.values().sum();
    
    // Print results
    log!("Entering directory '{}'", std::env::current_dir()?.display());
    log!("");
    log!("{}", "=".repeat(80));
    log!("PARETO ANALYSIS: BUGS");
    log!("{}", "=".repeat(80));
    
    if !bug_vec.is_empty() {
        let mut cumulative = 0;
        for (issue_type, count) in &bug_vec {
            cumulative += **count;
            let percentage = (**count as f64 / total_bugs as f64) * 100.0;
            let cumulative_pct = (cumulative as f64 / total_bugs as f64) * 100.0;
            log!("{:6} ({:5.1}%, cumulative {:5.1}%): {}", 
                format_number(**count), percentage, cumulative_pct, issue_type);
        }
        log!("{}", "-".repeat(80));
        log!("TOTAL BUGS: {}", format_number(total_bugs));
    } else {
        log!("No bugs found!");
    }
    
    log!("");
    log!("{}", "=".repeat(80));
    log!("PARETO ANALYSIS: WARNINGS");
    log!("{}", "=".repeat(80));
    
    if !warn_vec.is_empty() {
        let mut cumulative = 0;
        for (issue_type, count) in &warn_vec {
            cumulative += **count;
            let percentage = (**count as f64 / total_warnings as f64) * 100.0;
            let cumulative_pct = (cumulative as f64 / total_warnings as f64) * 100.0;
            log!("{:6} ({:5.1}%, cumulative {:5.1}%): {}", 
                format_number(**count), percentage, cumulative_pct, issue_type);
        }
        log!("{}", "-".repeat(80));
        log!("TOTAL WARNINGS: {}", format_number(total_warnings));
    } else {
        log!("No warnings found!");
    }
    
    log!("");
    log!("Completed in {}ms", start.elapsed().as_millis());
    
    Ok(())
}

