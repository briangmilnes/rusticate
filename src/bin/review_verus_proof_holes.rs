use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SyntaxNode};
use rusticate::{StandardArgs, find_rust_files};
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

#[derive(Debug, Clone, PartialEq)]
enum VerifierAttribute {
    ExternalBody,
    ExternalFnSpec,
    ExternalTraitSpec,
    ExternalTypeSpec,
    ExternalTraitExt,
    External,
    Opaque,
}

#[derive(Debug, Default, Clone)]
struct ProofHoleStats {
    assume_false_count: usize,
    assume_count: usize,
    admit_count: usize,
    external_body_count: usize,
    external_fn_spec_count: usize,
    external_trait_spec_count: usize,
    external_type_spec_count: usize,
    external_trait_ext_count: usize,
    external_count: usize,
    opaque_count: usize,
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
    log!("Looking for:");
    log!("  - assume(false), assume(), admit()");
    log!("  - external_body, external_fn_specification, external_trait_specification");
    log!("  - external_type_specification, external_trait_extension, external");
    log!("  - opaque");
    log!("");
    
    // Collect all Rust files from the specified paths
    let mut all_files: Vec<PathBuf> = Vec::new();
    let base_dir = args.base_dir();
    
    // Handle both file and directory modes
    for path in &args.paths {
        if path.is_file() && path.extension().map_or(false, |e| e == "rs") {
            all_files.push(path.clone());
        } else if path.is_dir() {
            all_files.extend(find_rust_files(&[path.clone()]));
        }
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
    
    let mut stats = FileStats::default();
    
    // Scan for verifier attributes using text scanning (more reliable than AST for Verus syntax)
    for line in content.lines() {
        let trimmed = line.trim();
        
        if trimmed.starts_with("#[verifier::") || trimmed.starts_with("#[verifier(") {
            if trimmed.contains("external_body") {
                stats.holes.external_body_count += 1;
                stats.holes.total_holes += 1;
            } else if trimmed.contains("external_fn_specification") {
                stats.holes.external_fn_spec_count += 1;
                stats.holes.total_holes += 1;
            } else if trimmed.contains("external_trait_specification") {
                stats.holes.external_trait_spec_count += 1;
                stats.holes.total_holes += 1;
            } else if trimmed.contains("external_type_specification") {
                stats.holes.external_type_spec_count += 1;
                stats.holes.total_holes += 1;
            } else if trimmed.contains("external_trait_extension") {
                stats.holes.external_trait_ext_count += 1;
                stats.holes.total_holes += 1;
            } else if trimmed.contains("external)") || trimmed.contains("external]") {
                stats.holes.external_count += 1;
                stats.holes.total_holes += 1;
            } else if trimmed.contains("opaque") {
                stats.holes.opaque_count += 1;
                stats.holes.total_holes += 1;
            }
        }
    }
    
    // Parse for verus! macros to find assume/admit calls and proof functions
    // (AST parsing still works for macro structure even if contents have parse errors)
    let parsed = ra_ap_syntax::SourceFile::parse(&content, ra_ap_syntax::Edition::Edition2021);
    let source_file = parsed.tree();
    let root = source_file.syntax();
    
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

fn analyze_verus_macro(tree: &SyntaxNode, _content: &str, stats: &mut FileStats) {
    // Walk the token tree looking for:
    // 1. Functions with proof modifier
    // 2. Function calls to assume/admit
    // Note: Attributes are already counted at the file level
    
    let tokens: Vec<_> = tree.descendants_with_tokens()
        .filter_map(|n| n.into_token())
        .collect();
    
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        
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

fn detect_verifier_attribute(tokens: &[ra_ap_syntax::SyntaxToken], start_idx: usize) -> Option<VerifierAttribute> {
    // Look for patterns:
    // #[verifier::external_body]
    // #[verifier(external_body)]
    // #[verifier::opaque]
    // #[verifier(opaque)]
    // etc.
    
    let mut i = start_idx;
    
    if i >= tokens.len() || tokens[i].kind() != SyntaxKind::POUND {
        return None;
    }
    i += 1;
    
    // Skip whitespace
    while i < tokens.len() && tokens[i].kind() == SyntaxKind::WHITESPACE {
        i += 1;
    }
    
    if i >= tokens.len() || tokens[i].kind() != SyntaxKind::L_BRACK {
        return None;
    }
    i += 1;
    
    // Skip whitespace
    while i < tokens.len() && tokens[i].kind() == SyntaxKind::WHITESPACE {
        i += 1;
    }
    
    // Look for "verifier"
    if i >= tokens.len() || tokens[i].kind() != SyntaxKind::IDENT || tokens[i].text() != "verifier" {
        return None;
    }
    i += 1;
    
    // Skip whitespace
    while i < tokens.len() && tokens[i].kind() == SyntaxKind::WHITESPACE {
        i += 1;
    }
    
    if i >= tokens.len() {
        return None;
    }
    
    // Check for :: (path) or ( (call syntax)
    let use_path_syntax = tokens[i].kind() == SyntaxKind::COLON2;
    let use_call_syntax = tokens[i].kind() == SyntaxKind::L_PAREN;
    
    if !use_path_syntax && !use_call_syntax {
        return None;
    }
    
    i += 1;
    
    // Skip whitespace
    while i < tokens.len() && tokens[i].kind() == SyntaxKind::WHITESPACE {
        i += 1;
    }
    
    // Get the attribute name
    if i >= tokens.len() || tokens[i].kind() != SyntaxKind::IDENT {
        return None;
    }
    
    let attr_name = tokens[i].text();
    
    match attr_name {
        "external_body" => Some(VerifierAttribute::ExternalBody),
        "external_fn_specification" => Some(VerifierAttribute::ExternalFnSpec),
        "external_trait_specification" => Some(VerifierAttribute::ExternalTraitSpec),
        "external_type_specification" => Some(VerifierAttribute::ExternalTypeSpec),
        "external_trait_extension" => Some(VerifierAttribute::ExternalTraitExt),
        "external" => Some(VerifierAttribute::External),
        "opaque" => Some(VerifierAttribute::Opaque),
        _ => None,
    }
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
        
        // Check for #[verifier::*] attributes
        if tokens[j].kind() == SyntaxKind::POUND {
            if detect_verifier_attribute(tokens, j).is_some() {
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
            log!("      {} × external_body", stats.holes.external_body_count);
        }
        if stats.holes.external_fn_spec_count > 0 {
            log!("      {} × external_fn_specification", stats.holes.external_fn_spec_count);
        }
        if stats.holes.external_trait_spec_count > 0 {
            log!("      {} × external_trait_specification", stats.holes.external_trait_spec_count);
        }
        if stats.holes.external_type_spec_count > 0 {
            log!("      {} × external_type_specification", stats.holes.external_type_spec_count);
        }
        if stats.holes.external_trait_ext_count > 0 {
            log!("      {} × external_trait_extension", stats.holes.external_trait_ext_count);
        }
        if stats.holes.external_count > 0 {
            log!("      {} × external", stats.holes.external_count);
        }
        if stats.holes.opaque_count > 0 {
            log!("      {} × opaque", stats.holes.opaque_count);
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
        summary.holes.external_fn_spec_count += stats.holes.external_fn_spec_count;
        summary.holes.external_trait_spec_count += stats.holes.external_trait_spec_count;
        summary.holes.external_type_spec_count += stats.holes.external_type_spec_count;
        summary.holes.external_trait_ext_count += stats.holes.external_trait_ext_count;
        summary.holes.external_count += stats.holes.external_count;
        summary.holes.opaque_count += stats.holes.opaque_count;
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
        log!("   {} × external_body", summary.holes.external_body_count);
    }
    if summary.holes.external_fn_spec_count > 0 {
        log!("   {} × external_fn_specification", summary.holes.external_fn_spec_count);
    }
    if summary.holes.external_trait_spec_count > 0 {
        log!("   {} × external_trait_specification", summary.holes.external_trait_spec_count);
    }
    if summary.holes.external_type_spec_count > 0 {
        log!("   {} × external_type_specification", summary.holes.external_type_spec_count);
    }
    if summary.holes.external_trait_ext_count > 0 {
        log!("   {} × external_trait_extension", summary.holes.external_trait_ext_count);
    }
    if summary.holes.external_count > 0 {
        log!("   {} × external", summary.holes.external_count);
    }
    if summary.holes.opaque_count > 0 {
        log!("   {} × opaque", summary.holes.opaque_count);
    }
    
    if summary.holes.total_holes == 0 {
        log!("");
        log!("🎉 No proof holes found! All proofs are complete.");
    }
}

