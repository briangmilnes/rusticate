// Copyright (C) Brian G. Milnes 2025

//! Review: Minimize UFCS at call sites.
//!
//! RustRules.md Lines 309-320: "Replace <Type as Trait>::method(...) at call sites
//! with method-call syntax wherever possible. Keep UFCS inside impls/traits for
//! disambiguation; minimize UFCS in callers."
//!
//! Checks src/, tests/, and benches/ for UFCS usage outside of impl/trait blocks.
//!
//! Note: Some UFCS usage may be legitimate (primitives, macros, disambiguation).
//! This check identifies candidates for review, not automatic violations.
//! Git commit: 584a672b6a34782766863c5f76a461d3297a741a
//! Binary: rusticate-review-minimize-ufcs-call-sites

use anyhow::Result;
use rusticate::{StandardArgs, format_number, parse_source, find_rust_files, line_number};
use ra_ap_syntax::{SyntaxKind, SyntaxNode, ast::AstNode};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::io::{self, Write};


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_minimize_ufcs_call_sites.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    ufcs_text: String,
    code_context: String,
}

/// Check if a node is inside an impl or trait block
fn is_inside_impl_or_trait(node: &SyntaxNode) -> bool {
    node.ancestors().any(|ancestor| {
        matches!(ancestor.kind(), SyntaxKind::IMPL | SyntaxKind::TRAIT)
    })
}

/// Check if this path expression looks like UFCS: <Type as Trait>::method using AST
fn is_ufcs_path(path_node: &SyntaxNode) -> bool {
    // UFCS patterns have PATH nodes with a generic_arg_list containing AS_KW
    // Structure: PATH -> PATH_SEGMENT -> GENERIC_ARG_LIST -> (contains AS keyword)
    for child in path_node.descendants() {
        if child.kind() == SyntaxKind::GENERIC_ARG_LIST {
            // Check if there's an AS keyword in the generic args (indicates "Type as Trait")
            for token in child.descendants_with_tokens() {
                if let Some(token) = token.as_token() {
                    if token.kind() == SyntaxKind::AS_KW {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn check_file(file_path: &Path, source: &str) -> Result<Vec<Violation>> {
    let source_file = parse_source(source)?;
    let root = source_file.syntax();
    
    let mut violations = Vec::new();

    // Find all PATH expressions
    for node in root.descendants() {
        if node.kind() == SyntaxKind::PATH {
            // Check if this looks like UFCS
            if is_ufcs_path(&node) {
                // Check if we're inside impl or trait
                if !is_inside_impl_or_trait(&node) {
                    let ufcs_text = node.text().to_string();
                    let line_num = line_number(&node, source);
                    let code_context = source.lines().nth(line_num - 1)
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    
                    violations.push(Violation {
                        file: file_path.to_path_buf(),
                        line_num,
                        ufcs_text,
                        code_context,
                    });
                }
            }
        }
    }
    
    Ok(violations)
}

fn print_line(line: &str) -> Result<()> {
    writeln!(io::stdout(), "{line}")?;
    Ok(())
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();

    if !base_dir.exists() {
        print_line(&format!("Error: Base directory not found: {}", base_dir.display()))?;
        std::process::exit(1);
    }

    print_line(&format!("Entering directory '{}'", base_dir.display()))?;

    let search_dirs = args.get_search_dirs();
    let rust_files = find_rust_files(&search_dirs);

    let mut all_violations: Vec<Violation> = Vec::new();
    let mut files_checked = 0;

    for file_path in &rust_files {
        files_checked += 1;
        let source_code = fs::read_to_string(file_path)?;
        let violations = check_file(file_path, &source_code)?;
        all_violations.extend(violations);
    }

    let total_violations = all_violations.len();
    let files_with_violations = all_violations.iter()
        .map(|v| &v.file)
        .collect::<HashSet<_>>()
        .len();

    if total_violations > 0 {
        print_line("✗ UFCS at call sites found (RustRules.md Lines 309-320):")?;
        print_line("Replace <Type as Trait>::method(...) with method-call syntax where possible.")?;
        print_line("Keep UFCS inside impls/traits; minimize in callers.")?;
        print_line("")?;

        for v in &all_violations {
            let rel_path = v.file.strip_prefix(&base_dir).unwrap_or(&v.file);
            print_line(&format!("{}:{}: UFCS usage: {}",
                rel_path.display(),
                v.line_num,
                v.ufcs_text
            ))?;
            print_line(&format!("    {}", v.code_context))?;
        }
        print_line("")?;
    } else {
        print_line("✓ No UFCS at call sites (outside impl/trait)")?;
    }

    let elapsed = start.elapsed().as_millis();
    print_line(&format!("Summary: {} files checked, {} files with violations, {} total violations",
        format_number(files_checked),
        format_number(files_with_violations),
        format_number(total_violations)
    ))?;
    print_line(&format!("Completed in {elapsed}ms"))?;

    if total_violations > 0 {
        std::process::exit(1);
    }

    Ok(())
}
