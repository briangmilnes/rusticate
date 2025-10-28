//! Review tool to find files using ArraySeqMtPer (which should not exist).
//!
//! Parallel algorithms require ephemerality (mutability), so ArraySeqMtPer
//! is incorrect. This tool identifies all files using it for deletion.
//!
//! Reports:
//! - Files in src/ using ArraySeqMtPer
//! - Files in tests/ using ArraySeqMtPer
//! - Files in benches/ using ArraySeqMtPer
//! - Lines of code in each file
//! - Whether an Eph equivalent exists
//! - Total LOC to be removed
//!
//! Uses AST parsing - NO STRING HACKING.
//!
//! Binary: review-mt-per

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition};
use rusticate::{find_rust_files, StandardArgs};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug)]
struct FileInfo {
    path: PathBuf,
    lines: usize,
    has_eph_equivalent: bool,
}

fn has_mt_per_import(content: &str) -> bool {
    let parsed = SourceFile::parse(content, Edition::Edition2021);
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find USE statements that import from ArraySeqMtPer module
    for node in root.descendants() {
        if node.kind() == SyntaxKind::USE {
            // Look for NAME_REF nodes within the USE statement
            for use_descendant in node.descendants() {
                if use_descendant.kind() == SyntaxKind::NAME_REF {
                    if let Some(name_ref) = ast::NameRef::cast(use_descendant.clone()) {
                        if name_ref.text() == "ArraySeqMtPer" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    
    false
}

fn count_lines(file_path: &Path) -> Result<usize> {
    let content = fs::read_to_string(file_path)?;
    Ok(content.lines().count())
}

fn check_eph_equivalent_exists(per_path: &Path, base_dir: &Path) -> bool {
    // Convert *MtPer.rs to *MtEph.rs
    if let Some(file_name) = per_path.file_name().and_then(|n| n.to_str()) {
        if file_name.ends_with("MtPer.rs") {
            let eph_name = file_name.replace("MtPer.rs", "MtEph.rs");
            if let Some(parent) = per_path.parent() {
                let eph_path = parent.join(eph_name);
                return eph_path.exists();
            }
        }
    }
    false
}

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/review_mt_per.log").ok();
    
    #[allow(unused_macros)]
    macro_rules! log {
        ($($arg:tt)*) => {{
            let msg = format!($($arg)*);
            println!("{}", msg);
            if let Some(ref mut f) = _log_file {
                use std::io::Write;
                let _ = writeln!(f, "{}", msg);
            }
        }};
    }
    
    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    let all_files = find_rust_files(&args.paths);
    
    log!("Entering directory '{}'", std::env::current_dir()?.display());
    println!();
    
    let mut src_files = Vec::new();
    let mut test_files = Vec::new();
    let mut bench_files = Vec::new();
    
    for file_path in all_files {
        let content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        if !has_mt_per_import(&content) {
            continue;
        }
        
        let lines = count_lines(&file_path)?;
        let has_eph_equivalent = check_eph_equivalent_exists(&file_path, &args.base_dir());
        
        let file_info = FileInfo {
            path: file_path.clone(),
            lines,
            has_eph_equivalent,
        };
        
        // Categorize by directory
        if file_path.to_str().is_some_and(|s| s.contains("/tests/")) {
            test_files.push(file_info);
        } else if file_path.to_str().is_some_and(|s| s.contains("/benches/")) {
            bench_files.push(file_info);
        } else if file_path.to_str().is_some_and(|s| s.contains("/src/")) {
            src_files.push(file_info);
        }
    }
    
    // Sort all lists by path
    src_files.sort_by(|a, b| a.path.cmp(&b.path));
    test_files.sort_by(|a, b| a.path.cmp(&b.path));
    bench_files.sort_by(|a, b| a.path.cmp(&b.path));
    
    // Calculate totals
    let src_total: usize = src_files.iter().map(|f| f.lines).sum();
    let test_total: usize = test_files.iter().map(|f| f.lines).sum();
    let bench_total: usize = bench_files.iter().map(|f| f.lines).sum();
    let grand_total = src_total + test_total + bench_total;
    
    // Report
    log!("{}", "=".repeat(80));
    log!("Files using ArraySeqMtPer (marked for deletion)");
    log!("{}", "=".repeat(80));
    println!();
    
    // src/ files
    log!("{}", "=".repeat(80));
    log!("src/ files using ArraySeqMtPer: {}", src_files.len());
    log!("{}", "=".repeat(80));
    for file_info in &src_files {
        let eph_status = if file_info.has_eph_equivalent { "HAS_EPH" } else { "NO_EPH" };
        log!("{}:1: {} lines [{}]", file_info.path.display(), file_info.lines, eph_status);
    }
    log!("src/ subtotal: {} lines", src_total);
    println!();
    
    // tests/ files
    log!("{}", "=".repeat(80));
    log!("tests/ files using ArraySeqMtPer: {}", test_files.len());
    log!("{}", "=".repeat(80));
    for file_info in &test_files {
        let eph_status = if file_info.has_eph_equivalent { "HAS_EPH" } else { "NO_EPH" };
        log!("{}:1: {} lines [{}]", file_info.path.display(), file_info.lines, eph_status);
    }
    log!("tests/ subtotal: {} lines", test_total);
    println!();
    
    // benches/ files
    log!("{}", "=".repeat(80));
    log!("benches/ files using ArraySeqMtPer: {}", bench_files.len());
    log!("{}", "=".repeat(80));
    for file_info in &bench_files {
        let eph_status = if file_info.has_eph_equivalent { "HAS_EPH" } else { "NO_EPH" };
        log!("{}:1: {} lines [{}]", file_info.path.display(), file_info.lines, eph_status);
    }
    log!("benches/ subtotal: {} lines", bench_total);
    println!();
    
    // Summary
    log!("{}", "=".repeat(80));
    log!("SUMMARY:");
    log!("  src/ files: {} ({} lines)", src_files.len(), src_total);
    log!("  tests/ files: {} ({} lines)", test_files.len(), test_total);
    log!("  benches/ files: {} ({} lines)", bench_files.len(), bench_total);
    log!("  TOTAL: {} files ({} lines to delete)", 
         src_files.len() + test_files.len() + bench_files.len(),
         grand_total);
    println!();
    
    // Count Eph equivalents
    let src_with_eph = src_files.iter().filter(|f| f.has_eph_equivalent).count();
    let test_with_eph = test_files.iter().filter(|f| f.has_eph_equivalent).count();
    let bench_with_eph = bench_files.iter().filter(|f| f.has_eph_equivalent).count();
    let total_with_eph = src_with_eph + test_with_eph + bench_with_eph;
    
    log!("Eph equivalents exist:");
    log!("  src/: {}/{}", src_with_eph, src_files.len());
    log!("  tests/: {}/{}", test_with_eph, test_files.len());
    log!("  benches/: {}/{}", bench_with_eph, bench_files.len());
    log!("  TOTAL: {}/{}", total_with_eph, 
         src_files.len() + test_files.len() + bench_files.len());
    
    let elapsed = start_time.elapsed();
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

