// Copyright (C) Brian G. Milnes 2025

//! Review: Summary Accuracy
//!
//! Verifies that summary sections in review tool output match the actual counts
//! of items listed above them.
//!
//! Binary: review-summary-accuracy

use anyhow::Result;
use rusticate::StandardArgs;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_summary_accuracy.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

#[derive(Debug)]
struct SummaryCheck {
    file: PathBuf,
    summary_type: String,
    claimed_count: usize,
    actual_count: usize,
    matches: bool,
}

/// Detect summary patterns and extract counts
fn extract_summary_counts(content: &str) -> Vec<(String, usize)> {
    let mut counts = Vec::new();
    
    for line in content.lines() {
        // Pattern: "Total public functions: 332"
        if let Some(start) = line.find("Total public functions:") {
            if let Some(num_str) = line[start..].split(':').nth(1) {
                if let Ok(num) = num_str.split_whitespace().next().unwrap_or("0").replace(",", "").parse::<usize>() {
                    counts.push(("Total public functions".to_string(), num));
                }
            }
        }
        
        // Pattern: "Functions with test coverage: 329 (99.1%)"
        if let Some(start) = line.find("Functions with test coverage:") {
            if let Some(num_str) = line[start..].split(':').nth(1) {
                let num_part = num_str.split_whitespace().next().unwrap_or("0");
                if let Ok(num) = num_part.replace(",", "").parse::<usize>() {
                    counts.push(("Functions with test coverage".to_string(), num));
                }
            }
        }
        
        // Pattern: "Functions without test coverage: 3 (0.9%)"
        if let Some(start) = line.find("Functions without test coverage:") {
            if let Some(num_str) = line[start..].split(':').nth(1) {
                let num_part = num_str.split_whitespace().next().unwrap_or("0");
                if let Ok(num) = num_part.replace(",", "").parse::<usize>() {
                    counts.push(("Functions without test coverage".to_string(), num));
                }
            }
        }
        
        // Pattern: "Total violations: 75 files checked, 4 violations found"
        if let Some(start) = line.find("Total violations:") {
            let rest = &line[start + "Total violations:".len()..];
            // Extract "4 violations found"
            if let Some(violations_pos) = rest.find("violations found") {
                let before_violations = &rest[..violations_pos].trim();
                if let Some(num_str) = before_violations.split_whitespace().last() {
                    if let Ok(num) = num_str.replace(",", "").parse::<usize>() {
                        counts.push(("Total violations".to_string(), num));
                    }
                }
            }
        }
        
        // Pattern: "Summary: 86 files checked, 1 files with violations, 6 total violations"
        if let Some(start) = line.find("Summary:") {
            let rest = &line[start + "Summary:".len()..];
            
            // Extract "86 files checked"
            if let Some(files_checked_pos) = rest.find("files checked") {
                let before = &rest[..files_checked_pos].trim();
                if let Some(num_str) = before.split_whitespace().last() {
                    if let Ok(num) = num_str.replace(",", "").parse::<usize>() {
                        counts.push(("Files checked".to_string(), num));
                    }
                }
            }
            
            // Extract "6 total violations"
            if let Some(violations_pos) = rest.find("total violations") {
                let before = &rest[..violations_pos].trim();
                if let Some(num_str) = before.split_whitespace().last() {
                    if let Ok(num) = num_str.replace(",", "").parse::<usize>() {
                        counts.push(("Total violations (summary)".to_string(), num));
                    }
                }
            }
        }
    }
    
    counts
}

/// Count actual items in output
fn count_actual_items(content: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    
    let mut in_summary = false;
    let mut no_test_coverage_count = 0;
    let mut with_test_coverage_count = 0;
    let mut violation_lines = 0;
    
    for line in content.lines() {
        // Stop counting when we hit summary
        if line.contains("SUMMARY:") || line.contains("Summary:") || line.contains("================") {
            if line.contains("SUMMARY") || line.contains("Summary") {
                in_summary = true;
            }
            continue;
        }
        
        if in_summary {
            continue;
        }
        
        // Count "NO TEST COVERAGE" lines
        if line.contains("NO TEST COVERAGE") {
            no_test_coverage_count += 1;
        }
        
        // Count tested functions (lines with "call(s) in")
        if line.contains("call(s) in") && !line.contains("NO TEST COVERAGE") {
            with_test_coverage_count += 1;
        }
        
        // Count violation lines (lines starting with file path and containing line number)
        // Pattern: "src/path/file.rs:123: message"
        if (line.starts_with("src/") || line.starts_with("tests/") || line.starts_with("benches/"))
            && (line.contains(":.") || line.contains(": ")) {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() >= 2 && parts[1].chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    violation_lines += 1;
                }
            }
    }
    
    counts.insert("Functions without test coverage (actual)".to_string(), no_test_coverage_count);
    counts.insert("Functions with test coverage (actual)".to_string(), with_test_coverage_count);
    counts.insert("Violation lines (actual)".to_string(), violation_lines);
    
    // Total public functions = tested + untested
    let total_functions = no_test_coverage_count + with_test_coverage_count;
    counts.insert("Total public functions (actual)".to_string(), total_functions);
    
    counts
}

fn check_file(file_path: &Path) -> Result<Vec<SummaryCheck>> {
    let content = fs::read_to_string(file_path)?;
    let mut checks = Vec::new();
    
    // Extract claimed counts from summaries
    let summary_counts = extract_summary_counts(&content);
    
    // Count actual items
    let actual_counts = count_actual_items(&content);
    
    // Match claimed vs actual
    for (summary_type, claimed) in summary_counts {
        let actual_key = match summary_type.as_str() {
            "Total public functions" => "Total public functions (actual)",
            "Functions with test coverage" => "Functions with test coverage (actual)",
            "Functions without test coverage" => "Functions without test coverage (actual)",
            "Total violations" => "Violation lines (actual)",
            "Total violations (summary)" => "Violation lines (actual)",
            _ => continue,
        };
        
        if let Some(&actual) = actual_counts.get(actual_key) {
            let matches = claimed == actual;
            checks.push(SummaryCheck {
                file: file_path.to_path_buf(),
                summary_type: summary_type.clone(),
                claimed_count: claimed,
                actual_count: actual,
                matches,
            });
        }
    }
    
    Ok(checks)
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    log!("Entering directory '{}'", base_dir.display());
    log!("");
    
    // Collect files to check
    let mut files_to_check: Vec<PathBuf> = Vec::new();
    
    if !args.paths.is_empty() {
        // Expand directories to .txt and .log files
        for path in &args.paths {
            if path.is_file() {
                files_to_check.push(path.clone());
            } else if path.is_dir() {
                let dir_files: Vec<PathBuf> = fs::read_dir(path)?
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter(|p| {
                        p.is_file() && 
                        (p.extension().is_some_and(|e| e == "txt" || e == "log"))
                    })
                    .collect();
                files_to_check.extend(dir_files);
            }
        }
    } else {
        // Default: check analyses directory
        let analyses_dir = base_dir.join("analyses");
        if analyses_dir.exists() {
            files_to_check = fs::read_dir(&analyses_dir)?
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file() && 
                    (path.extension().is_some_and(|e| e == "txt" || e == "log"))
                })
                .collect();
        }
    }
    
    if files_to_check.is_empty() {
        log!("No files to check. Specify -d analyses or -f analyses/review_test_functions.txt");
        return Ok(());
    }
    
    let mut all_checks = Vec::new();
    let mut files_checked = 0;
    
    for file in &files_to_check {
        if let Ok(checks) = check_file(file) {
            if !checks.is_empty() {
                files_checked += 1;
                all_checks.extend(checks);
            }
        }
    }
    
    // Report results
    let mismatches: Vec<&SummaryCheck> = all_checks.iter().filter(|c| !c.matches).collect();
    
    if mismatches.is_empty() {
        log!("✓ All summaries are accurate!");
        log!("");
        log!("Checked {} summaries across {} files", all_checks.len(), files_checked);
    } else {
        log!("✗ Found {} summary mismatches:", mismatches.len());
        log!("");
        
        for check in &mismatches {
            let file_name = check.file.file_name().unwrap().to_string_lossy();
            log!("{}:", file_name);
            log!("  {}: claimed {}, actual {}", 
                 check.summary_type, check.claimed_count, check.actual_count);
            log!("");
        }
    }
    
    // Summary
    let matches = all_checks.iter().filter(|c| c.matches).count();
    log!("================================================================================");
    log!("SUMMARY:");
    log!("  Files checked: {}", files_checked);
    log!("  Total summary checks: {}", all_checks.len());
    log!("  Accurate summaries: {} ({:.1}%)", matches, (matches as f64 / all_checks.len() as f64) * 100.0);
    log!("  Inaccurate summaries: {} ({:.1}%)", mismatches.len(), (mismatches.len() as f64 / all_checks.len() as f64) * 100.0);
    log!("================================================================================");
    
    let elapsed = start.elapsed().as_millis();
    log!("Completed in {}ms", elapsed);
    
    if !mismatches.is_empty() {
        std::process::exit(1);
    }
    
    Ok(())
}

