//! Review: Binary Logging
//!
//! Ensures all rusticate binaries log to analyses/binaryname.log
//! Uses AST parsing to check for proper logging setup in main() functions
//!
//! Binary: rusticate-review-binary-logging

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode, HasName}, SyntaxKind, SourceFile, Edition};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug)]
struct BinaryInfo {
    name: String,
    path: PathBuf,
    has_main: bool,
    has_file_logging: bool,
    uses_analyses_dir: bool,
    log_path_expr: Option<String>,
}

#[derive(Debug)]
enum Violation {
    NoMainFunction {
        binary: String,
    },
    NoFileLogging {
        binary: String,
    },
    WrongLogDirectory {
        binary: String,
        found_path: String,
        expected_path: String,
    },
}

fn extract_binary_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn find_main_function(root: &ra_ap_syntax::SyntaxNode) -> Option<ast::Fn> {
    for node in root.descendants() {
        if node.kind() == SyntaxKind::FN {
            if let Some(func) = ast::Fn::cast(node.clone()) {
                if let Some(name) = func.name() {
                    if name.to_string() == "main" {
                        return Some(func);
                    }
                }
            }
        }
    }
    None
}

fn check_for_file_logging(main_fn: &ast::Fn, binary_name: &str) -> (bool, bool, Option<String>) {
    let mut has_file_logging = false;
    let mut uses_analyses_dir = false;
    let mut log_path_expr = None;
    
    if let Some(body) = main_fn.body() {
        let body_text = body.to_string();
        
        // Check for File::create patterns
        if body_text.contains("File::create") || body_text.contains("OpenOptions::new") {
            has_file_logging = true;
            
            // Look for the expected pattern: analyses/{binary_name}.log
            let expected_path = format!("analyses/{}.log", binary_name);
            
            // Check if analyses directory is used
            if body_text.contains("analyses/") || body_text.contains("\"analyses\"") {
                uses_analyses_dir = true;
                
                // Try to extract the actual path expression
                for line in body_text.lines() {
                    if line.contains("analyses/") {
                        log_path_expr = Some(line.trim().to_string());
                        break;
                    }
                }
                
                // Verify it matches expected pattern
                if !body_text.contains(&expected_path) {
                    uses_analyses_dir = false; // Wrong path
                }
            }
        }
        
        // Check for writeln! or write! to file handles (indicating file logging)
        if body_text.contains("writeln!(log_file") || 
           body_text.contains("write!(log_file") ||
           body_text.contains("writeln!(file") ||
           body_text.contains("write!(file") {
            has_file_logging = true;
        }
    }
    
    (has_file_logging, uses_analyses_dir, log_path_expr)
}

fn analyze_binary(file_path: &Path) -> Result<BinaryInfo> {
    let binary_name = extract_binary_name_from_path(file_path);
    let content = fs::read_to_string(file_path)?;
    
    let parsed = SourceFile::parse(&content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    let main_fn = find_main_function(&root);
    let has_main = main_fn.is_some();
    
    let (has_file_logging, uses_analyses_dir, log_path_expr) = if let Some(ref main) = main_fn {
        check_for_file_logging(main, &binary_name)
    } else {
        (false, false, None)
    };
    
    Ok(BinaryInfo {
        name: binary_name,
        path: file_path.to_path_buf(),
        has_main,
        has_file_logging,
        uses_analyses_dir,
        log_path_expr,
    })
}

fn main() -> Result<()> {
    
    // Setup logging to analyses/review_binary_logging.log
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = File::create("analyses/review_binary_logging.log").ok();
    
    #[allow(unused_macros)]
    macro_rules! log {
        ($($arg:tt)*) => {{
            let msg = format!($($arg)*);
            println!("{}", msg);
            if let Some(ref mut f) = _log_file {
                let _ = writeln!(f, "{}", msg);
            }
        }};
    }
let start_time = Instant::now();
    
    let bin_dir = PathBuf::from("src/bin");
    
    if !bin_dir.exists() {
        eprintln!("Error: src/bin directory not found");
        std::process::exit(1);
    }
    
    log!("Entering directory '{}'", std::env::current_dir()?.display());
    println!();
    
    let mut binaries = Vec::new();
    let mut violations = Vec::new();
    
    // Find all .rs files in src/bin/
    for entry in fs::read_dir(&bin_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            match analyze_binary(&path) {
                Ok(info) => {
                    // Check for violations
                    if !info.has_main {
                        violations.push(Violation::NoMainFunction {
                            binary: info.name.clone(),
                        });
                    } else if !info.has_file_logging {
                        violations.push(Violation::NoFileLogging {
                            binary: info.name.clone(),
                        });
                    } else if !info.uses_analyses_dir {
                        let expected_path = format!("analyses/{}.log", info.name);
                        violations.push(Violation::WrongLogDirectory {
                            binary: info.name.clone(),
                            found_path: info.log_path_expr.clone().unwrap_or_else(|| "unknown".to_string()),
                            expected_path,
                        });
                    }
                    
                    binaries.push(info);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to analyze {}: {}", path.display(), e);
                }
            }
        }
    }
    
    // Report results
    if violations.is_empty() {
        log!("✓ All binaries properly log to analyses/binaryname.log");
    } else {
        log!("✗ Binary logging violations found:");
        println!();
        
        let mut no_main_count = 0;
        let mut no_logging_count = 0;
        let mut wrong_dir_count = 0;
        
        for violation in &violations {
            match violation {
                Violation::NoMainFunction { binary } => {
                    log!("src/bin/{}.rs: No main function found", binary);
                    no_main_count += 1;
                }
                Violation::NoFileLogging { binary } => {
                    log!("src/bin/{}.rs: No file logging detected - BUG", binary);
                    log!("  Expected: File logging to analyses/{}.log", binary);
                    no_logging_count += 1;
                }
                Violation::WrongLogDirectory { binary, found_path, expected_path } => {
                    log!("src/bin/{}.rs: Wrong log directory - BUG", binary);
                    log!("  Found: {}", found_path);
                    log!("  Expected: {}", expected_path);
                    wrong_dir_count += 1;
                }
            }
            println!();
        }
        
        log!("{}", "=".repeat(80));
        log!("SUMMARY:");
        log!("  Total binaries: {}", binaries.len());
        log!("  Binaries without main: {}", no_main_count);
        log!("  Binaries without file logging: {}", no_logging_count);
        log!("  Binaries with wrong log directory: {}", wrong_dir_count);
        log!("  Total violations: {}", violations.len());
    }
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    if !violations.is_empty() {
        std::process::exit(1);
    }
    
    Ok(())
}

