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
use ra_ap_syntax::{ast::{self, AstNode, HasName}, SyntaxKind, SourceFile, Edition};
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
    TestMtNotImportingMt {
        file: String,
    },
}

fn is_st_file(path: &Path) -> bool {
    let path_str = path.to_str().unwrap_or("");
    
    // Skip test files - they just test St modules
    if path_str.contains("/tests/") || path_str.contains("/benches/") {
        return false;
    }
    
    path_str.contains("St") && !path_str.contains("Mt")
}

fn is_mt_file(path: &Path) -> bool {
    let path_str = path.to_str().unwrap_or("");
    
    // Skip test files - they just call Mt modules, don't need to implement parallelism
    if path_str.contains("/tests/") || path_str.contains("/benches/") {
        return false;
    }
    
    path_str.contains("Mt")
}

fn test_file_imports_mt_module(content: &str) -> bool {
    // Check if test file imports an Mt module
    // Look for patterns like:
    // - use apas_ai::SomethingMt::...
    // - Chap06::UnDirGraphMtEph::...
    // - Any module name containing Mt
    
    let mut in_use_block = false;
    
    for line in content.lines() {
        let trimmed = line.trim();
        
        // Check if we're entering a use statement
        if trimmed.starts_with("use ") {
            in_use_block = true;
        }
        
        // If we're in a use block or on a use line, check for Mt
        if in_use_block || trimmed.starts_with("use ") {
            if trimmed.contains("Mt") {
                return true;
            }
            
            // End of use block
            if trimmed.ends_with(';') {
                in_use_block = false;
            }
        }
    }
    false
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
        "ParaPair",  // APAS parallel primitive
        "ParaPair!",
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
        "ParaPair!(",  // APAS parallel primitive usage
        "ParaPair(",
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
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let mut explosions = Vec::new();
    
    // Find all function definitions
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FN {
            if let Some(func) = ast::Fn::cast(node.clone()) {
                let fn_name = if let Some(name) = func.name() {
                    name.to_string()
                } else {
                    continue;
                };
                
                let line_num = content[..node.text_range().start().into()]
                    .chars()
                    .filter(|&c| c == '\n')
                    .count() + 1;
                
                let first_line = node.text().to_string().lines().next().unwrap_or("").to_string();
                
                // Count parallel operations in this function
                let mut spawn_count = 0;
                let mut is_recursive = false;
                
                // Check function body
                if let Some(body) = func.body() {
                    let body_syntax = body.syntax();
                    
                    // Count spawns via AST
                    for child in body_syntax.descendants() {
                        match child.kind() {
                            // Function calls: spawn(), rayon::join()
                            SyntaxKind::CALL_EXPR => {
                                if let Some(call) = ast::CallExpr::cast(child.clone()) {
                                    if let Some(expr) = call.expr() {
                                        // Check if it's a path expression
                                        if let Some(path_expr) = ast::PathExpr::cast(expr.syntax().clone()) {
                                            if let Some(path) = path_expr.path() {
                                                let path_str = path.to_string();
                                                // Check for spawn or rayon::join
                                                if path_str.ends_with("spawn") || 
                                                   path_str == "rayon::join" {
                                                    spawn_count += 1;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Macro calls: ParaPair!(), join!()
                            SyntaxKind::MACRO_CALL => {
                                if let Some(macro_call) = ast::MacroCall::cast(child.clone()) {
                                    if let Some(path) = macro_call.path() {
                                        let macro_name = path.to_string();
                                        if macro_name == "ParaPair" {
                                            spawn_count += 2; // ParaPair spawns 2 tasks
                                        } else if macro_name == "join" {
                                            spawn_count += 1;
                                        }
                                    }
                                }
                            }
                            // Method calls: .par_iter(), .join()
                            SyntaxKind::METHOD_CALL_EXPR => {
                                if let Some(method) = ast::MethodCallExpr::cast(child.clone()) {
                                    if let Some(name) = method.name_ref() {
                                        let method_name = name.to_string();
                                        if method_name == "par_iter" || 
                                           method_name == "par_bridge" ||
                                           method_name == "join" {
                                            spawn_count += 1;
                                        }
                                    }
                                }
                            }
                            // Check for recursion - function calls to self
                            SyntaxKind::PATH_EXPR => {
                                if let Some(path_expr) = ast::PathExpr::cast(child.clone()) {
                                    if let Some(path) = path_expr.path() {
                                        let path_str = path.to_string();
                                        if path_str == fn_name || 
                                           path_str == format!("Self::{}", fn_name) {
                                            is_recursive = true;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                
                // Flag if recursive and spawns any threads (exponential explosion)
                if is_recursive && spawn_count >= 1 {
                    explosions.push((line_num, first_line, spawn_count));
                }
                // Also flag non-recursive with many spawns
                else if spawn_count >= 4 {
                    explosions.push((line_num, first_line, spawn_count));
                }
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
    let path_str = file_path.to_str().unwrap_or("");
    
    // Check test files named TestXxxMt
    if (path_str.contains("/tests/") || path_str.contains("/benches/")) && 
       path_str.contains("Mt") {
        // Verify they actually import an Mt module
        if !test_file_imports_mt_module(&content) {
            violations.push(Violation::TestMtNotImportingMt {
                file: rel_path_str.clone(),
            });
        }
    }

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
    let mut test_mt_no_import: Vec<_> = Vec::new();

    for v in &all_violations {
        match v {
            Violation::StWithThreading { .. } => st_threading.push(v),
            Violation::MtWithoutThreading { .. } => mt_no_threading.push(v),
            Violation::MtWithThreshold { .. } => mt_thresholds.push(v),
            Violation::ThreadExplosion { .. } => thread_explosions.push(v),
            Violation::TestMtNotImportingMt { .. } => test_mt_no_import.push(v),
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

    if !test_mt_no_import.is_empty() {
        println!("{}", "=".repeat(80));
        println!("Test files named TestXxxMt not importing Mt modules:");
        println!("(These files should import and test the Mt implementation)");
        println!("{}", "=".repeat(80));
        for v in &test_mt_no_import {
            if let Violation::TestMtNotImportingMt { file } = v {
                println!("{}: No Mt module imported", file);
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
    println!("  Test Mt files not importing Mt: {}", test_mt_no_import.len());
    println!("  Total violations: {}", all_violations.len());

    let elapsed = start_time.elapsed();
    eprintln!("\nCompleted in {}ms", elapsed.as_millis());

    Ok(())
}

