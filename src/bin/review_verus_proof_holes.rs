use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SyntaxNode};
use rusticate::{StandardArgs, parse_source, find_rust_files};
use std::{collections::HashMap, fs, path::{Path, PathBuf}, time::Instant};

macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/rusticate-review-verus-proof-holes.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

#[derive(Debug, Default, Clone)]
struct ProofHoleStats {
    assume_false_count: usize,
    assume_count: usize,
    admit_count: usize,
    external_body_count: usize,
    total_holes: usize,
}

#[derive(Debug, Default)]
struct FileStats {
    holes: ProofHoleStats,
    proof_functions: usize,
    clean_proof_functions: usize,
    holed_proof_functions: usize,
}

#[derive(Debug, Default)]
struct SummaryStats {
    total_files: usize,
    clean_modules: usize,
    holed_modules: usize,
    total_proof_functions: usize,
    clean_proof_functions: usize,
    holed_proof_functions: usize,
    holes: ProofHoleStats,
}

fn main() -> Result<()> {
    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    
    if args.language != "Verus" {
        anyhow::bail!("This tool requires -l Verus flag");
    }
    
    log!("Verus Proof Hole Detection");
    log!("Looking for: assume(false), assume(), admit(), #[verifier::external_body]");
    log!("");
    
    // Collect all Rust files from the specified paths
    let mut all_files: Vec<PathBuf> = Vec::new();
    let base_dir = args.base_dir();
    let search_dirs = args.get_search_dirs();
    
    for dir in search_dirs {
        all_files.extend(find_rust_files(&[dir]));
    }
    
    let mut file_stats_map: HashMap<String, FileStats> = HashMap::new();
    
    for file in &all_files {
        if let Ok(stats) = analyze_file(file) {
            // Use relative path if possible
            let path_str = if let Ok(rel_path) = file.strip_prefix(&base_dir) {
                rel_path.display().to_string()
            } else {
                file.display().to_string()
            };
            print_file_report(&path_str, &stats);
            file_stats_map.insert(path_str, stats);
        }
    }
    
    // Print summary
    let summary = compute_summary(&file_stats_map);
    print_summary(&summary);
    
    let elapsed = start_time.elapsed();
    log!("");
    log!("Completed in {}ms", elapsed.as_millis());
    
    Ok(())
}

fn analyze_file(path: &Path) -> Result<FileStats> {
    let content = fs::read_to_string(path)?;
    let source_file = parse_source(&content)?;
    let root = source_file.syntax();
    
    let mut stats = FileStats::default();
    
    // Find verus! {} macros
    for node in root.descendants() {
        if node.kind() == SyntaxKind::MACRO_CALL {
            if let Some(macro_call) = ast::MacroCall::cast(node.clone()) {
                if let Some(macro_path) = macro_call.path() {
                    if macro_path.to_string() == "verus" {
                        if let Some(token_tree) = macro_call.token_tree() {
                            analyze_verus_macro(token_tree.syntax(), &content, &mut stats);
                        }
                    }
                }
            }
        }
    }
    
    Ok(stats)
}

fn analyze_verus_macro(tree: &SyntaxNode, content: &str, stats: &mut FileStats) {
    // Walk the token tree looking for:
    // 1. Functions with proof modifier
    // 2. Attributes #[verifier::external_body]
    // 3. Function calls to assume/admit
    
    let tokens: Vec<_> = tree.descendants_with_tokens()
        .filter_map(|n| n.into_token())
        .collect();
    
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        
        // Look for #[verifier::external_body]
        if token.kind() == SyntaxKind::POUND && i + 1 < tokens.len() {
            if is_external_body_attribute(&tokens, i) {
                stats.holes.external_body_count += 1;
                stats.holes.total_holes += 1;
            }
        }
        
        // Look for "fn" keyword to find proof functions
        if token.kind() == SyntaxKind::FN_KW {
            let is_proof = is_proof_function(&tokens, i);
            
            if is_proof {
                stats.proof_functions += 1;
                
                // Check if this proof function has holes
                let holes_in_function = count_holes_in_function(&tokens, i);
                if holes_in_function > 0 {
                    stats.holed_proof_functions += 1;
                } else {
                    stats.clean_proof_functions += 1;
                }
            }
        }
        
        // Look for assume/admit function calls
        if token.kind() == SyntaxKind::IDENT {
            let text = token.text();
            if text == "assume" || text == "admit" {
                // Check if it's followed by (
                if i + 1 < tokens.len() && tokens[i + 1].kind() == SyntaxKind::L_PAREN {
                    if text == "assume" {
                        // Check if it's assume(false)
                        if i + 2 < tokens.len() && tokens[i + 2].text() == "false" {
                            stats.holes.assume_false_count += 1;
                        } else {
                            stats.holes.assume_count += 1;
                        }
                        stats.holes.total_holes += 1;
                    } else if text == "admit" {
                        stats.holes.admit_count += 1;
                        stats.holes.total_holes += 1;
                    }
                }
            }
        }
        
        i += 1;
    }
}

fn is_external_body_attribute(tokens: &[ra_ap_syntax::SyntaxToken], start_idx: usize) -> bool {
    // Look for pattern: # [ verifier :: external_body ]
    let mut i = start_idx;
    
    if i >= tokens.len() || tokens[i].kind() != SyntaxKind::POUND {
        return false;
    }
    i += 1;
    
    // Skip whitespace
    while i < tokens.len() && tokens[i].kind() == SyntaxKind::WHITESPACE {
        i += 1;
    }
    
    if i >= tokens.len() || tokens[i].kind() != SyntaxKind::L_BRACK {
        return false;
    }
    i += 1;
    
    // Look for "verifier"
    while i < tokens.len() {
        if tokens[i].kind() == SyntaxKind::IDENT && tokens[i].text() == "verifier" {
            // Look for :: external_body
            let mut j = i + 1;
            while j < tokens.len() && tokens[j].kind() == SyntaxKind::WHITESPACE {
                j += 1;
            }
            if j + 2 < tokens.len() 
                && tokens[j].kind() == SyntaxKind::COLON2 
                && tokens[j + 1].kind() == SyntaxKind::IDENT 
                && tokens[j + 1].text() == "external_body" {
                return true;
            }
        }
        
        if tokens[i].kind() == SyntaxKind::R_BRACK {
            break;
        }
        i += 1;
    }
    
    false
}

fn is_proof_function(tokens: &[ra_ap_syntax::SyntaxToken], fn_idx: usize) -> bool {
    // Look backwards for "proof" modifier
    let start_idx = if fn_idx >= 10 { fn_idx - 10 } else { 0 };
    for j in start_idx..fn_idx {
        if tokens[j].kind() == SyntaxKind::IDENT && tokens[j].text() == "proof" {
            return true;
        }
    }
    false
}

fn count_holes_in_function(tokens: &[ra_ap_syntax::SyntaxToken], fn_idx: usize) -> usize {
    // Find the function body (from fn to its closing brace)
    let mut i = fn_idx + 1;
    
    // Find opening brace
    while i < tokens.len() && tokens[i].kind() != SyntaxKind::L_CURLY {
        i += 1;
    }
    
    if i >= tokens.len() {
        return 0;
    }
    
    let start = i;
    let mut brace_depth = 1;
    i += 1;
    
    // Find matching closing brace
    while i < tokens.len() && brace_depth > 0 {
        match tokens[i].kind() {
            SyntaxKind::L_CURLY => brace_depth += 1,
            SyntaxKind::R_CURLY => brace_depth -= 1,
            _ => {}
        }
        i += 1;
    }
    
    let end = i;
    
    // Count holes in this range
    let mut holes = 0;
    for j in start..end {
        if tokens[j].kind() == SyntaxKind::IDENT {
            let text = tokens[j].text();
            if (text == "assume" || text == "admit") 
                && j + 1 < end 
                && tokens[j + 1].kind() == SyntaxKind::L_PAREN {
                holes += 1;
            }
        }
        
        // Check for #[verifier::external_body]
        if tokens[j].kind() == SyntaxKind::POUND {
            if is_external_body_attribute(tokens, j) {
                holes += 1;
            }
        }
    }
    
    holes
}

fn print_file_report(path: &str, stats: &FileStats) {
    let has_holes = stats.holes.total_holes > 0;
    
    if has_holes {
        log!("❌ {}", path);
        log!("   Holes: {} total", stats.holes.total_holes);
        
        if stats.holes.assume_false_count > 0 {
            log!("      {} × assume(false)", stats.holes.assume_false_count);
        }
        if stats.holes.assume_count > 0 {
            log!("      {} × assume()", stats.holes.assume_count);
        }
        if stats.holes.admit_count > 0 {
            log!("      {} × admit()", stats.holes.admit_count);
        }
        if stats.holes.external_body_count > 0 {
            log!("      {} × #[verifier::external_body]", stats.holes.external_body_count);
        }
        
        if stats.proof_functions > 0 {
            log!("   Proof functions: {} total ({} clean, {} holed)", 
                 stats.proof_functions, 
                 stats.clean_proof_functions, 
                 stats.holed_proof_functions);
        }
    } else {
        log!("✓ {}", path);
        if stats.proof_functions > 0 {
            log!("   {} clean proof function{}", 
                 stats.proof_functions,
                 if stats.proof_functions == 1 { "" } else { "s" });
        }
    }
}

fn compute_summary(file_stats_map: &HashMap<String, FileStats>) -> SummaryStats {
    let mut summary = SummaryStats::default();
    
    for stats in file_stats_map.values() {
        summary.total_files += 1;
        
        if stats.holes.total_holes > 0 {
            summary.holed_modules += 1;
        } else {
            summary.clean_modules += 1;
        }
        
        summary.total_proof_functions += stats.proof_functions;
        summary.clean_proof_functions += stats.clean_proof_functions;
        summary.holed_proof_functions += stats.holed_proof_functions;
        
        summary.holes.assume_false_count += stats.holes.assume_false_count;
        summary.holes.assume_count += stats.holes.assume_count;
        summary.holes.admit_count += stats.holes.admit_count;
        summary.holes.external_body_count += stats.holes.external_body_count;
        summary.holes.total_holes += stats.holes.total_holes;
    }
    
    summary
}

fn print_summary(summary: &SummaryStats) {
    log!("");
    log!("═══════════════════════════════════════════════════════════════");
    log!("SUMMARY");
    log!("═══════════════════════════════════════════════════════════════");
    log!("");
    log!("Modules:");
    log!("   {} clean (no holes)", summary.clean_modules);
    log!("   {} holed (contains holes)", summary.holed_modules);
    log!("   {} total", summary.total_files);
    log!("");
    log!("Proof Functions:");
    log!("   {} clean", summary.clean_proof_functions);
    log!("   {} holed", summary.holed_proof_functions);
    log!("   {} total", summary.total_proof_functions);
    log!("");
    log!("Holes Found: {} total", summary.holes.total_holes);
    if summary.holes.assume_false_count > 0 {
        log!("   {} × assume(false)", summary.holes.assume_false_count);
    }
    if summary.holes.assume_count > 0 {
        log!("   {} × assume()", summary.holes.assume_count);
    }
    if summary.holes.admit_count > 0 {
        log!("   {} × admit()", summary.holes.admit_count);
    }
    if summary.holes.external_body_count > 0 {
        log!("   {} × #[verifier::external_body]", summary.holes.external_body_count);
    }
    
    if summary.holes.total_holes == 0 {
        log!("");
        log!("🎉 No proof holes found! All proofs are complete.");
    }
}

