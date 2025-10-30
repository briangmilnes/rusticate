// Copyright (C) Brian G. Milnes 2025

//! Review dispatcher - run review tools by name or all at once
//!
//! Usage:
//!   rusticate-review all -c               # Run all review tools
//!   rusticate-review string-hacking -c    # Run specific review tool
//!   rusticate-review logging -d src/bin   # Run with specific args
//!
//! Binary: rusticate-review

use anyhow::{Context, Result};
use std::process::{Command, Stdio};
use std::time::Instant;
use std::env;
use std::fs;
use std::io::Write;

macro_rules! log {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/rusticate-review.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

fn log_full(msg: &str) {
    // Log only to comprehensive log file (stdout is handled separately)
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("analyses/rusticate-review-full.log")
    {
        let _ = write!(file, "{}", msg);
    }
}

fn get_available_review_tools() -> Vec<&'static str> {
    // Only tools that are actually built (in Cargo.toml)
    vec![
        "bench-modules",
        "comment-placement",
        "doctests",
        "duplicate-bench-names",
        "duplicate-methods",
        "impl-order",
        "impl-trait-bounds",
        "import-order",
        "inherent-and-trait-impl",
        "inherent-plus-trait-impl",
        "integration-test-structure",
        "internal-method-impls",
        "logging",
        "minimize-ufcs-call-sites",
        "module-encapsulation",
        "no-extern-crate",
        "non-wildcard-uses",
        "no-trait-method-duplication",
        "pascal-case-filenames",
        "public-only-inherent-impls",
        "qualified-paths",
        "redundant-inherent-impls",
        "single-trait-impl",
        "snake-case-filenames",
        "st-mt-consistency",
        "string-hacking",
        "struct-file-naming",
        "stt-compliance",
        "stub-delegation",
        "test-modules",
        "trait-bound-mismatches",
        "trait-definition-order",
        "trait-method-conflicts",
        "trait-self-usage",
        "typeclasses",
        "variable-naming",
        "where-clause-simplification",
    ]
}

fn run_review_tool(tool_name: &str, args: &[String]) -> Result<()> {
    let binary_name = format!("rusticate-review-{tool_name}");
    let exe_path = env::current_exe()
        .context("Failed to get current executable path")?
        .parent()
        .context("Failed to get parent directory")?
        .join(&binary_name);
    
    log!("\n=== Running {tool_name} ===");
    log_full(&format!("\n=== Running {tool_name} ===\n"));
    
    // Capture stdout and stderr
    let output = Command::new(&exe_path)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("Failed to run {binary_name}"))?;
    
    // Write captured output to terminal and comprehensive log
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    
    print!("{stdout_str}");
    log_full(&stdout_str);
    
    if !stderr_str.is_empty() {
        eprint!("{stderr_str}");
        log_full(&stderr_str);
    }
    
    if !output.status.success() {
        let warning = format!("Warning: {tool_name} exited with status {}\n", output.status);
        log!("{}", warning.trim());
        log_full(&warning);
    }
    
    Ok(())
}

fn print_usage() {
    eprintln!("rusticate-review: Run review tools by name or all at once");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  rusticate-review <tool-name> [OPTIONS]");
    eprintln!("  rusticate-review all [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -c, --codebase             Analyze src/, tests/, benches/");
    eprintln!("  -d, --dir DIR [DIR...]     Analyze specific directories");
    eprintln!("  -f, --file FILE            Analyze a single file");
    eprintln!("  -m, --module NAME          Find module and analyze");
    eprintln!("  -h, --help                 Show this help");
    eprintln!();
    eprintln!("Available tools:");
    for tool in get_available_review_tools() {
        eprintln!("  {tool}");
    }
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  rusticate-review all -c                    # Run all review tools");
    eprintln!("  rusticate-review string-hacking -c         # Check for string hacking");
    eprintln!("  rusticate-review logging -d src/bin        # Check logging in binaries");
    eprintln!("  rusticate-review test-functions -m ArraySeq # Check test coverage for module");
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }
    
    let tool_or_command = &args[1];
    
    // Check for help
    if tool_or_command == "--help" || tool_or_command == "-h" {
        print_usage();
        return Ok(());
    }
    
    // Get remaining args to pass through
    let passthrough_args: Vec<String> = args.iter().skip(2).cloned().collect();
    
    if tool_or_command == "all" {
        log!("Running all review tools...");
        log!("");
        log_full("RUSTICATE COMPREHENSIVE REVIEW - ALL TOOLS\n");
        log_full("===========================================\n\n");
        
        let tools = get_available_review_tools();
        let mut failed_tools = Vec::new();
        
        for tool in &tools {
            if let Err(e) = run_review_tool(tool, &passthrough_args) {
                let err_msg = format!("Error running {tool}: {e}");
                log!("{err_msg}");
                log_full(&format!("\nERROR: {err_msg}\n"));
                failed_tools.push(*tool);
            }
        }
        
        log!("");
        log!("=== Summary ===");
        log!("Ran {} review tools", tools.len());
        log_full(&format!("\n===========================================\n"));
        log_full(&format!("SUMMARY: Ran {} review tools\n", tools.len()));
        
        if !failed_tools.is_empty() {
            log!("Failed tools ({}):", failed_tools.len());
            log_full(&format!("Failed tools ({}):\n", failed_tools.len()));
            for tool in failed_tools {
                log!("  - {tool}");
                log_full(&format!("  - {tool}\n"));
            }
            log_full("===========================================\n");
            std::process::exit(1);
        } else {
            log!("All tools completed successfully");
            log_full("All tools completed successfully\n");
            log_full("===========================================\n");
        }
    } else {
        // Run specific tool
        let available_tools = get_available_review_tools();
        if !available_tools.contains(&tool_or_command.as_str()) {
            eprintln!("Error: Unknown review tool '{tool_or_command}'");
            eprintln!();
            eprintln!("Available tools:");
            for tool in available_tools {
                eprintln!("  {tool}");
            }
            eprintln!();
            eprintln!("Or use 'all' to run all review tools");
            std::process::exit(1);
        }
        
        run_review_tool(tool_or_command, &passthrough_args)?;
    }
    
    let completion = format!("Completed in {}ms\n", start.elapsed().as_millis());
    log!("");
    log!("Completed in {}ms", start.elapsed().as_millis());
    log_full(&format!("\n{completion}"));
    
    Ok(())
}
