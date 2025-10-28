// Copyright (C) Brian G. Milnes 2025

//! Review: Duplicate Methods
//! 
//! Finds duplicate method/function names within a module.
//! Shows each duplicate pair with their line numbers.
//! 
//! Binary: rusticate-review-duplicate-methods

use std::time::Instant;
use rusticate::StandardArgs;
use rusticate::args::args::find_rust_files;
use rusticate::duplicate_methods::find_duplicate_methods;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_duplicate_methods.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn main() -> anyhow::Result<()> {
    let args = StandardArgs::parse()?;
    let start = Instant::now();

    let search_dirs = args.get_search_dirs();
    let files = find_rust_files(&search_dirs);

    let mut all_issues = Vec::new();
    
    for file_path in &files {
        if let Ok(issues) = find_duplicate_methods(file_path) {
            if !issues.is_empty() {
                all_issues.push((file_path.clone(), issues));
            }
        }
    }

    log!("{}", "=".repeat(80));
    log!("DUPLICATE METHODS REVIEW");
    log!("{}", "=".repeat(80));

    if all_issues.is_empty() {
        log!("\n✓ No duplicate methods found!");
    } else {
        log!("");
        for (file_path, issues) in &all_issues {
            for issue in issues {
                log!("Duplicate method '{}' in {}:", issue.name, file_path.display());
                for loc in &issue.locations {
                    log!("{}:{}: {}", file_path.display(), loc.line, loc.first_line);
                }
                log!("");
            }
        }
    }

    let total_files = all_issues.len();
    let total_issues: usize = all_issues.iter().map(|(_, issues)| issues.len()).sum();

    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  Total files with duplicates: {}", total_files);
    log!("  Total duplicate names: {}", total_issues);
    log!("");
    log!("Completed in {}ms", start.elapsed().as_millis());

    Ok(())
}
