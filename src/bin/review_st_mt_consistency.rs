//! Review: St/Mt Consistency
//!
//! Checks that single-threaded (St) and multi-threaded (Mt) files are properly implemented:
//!
//! St files should NOT have:
//! - Threading imports (std::thread, rayon, crossbeam, etc.)
//! - Parallel operations
//! - Send/Sync bounds (except in trait definitions)
//!
//! Mt files MUST have:
//! - Threading imports
//! - Actual parallel execution (not just imports)
//!
//! Mt files should NOT have:
//! - Threshold checks like "if n < 8 { serial_version() }"
//! - APAS is a class example - always demonstrate parallelism
//!
//! Binary: rusticate-review-st-mt-consistency

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition};
use rusticate::{StandardArgs, find_rust_files};
use std::path::Path;
use std::time::Instant;

#[derive(Debug)]
enum Violation {
    StWithThreading {
        file: String,
        line: usize,
        content: String,
    },
    MtWithoutThreading {
        file: String,
    },
    MtWithThreshold {
        file: String,
        line: usize,
        content: String,
    },
    ThreadExplosion {
        file: String,
        line: usize,
        content: String,
        spawn_count: usize,
    },
}

fn is_st_file(path: &Path) -> bool {
    let path_str = path.to_str().unwrap_or("");
    path_str.contains("St") && !path_str.contains("Mt")
}

fn is_mt_file(path: &Path) -> bool {
    let path_str = path.to_str().unwrap_or("");
    path_str.contains("Mt")
}

fn has_threading_imports(content: &str) -> bool {
    let threading_indicators = [
        "std::thread",
        "rayon::",
        "crossbeam",
        "par_iter",
        "par_bridge",
        "ParallelIterator",
        "spawn(",
        ".join(",
        "thread::spawn",
        "scope(",
    ];
    
    for indicator in &threading_indicators {
        if content.contains(indicator) {
            return true;
        }
    }
    false
}

fn has_parallel_usage(content: &str) -> bool {
    // Check for actual parallel operations, not just imports
    let parallel_ops = [
        "par_iter()",
        "par_bridge()",
        ".par_iter",
        ".par_bridge",
        "parallel_",
        "spawn(",
        "scope(",
        "rayon::join",
        "rayon::scope",
    ];
    
    for op in &parallel_ops {
        if content.contains(op) {
            return true;
        }
    }
    false
}

fn find_threading_usage(file_path: &Path, content: &str) -> Vec<(usize, String)> {
    let mut usages = Vec::new();
    
    for (i, line) in content.lines().enumerate() {
        let line_num = i + 1;
        
        // Skip use statements - we only care about actual usage
        if line.trim().starts_with("use ") {
            continue;
        }
        
        // Skip comments
        if line.trim().starts_with("//") {
            continue;
        }
        
        // Check for threading constructs
        let threading_patterns = [
            "std::thread::",
            "thread::spawn",
            "par_iter",
            "par_bridge",
            "rayon::",
            ".spawn(",
            "crossbeam",
        ];
        
        for pattern in &threading_patterns {
            if line.contains(pattern) {
                usages.push((line_num, line.trim().to_string()));
                break;
            }
        }
    }
    
    usages
}

fn find_threshold_checks(content: &str) -> Vec<(usize, String)> {
    let mut thresholds = Vec::new();
    
    for (i, line) in content.lines().enumerate() {
        let line_num = i + 1;
        let trimmed = line.trim();
        
        // Look for patterns like: if n < 8, if len() < 16, if size < threshold, etc.
        let threshold_patterns = [
            "if n <",
            "if n <=",
            "if len() <",
            "if size <",
            "if length <",
            "if count <",
            "< 8",
            "< 16",
            "< 32",
            "< 64",
            "< 128",
            "< THRESHOLD",
            "< MIN_",
            "< CUTOFF",
        ];
        
        for pattern in &threshold_patterns {
            if trimmed.contains(pattern) {
                // Check if this is followed by serial/sequential logic
                let lower = trimmed.to_lowercase();
                if lower.contains("serial") || 
                   lower.contains("sequential") ||
                   lower.contains("return ") ||
                   lower.contains("{") {
                    thresholds.push((line_num, trimmed.to_string()));
                    break;
                }
            }
        }
    }
    
    thresholds
}

fn analyze_thread_explosion(content: &str) -> Vec<(usize, String, usize)> {
    let mut explosions = Vec::new();
    
    // Look for functions that spawn threads
    let lines: Vec<&str> = content.lines().collect();
    
    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;
        let trimmed = line.trim();
        
        // Skip comments
        if trimmed.starts_with("//") {
            continue;
        }
        
        // Look for function definitions (including indented ones in impl blocks)
        if (trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ")) && trimmed.contains('(') {
            // Check if this function spawns multiple threads
            let mut spawn_count = 0;
            let mut is_recursive = false;
            
            // Get function name
            let fn_name = if let Some(start) = trimmed.find("fn ") {
                let after_fn = &trimmed[start + 3..];
                if let Some(paren) = after_fn.find('(') {
                    after_fn[..paren].trim()
                } else {
                    ""
                }
            } else {
                ""
            };
            
            // Scan function body (next ~100 lines)
            for j in i..std::cmp::min(i + 100, lines.len()) {
                let body_line = lines[j];
                
                // Count thread spawns
                if body_line.contains("spawn(") || 
                   body_line.contains("par_iter") ||
                   body_line.contains("scope(|s| {") ||
                   body_line.contains(".join(") {
                    spawn_count += 1;
                }
                
                // Check for multiple spawns in one statement (e.g., let (a, b) = rayon::join(...))
                if body_line.contains("rayon::join") || body_line.contains("join!(") {
                    spawn_count += 1; // rayon::join spawns 2 tasks
                }
                
                // Check if function calls itself (recursion)
                if !fn_name.is_empty() && body_line.contains('(') {
                    // Check for direct recursion or Self::function_name recursion
                    let has_direct_call = body_line.contains(fn_name);
                    let has_self_call = body_line.contains(&format!("Self::{}", fn_name)) ||
                                       body_line.contains(&format!("self.{}", fn_name));
                    
                    if (has_direct_call || has_self_call) && j != i && !body_line.trim().starts_with("//") {
                        is_recursive = true;
                    }
                }
                
                // Stop at next function definition
                if j > i && (lines[j].trim().starts_with("fn ") || lines[j].trim().starts_with("pub fn ")) {
                    break;
                }
            }
            
            // Flag if recursive and spawns any threads (exponential explosion)
            // Even spawning 1 thread per recursive call leads to exponential growth
            if is_recursive && spawn_count >= 1 {
                explosions.push((line_num, trimmed.to_string(), spawn_count));
            }
            // Also flag non-recursive with many spawns
            else if spawn_count >= 4 {
                explosions.push((line_num, trimmed.to_string(), spawn_count));
            }
        }
    }
    
    explosions
}

fn analyze_file(file_path: &Path, base_dir: &Path) -> Vec<Violation> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut violations = Vec::new();
    let rel_path = file_path.strip_prefix(base_dir).unwrap_or(file_path);
    let rel_path_str = rel_path.display().to_string();

    // Check St files
    if is_st_file(file_path) {
        let threading_usages = find_threading_usage(file_path, &content);
        for (line, content) in threading_usages {
            violations.push(Violation::StWithThreading {
                file: rel_path_str.clone(),
                line,
                content,
            });
        }
    }

    // Check Mt files
    if is_mt_file(file_path) {
        // Check for missing threading
        if !has_threading_imports(&content) || !has_parallel_usage(&content) {
            violations.push(Violation::MtWithoutThreading {
                file: rel_path_str.clone(),
            });
        }
        
        // Check for threshold-based serial fallback
        let thresholds = find_threshold_checks(&content);
        for (line, content) in thresholds {
            violations.push(Violation::MtWithThreshold {
                file: rel_path_str.clone(),
                line,
                content,
            });
        }
        
        // Check for thread explosion risk
        let explosions = analyze_thread_explosion(&content);
        for (line, content, spawn_count) in explosions {
            violations.push(Violation::ThreadExplosion {
                file: rel_path_str.clone(),
                line,
                content,
                spawn_count,
            });
        }
    }

    violations
}

fn main() -> Result<()> {
    let start_time = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();

    let files = find_rust_files(&args.paths);

    let mut all_violations = Vec::new();

    for file_path in &files {
        let violations = analyze_file(file_path, &base_dir);
        all_violations.extend(violations);
    }

    if all_violations.is_empty() {
        println!("✓ St/Mt Consistency: All files properly implement single/multi-threading");
        let elapsed = start_time.elapsed();
        eprintln!("\nCompleted in {}ms", elapsed.as_millis());
        return Ok(());
    }

    // Group violations by type
    let mut st_threading: Vec<_> = Vec::new();
    let mut mt_no_threading: Vec<_> = Vec::new();
    let mut mt_thresholds: Vec<_> = Vec::new();
    let mut thread_explosions: Vec<_> = Vec::new();

    for v in &all_violations {
        match v {
            Violation::StWithThreading { .. } => st_threading.push(v),
            Violation::MtWithoutThreading { .. } => mt_no_threading.push(v),
            Violation::MtWithThreshold { .. } => mt_thresholds.push(v),
            Violation::ThreadExplosion { .. } => thread_explosions.push(v),
        }
    }

    println!("✗ St/Mt Consistency violations found:\n");

    if !st_threading.is_empty() {
        println!("{}", "=".repeat(80));
        println!("St files with threading (should be single-threaded only):");
        println!("{}", "=".repeat(80));
        for v in &st_threading {
            if let Violation::StWithThreading { file, line, content } = v {
                println!("{}:{}: {}", file, line, content);
            }
        }
        println!();
    }

    if !mt_no_threading.is_empty() {
        println!("{}", "=".repeat(80));
        println!("Mt files without threading (should use parallel operations):");
        println!("{}", "=".repeat(80));
        for v in &mt_no_threading {
            if let Violation::MtWithoutThreading { file } = v {
                println!("{}: Missing parallel operations", file);
            }
        }
        println!();
    }

    if !mt_thresholds.is_empty() {
        println!("{}", "=".repeat(80));
        println!("Mt files with threshold-based serial fallback:");
        println!("(APAS is a class example - should always demonstrate parallelism)");
        println!("{}", "=".repeat(80));
        for v in &mt_thresholds {
            if let Violation::MtWithThreshold { file, line, content } = v {
                println!("{}:{}: {}", file, line, content);
            }
        }
        println!();
    }

    if !thread_explosions.is_empty() {
        println!("{}", "=".repeat(80));
        println!("THREAD EXPLOSION RISK - Recursive functions spawning multiple threads:");
        println!("(Recommendation: Use a thread pool like rayon instead of raw spawning)");
        println!("{}", "=".repeat(80));
        for v in &thread_explosions {
            if let Violation::ThreadExplosion { file, line, content, spawn_count } = v {
                println!("{}:{}: {} spawn(s) detected", file, line, spawn_count);
                println!("  {}", content);
                
                // Calculate potential thread explosion
                let depth = 5; // Assume typical recursion depth
                let potential_threads = spawn_count.pow(depth as u32);
                println!("  → Potential threads at depth {}: {} ({}^{})", 
                    depth, potential_threads, spawn_count, depth);
                
                if potential_threads > 1000 {
                    println!("  ⚠️  CRITICAL: Exponential thread explosion detected!");
                    println!("  📋 RECOMMENDATION: Replace raw spawning with rayon thread pool");
                    println!("     - Use rayon::join() instead of manual spawn()");
                    println!("     - Use .par_iter() for collections");
                    println!("     - Thread pool automatically limits parallelism");
                }
                println!();
            }
        }
        println!();
    }

    println!("{}", "=".repeat(80));
    println!("SUMMARY:");
    println!("  St files with threading: {}", st_threading.len());
    println!("  Mt files without threading: {}", mt_no_threading.len());
    println!("  Mt files with thresholds: {}", mt_thresholds.len());
    println!("  Mt files with thread explosion risk: {}", thread_explosions.len());
    println!("  Total violations: {}", all_violations.len());

    let elapsed = start_time.elapsed();
    eprintln!("\nCompleted in {}ms", elapsed.as_millis());

    Ok(())
}

