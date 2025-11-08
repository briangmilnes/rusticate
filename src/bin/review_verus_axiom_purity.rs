use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SyntaxNode};
use rusticate::{StandardArgs, find_rust_files};
use std::{collections::HashMap, fs, path::{Path, PathBuf}, time::Instant};

macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        
        let log_path = "analyses/rusticate-review-verus-axiom-purity.log";
        if let Some(parent) = std::path::Path::new(log_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(log_path) {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

#[derive(Debug, Clone, PartialEq)]
enum AxiomPurity {
    NumericMath,        // Numbers only: Nat, Int, arithmetic
    SetTheoreticMath,   // Mathematical abstractions: seq, multiset, map, set
    MachineMath,        // Concrete data structures: hash, vec, array, ptr, etc.
}

#[derive(Debug, Default, Clone)]
struct PurityStats {
    numeric_math_count: usize,
    set_theoretic_math_count: usize,
    machine_math_count: usize,
    numeric_math_names: Vec<String>,
    set_theoretic_math_names: Vec<String>,
    machine_math_names: Vec<String>,
}

#[derive(Debug, Default)]
struct FileStats {
    axioms: PurityStats,
}

#[derive(Debug, Default)]
struct SummaryStats {
    total_files: usize,
    numeric_math_total: usize,
    set_theoretic_math_total: usize,
    machine_math_total: usize,
}

fn main() -> Result<()> {
    let start_time = Instant::now();
    
    let args = StandardArgs::parse()?;
    
    if args.language != "Verus" {
        anyhow::bail!("This tool requires -l Verus flag");
    }
    
    log!("Verus Axiom Purity Analysis");
    log!("");
    
    // Collect all Rust files from the specified paths
    let mut all_files: Vec<PathBuf> = Vec::new();
    let base_dir = args.base_dir();
    
    for path in &args.paths {
        let full_path = base_dir.join(path);
        if full_path.is_file() && full_path.extension().map_or(false, |ext| ext == "rs") {
            all_files.push(full_path);
        } else if full_path.is_dir() {
            all_files.extend(find_rust_files(&[full_path]));
        }
    }
    
    all_files.sort();
    
    // Analyze each file
    let mut file_stats_map: HashMap<String, FileStats> = HashMap::new();
    
    for file_path in &all_files {
        match analyze_file(file_path) {
            Ok(stats) => {
                let rel_path = file_path.strip_prefix(&base_dir)
                    .unwrap_or(file_path)
                    .display()
                    .to_string();
                print_file_report(&rel_path, &stats);
                file_stats_map.insert(rel_path, stats);
            }
            Err(e) => {
                eprintln!("Error analyzing {}: {}", file_path.display(), e);
            }
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
    
    let parsed = ra_ap_syntax::SourceFile::parse(&content, ra_ap_syntax::Edition::Edition2021);
    let source_file = parsed.tree();
    let root = source_file.syntax();
    
    let mut found_verus_macro = false;
    
    // Scan for axioms in verus! and verus_! macros
    for node in root.descendants() {
        if node.kind() == SyntaxKind::MACRO_CALL {
            if let Some(macro_call) = ast::MacroCall::cast(node.clone()) {
                if let Some(macro_path) = macro_call.path() {
                    let path_str = macro_path.to_string();
                    if path_str == "verus" || path_str == "verus_" {
                        if let Some(token_tree) = macro_call.token_tree() {
                            found_verus_macro = true;
                            analyze_verus_macro(token_tree.syntax(), &mut stats);
                        }
                    }
                }
            }
        }
    }
    
    // If no verus! macro found, scan at file level
    if !found_verus_macro {
        analyze_at_file_level(&root, &mut stats);
    }
    
    Ok(stats)
}

fn analyze_verus_macro(tree: &SyntaxNode, stats: &mut FileStats) {
    let tokens: Vec<_> = tree.descendants_with_tokens()
        .filter_map(|n| n.into_token())
        .collect();
    
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        
        // Look for "axiom fn" declarations
        if token.kind() == SyntaxKind::FN_KW {
            if let Some(axiom_name) = get_axiom_fn_name(&tokens, i) {
                let purity = classify_axiom(&axiom_name);
                add_axiom(stats, axiom_name, purity);
            }
        }
        
        // Look for "broadcast use" statements with axioms
        if token.kind() == SyntaxKind::IDENT && token.text() == "broadcast" {
            if i + 1 < tokens.len() {
                let mut j = i + 1;
                while j < tokens.len() && tokens[j].kind() == SyntaxKind::WHITESPACE {
                    j += 1;
                }
                if j < tokens.len() && tokens[j].kind() == SyntaxKind::USE_KW {
                    if let Some(axiom_names) = extract_broadcast_use_axioms(&tokens, j) {
                        for axiom_name in axiom_names {
                            let purity = classify_axiom(&axiom_name);
                            add_axiom(stats, axiom_name, purity);
                        }
                    }
                }
            }
        }
        
        i += 1;
    }
}

fn analyze_at_file_level(root: &SyntaxNode, stats: &mut FileStats) {
    let tokens: Vec<_> = root.descendants_with_tokens()
        .filter_map(|n| n.into_token())
        .collect();
    
    // Same logic as in verus macro
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        
        if token.kind() == SyntaxKind::FN_KW {
            if let Some(axiom_name) = get_axiom_fn_name(&tokens, i) {
                let purity = classify_axiom(&axiom_name);
                add_axiom(stats, axiom_name, purity);
            }
        }
        
        if token.kind() == SyntaxKind::IDENT && token.text() == "broadcast" {
            if i + 1 < tokens.len() {
                let mut j = i + 1;
                while j < tokens.len() && tokens[j].kind() == SyntaxKind::WHITESPACE {
                    j += 1;
                }
                if j < tokens.len() && tokens[j].kind() == SyntaxKind::USE_KW {
                    if let Some(axiom_names) = extract_broadcast_use_axioms(&tokens, j) {
                        for axiom_name in axiom_names {
                            let purity = classify_axiom(&axiom_name);
                            add_axiom(stats, axiom_name, purity);
                        }
                    }
                }
            }
        }
        
        i += 1;
    }
}

fn get_axiom_fn_name(tokens: &[ra_ap_syntax::SyntaxToken], fn_idx: usize) -> Option<String> {
    // Look backwards for "axiom" modifier
    let start_idx = if fn_idx >= 10 { fn_idx - 10 } else { 0 };
    let mut is_axiom = false;
    
    for j in start_idx..fn_idx {
        if tokens[j].kind() == SyntaxKind::IDENT && tokens[j].text() == "axiom" {
            is_axiom = true;
            break;
        }
    }
    
    if !is_axiom {
        return None;
    }
    
    // Look forward for function name (next IDENT after fn)
    let mut i = fn_idx + 1;
    while i < tokens.len() && tokens[i].kind() == SyntaxKind::WHITESPACE {
        i += 1;
    }
    
    if i < tokens.len() && tokens[i].kind() == SyntaxKind::IDENT {
        Some(tokens[i].text().to_string())
    } else {
        None
    }
}

fn extract_broadcast_use_axioms(tokens: &[ra_ap_syntax::SyntaxToken], use_idx: usize) -> Option<Vec<String>> {
    let mut axiom_names = Vec::new();
    let mut i = use_idx + 1;
    
    // Scan until semicolon, collecting IDENTs that contain "axiom"
    while i < tokens.len() {
        let token = &tokens[i];
        
        if token.kind() == SyntaxKind::SEMICOLON {
            break;
        }
        
        if token.kind() == SyntaxKind::IDENT {
            let text = token.text();
            if text.contains("axiom") {
                axiom_names.push(text.to_string());
            }
        }
        
        i += 1;
    }
    
    if axiom_names.is_empty() {
        None
    } else {
        Some(axiom_names)
    }
}

fn classify_axiom(name: &str) -> AxiomPurity {
    let name_lower = name.to_lowercase();
    
    // Numeric math patterns - numbers only (Nat, Int, arithmetic)
    if name_lower.contains("arithmetic") || name_lower.contains("div_mod") ||
       name_lower.contains("div") || name_lower.contains("mul") || 
       name_lower.contains("power") || name_lower.contains("logarithm") || 
       name_lower.contains("mod_internals") || name_lower.contains("div_internals") ||
       name_lower.contains("nat") || name_lower.contains("int") ||
       name_lower.contains("add") || name_lower.contains("sub") {
        return AxiomPurity::NumericMath;
    }
    
    // Set theoretic math patterns - mathematical abstractions
    if name_lower.contains("seq") || name_lower.contains("multiset") || 
       name_lower.contains("map") || name_lower.contains("set") ||
       name_lower.contains("to_multiset") {
        return AxiomPurity::SetTheoreticMath;
    }
    
    // Everything else is machine math (hash, vec, array, ptr, thread, etc.)
    AxiomPurity::MachineMath
}

fn add_axiom(stats: &mut FileStats, name: String, purity: AxiomPurity) {
    match purity {
        AxiomPurity::NumericMath => {
            stats.axioms.numeric_math_count += 1;
            stats.axioms.numeric_math_names.push(name);
        }
        AxiomPurity::SetTheoreticMath => {
            stats.axioms.set_theoretic_math_count += 1;
            stats.axioms.set_theoretic_math_names.push(name);
        }
        AxiomPurity::MachineMath => {
            stats.axioms.machine_math_count += 1;
            stats.axioms.machine_math_names.push(name);
        }
    }
}

fn print_file_report(path: &str, stats: &FileStats) {
    let has_machine = stats.axioms.machine_math_count > 0;
    let has_any = stats.axioms.numeric_math_count > 0 || 
                  stats.axioms.set_theoretic_math_count > 0 || 
                  stats.axioms.machine_math_count > 0;
    
    if !has_any {
        return; // Skip files with no axioms
    }
    
    if has_machine {
        log!("⚠ {}", path);
    } else {
        log!("✓ {}", path);
    }
    
    if stats.axioms.numeric_math_count > 0 {
        log!("   Numeric math axioms: {}", stats.axioms.numeric_math_count);
        let mut sorted_names = stats.axioms.numeric_math_names.clone();
        sorted_names.sort();
        let name_counts = count_names(&sorted_names);
        for (name, count) in name_counts {
            log!("      {} × {}", count, name);
        }
    }
    
    if stats.axioms.set_theoretic_math_count > 0 {
        log!("   Set theoretic math axioms: {}", stats.axioms.set_theoretic_math_count);
        let mut sorted_names = stats.axioms.set_theoretic_math_names.clone();
        sorted_names.sort();
        let name_counts = count_names(&sorted_names);
        for (name, count) in name_counts {
            log!("      {} × {}", count, name);
        }
    }
    
    if stats.axioms.machine_math_count > 0 {
        log!("   Machine math axioms: {}", stats.axioms.machine_math_count);
        let mut sorted_names = stats.axioms.machine_math_names.clone();
        sorted_names.sort();
        let name_counts = count_names(&sorted_names);
        for (name, count) in name_counts {
            log!("      {} × {}", count, name);
        }
    }
}

fn count_names(names: &[String]) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for name in names {
        *counts.entry(name.clone()).or_insert(0) += 1;
    }
    let mut result: Vec<_> = counts.into_iter().collect();
    result.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0))); // Sort by count desc, then name asc
    result
}

fn compute_summary(file_stats_map: &HashMap<String, FileStats>) -> SummaryStats {
    let mut summary = SummaryStats::default();
    
    for stats in file_stats_map.values() {
        summary.total_files += 1;
        summary.numeric_math_total += stats.axioms.numeric_math_count;
        summary.set_theoretic_math_total += stats.axioms.set_theoretic_math_count;
        summary.machine_math_total += stats.axioms.machine_math_count;
    }
    
    summary
}

fn print_summary(summary: &SummaryStats) {
    log!("");
    log!("═══════════════════════════════════════════════════════════════");
    log!("SUMMARY");
    log!("═══════════════════════════════════════════════════════════════");
    log!("");
    
    let total = summary.numeric_math_total + summary.set_theoretic_math_total + summary.machine_math_total;
    if total == 0 {
        log!("No axioms found.");
        return;
    }
    
    let numeric_pct = (summary.numeric_math_total as f64 / total as f64) * 100.0;
    let set_theoretic_pct = (summary.set_theoretic_math_total as f64 / total as f64) * 100.0;
    let machine_pct = (summary.machine_math_total as f64 / total as f64) * 100.0;
    
    log!("Axiom Classification:");
    log!("   {} numeric math ({:.1}%)", summary.numeric_math_total, numeric_pct);
    log!("   {} set theoretic math ({:.1}%)", summary.set_theoretic_math_total, set_theoretic_pct);
    log!("   {} machine math ({:.1}%)", summary.machine_math_total, machine_pct);
    log!("   ─────────────────────");
    log!("   {} total axioms", total);
}

