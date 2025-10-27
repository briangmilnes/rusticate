// Copyright (C) Brian G. Milnes 2025

//! Review: Stub Delegation Anti-Pattern
//! 
//! Detects inherent impl blocks that duplicate trait impl functionality.
//! 
//! Anti-pattern:
//! - Type has BOTH an inherent impl AND a trait impl
//! - Trait impl methods just delegate to inherent impl methods (or vice versa)
//! - One of the impls is redundant and should be removed
//! 
//! Example from BSTSetAVLMtEph:
//!   impl<T> BSTSetAVLMtEph<T> {
//!       pub fn size(&self) -> N { ... }  // real implementation
//!   }
//!   impl<T> BSTSetAVLMtEphTrait<T> for BSTSetAVLMtEph<T> {
//!       fn size(&self) -> N { self.size() }  // stub delegation!
//!   }
//! 
//! Binary: rusticate-review-stub-delegation

use anyhow::Result;
use ra_ap_syntax::{ast::{self, AstNode}, SyntaxKind, SourceFile, Edition};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use rusticate::{StandardArgs, find_rust_files, format_number, find_nodes, line_number};


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/review_stub_delegation.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
#[derive(Debug)]
struct ImplInfo {
    line: usize,
    methods: Vec<String>,
    method_bodies: HashMap<String, String>,  // method_name -> body
    method_has_self: HashMap<String, bool>,  // method_name -> has self param
    is_trait_impl: bool,
    trait_name: Option<String>,
}

#[derive(Debug)]
struct MethodAnalysis {
    name: String,
    pattern: DuplicationPattern,
    similarity: f64,
    is_method: bool,  // true if takes self, false if associated function
}

#[derive(Debug, PartialEq)]
enum DuplicationPattern {
    Identical,           // Exact match ignoring whitespace
    StubDelegation,      // One calls the other
    HighSimilarity(f64), // Levenshtein distance < threshold
    Different,           // Significantly different
}

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    type_name: String,
    inherent_line: usize,
    trait_line: usize,
    trait_name: String,
    method_analyses: Vec<MethodAnalysis>,
}

fn normalize_whitespace(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();
    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];
    
    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    for j in 0..=len2 {
        matrix[0][j] = j;
    }
    
    for (i, c1) in s1.chars().enumerate() {
        for (j, c2) in s2.chars().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            matrix[i + 1][j + 1] = std::cmp::min(
                std::cmp::min(
                    matrix[i][j + 1] + 1,     // deletion
                    matrix[i + 1][j] + 1,     // insertion
                ),
                matrix[i][j] + cost,          // substitution
            );
        }
    }
    
    matrix[len1][len2]
}

fn is_stub_call(body: &str, method_name: &str) -> bool {
    let normalized = normalize_whitespace(body);
    
    // Pattern 1: { self.method() } or { self.method(args) }
    let self_pattern = format!("{{self.{}(", method_name);
    if normalized.contains(&normalize_whitespace(&self_pattern)) {
        return true;
    }
    
    // Pattern 2: { Self::method() } or { Type::method() }
    let self_ufcs_pattern = format!("{{Self::{}(", method_name);
    if normalized.contains(&normalize_whitespace(&self_ufcs_pattern)) {
        return true;
    }
    
    false
}

fn analyze_method_pair(body1: &str, body2: &str, method_name: &str) -> (DuplicationPattern, f64) {
    let norm1 = normalize_whitespace(body1);
    let norm2 = normalize_whitespace(body2);
    
    // 1. Check for identical code (ignoring whitespace)
    if norm1 == norm2 {
        return (DuplicationPattern::Identical, 1.0);
    }
    
    // 2. Check for stub delegation (one just calls the other)
    if is_stub_call(body1, method_name) || is_stub_call(body2, method_name) {
        return (DuplicationPattern::StubDelegation, 0.9);
    }
    
    // 3. Levenshtein distance on normalized strings
    let distance = levenshtein_distance(&norm1, &norm2);
    let max_len = std::cmp::max(norm1.len(), norm2.len());
    
    if max_len == 0 {
        return (DuplicationPattern::Identical, 1.0);
    }
    
    let similarity = 1.0 - (distance as f64 / max_len as f64);
    
    if similarity > 0.8 {
        (DuplicationPattern::HighSimilarity(similarity), similarity)
    } else {
        (DuplicationPattern::Different, similarity)
    }
    
    // 4. TODO: AST-based Levenshtein (not yet implemented)
    // Would parse both bodies as AST and compare structural similarity
}

fn extract_type_name(self_ty: &ast::Type) -> String {
    let text = self_ty.syntax().text().to_string();
    // Extract base type name without generic parameters
    text.split('<').next().unwrap_or(&text).trim().to_string()
}

fn extract_method_names_and_bodies(impl_ast: &ast::Impl) -> (Vec<String>, HashMap<String, String>, HashMap<String, bool>) {
    let mut methods = Vec::new();
    let mut bodies = HashMap::new();
    let mut has_self = HashMap::new();
    
    if let Some(assoc_item_list) = impl_ast.assoc_item_list() {
        for item in assoc_item_list.assoc_items() {
            if let ast::AssocItem::Fn(func) = item {
                let syntax = func.syntax();
                
                // Extract function name
                let method_name = syntax.children()
                    .find(|n| n.kind() == SyntaxKind::NAME)
                    .and_then(|name_node| name_node.first_token())
                    .map(|t| t.text().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                
                // Extract method body
                let body = if let Some(body_node) = func.body() {
                    body_node.syntax().to_string()
                } else {
                    String::new()
                };
                
                // Check if it has self parameter
                let is_method = if let Some(param_list) = func.param_list() {
                    param_list.self_param().is_some()
                } else {
                    false
                };
                
                methods.push(method_name.clone());
                bodies.insert(method_name.clone(), body);
                has_self.insert(method_name, is_method);
            }
        }
    }
    
    (methods, bodies, has_self)
}

fn check_file(file_path: &Path, source: &str) -> Result<Vec<Violation>> {
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    
    if !parsed.errors().is_empty() {
        return Ok(Vec::new());
    }
    
    let tree = parsed.tree();
    let root = tree.syntax();
    
    // Find all impl blocks
    let impl_nodes = find_nodes(root, SyntaxKind::IMPL);
    
    // Group impls by type name
    let mut impls_by_type: HashMap<String, Vec<ImplInfo>> = HashMap::new();
    
    for impl_node in impl_nodes {
        if let Some(impl_ast) = ast::Impl::cast(impl_node.clone()) {
            let type_name = if let Some(self_ty) = impl_ast.self_ty() {
                extract_type_name(&self_ty)
            } else {
                continue;
            };
            
            let (methods, method_bodies, method_has_self) = extract_method_names_and_bodies(&impl_ast);
            
            if methods.is_empty() {
                continue;
            }
            
            let is_trait_impl = impl_ast.trait_().is_some();
            let trait_name = impl_ast.trait_().map(|t| t.syntax().text().to_string());
            
            let line = line_number(impl_ast.syntax(), source);
            
            let info = ImplInfo {
                line,
                methods,
                method_bodies,
                method_has_self,
                is_trait_impl,
                trait_name,
            };
            
            impls_by_type.entry(type_name).or_default().push(info);
        }
    }
    
    // Check for stub delegation pattern
    let mut violations = Vec::new();
    
    for (type_name, impls) in impls_by_type {
        // Need at least one inherent impl and one trait impl
        let inherent_impls: Vec<_> = impls.iter().filter(|i| !i.is_trait_impl).collect();
        let trait_impls: Vec<_> = impls.iter().filter(|i| i.is_trait_impl).collect();
        
        if inherent_impls.is_empty() || trait_impls.is_empty() {
            continue;
        }
        
        // Check for overlapping methods between inherent and trait impls
        for inherent in &inherent_impls {
            let inherent_methods: HashSet<_> = inherent.methods.iter().collect();
            
            for trait_impl in &trait_impls {
                let trait_methods: HashSet<_> = trait_impl.methods.iter().collect();
                
                // Find common methods
                let common: Vec<String> = inherent_methods
                    .intersection(&trait_methods)
                    .map(|s| s.to_string())
                    .collect();
                
                if !common.is_empty() {
                    // Analyze each common method
                    let mut method_analyses = Vec::new();
                    for method_name in &common {
                        if let (Some(inherent_body), Some(trait_body)) = (
                            inherent.method_bodies.get(method_name),
                            trait_impl.method_bodies.get(method_name)
                        ) {
                            let (pattern, similarity) = analyze_method_pair(
                                inherent_body,
                                trait_body,
                                method_name
                            );
                            
                            // Check if it's a method (has self) or associated function
                            let is_method = inherent.method_has_self.get(method_name).copied().unwrap_or(false);
                            
                            method_analyses.push(MethodAnalysis {
                                name: method_name.clone(),
                                pattern,
                                similarity,
                                is_method,
                            });
                        }
                    }
                    
                    violations.push(Violation {
                        file: file_path.to_path_buf(),
                        type_name: type_name.clone(),
                        inherent_line: inherent.line,
                        trait_line: trait_impl.line,
                        trait_name: trait_impl.trait_name.clone().unwrap_or_else(|| "Unknown".to_string()),
                        method_analyses,
                    });
                }
            }
        }
    }
    
    Ok(violations)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let args = StandardArgs::parse()?;
    let base_dir = args.base_dir();
    
    log!("Entering directory '{}'", base_dir.display());
    log!("");
    
    let files = find_rust_files(&args.paths);
    
    let mut all_violations = Vec::new();
    
    for file in &files {
        // Skip Types.rs
        if file.to_string_lossy().contains("Types.rs") {
            continue;
        }
        
        let source = match fs::read_to_string(file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        
        match check_file(file, &source) {
            Ok(violations) => {
                let rel_path = file.strip_prefix(&base_dir).unwrap_or(file);
                
                for mut v in violations {
                    v.file = rel_path.to_path_buf();
                    all_violations.push(v);
                }
            }
            Err(_) => continue,
        }
    }
    
    // Report findings in Emacs compile-mode format (all lines clickable)
    if !all_violations.is_empty() {
        for v in &all_violations {
            log!("{}:{}: {} - inherent impl with {} overlapping methods", 
                v.file.display(), v.inherent_line, v.type_name, v.method_analyses.len());
            log!("{}:{}: {} - trait impl {} with {} overlapping methods",
                v.file.display(), v.trait_line, v.type_name, v.trait_name, v.method_analyses.len());
            
            // Group methods by pattern
            let mut identical = Vec::new();
            let mut stubs = Vec::new();
            let mut similar = Vec::new();
            let mut different = Vec::new();
            
            for analysis in &v.method_analyses {
                let kind = if analysis.is_method { "m" } else { "f" };  // m=method, f=function
                match analysis.pattern {
                    DuplicationPattern::Identical => identical.push((&analysis.name, kind)),
                    DuplicationPattern::StubDelegation => stubs.push((&analysis.name, kind)),
                    DuplicationPattern::HighSimilarity(_) => similar.push((&analysis.name, kind, analysis.similarity)),
                    DuplicationPattern::Different => different.push((&analysis.name, kind, analysis.similarity)),
                }
            }
            
            if !identical.is_empty() {
                let names: Vec<String> = identical.iter()
                    .map(|(name, kind)| format!("{}:{}", name, kind))
                    .collect();
                log!("  IDENTICAL ({}): {}", identical.len(), names.join(", "));
            }
            if !stubs.is_empty() {
                let names: Vec<String> = stubs.iter()
                    .map(|(name, kind)| format!("{}:{}", name, kind))
                    .collect();
                log!("  STUB DELEGATION ({}): {}", stubs.len(), names.join(", "));
            }
            if !similar.is_empty() {
                let names: Vec<String> = similar.iter()
                    .map(|(name, kind, sim)| format!("{}:{} ({:.1}%)", name, kind, sim * 100.0))
                    .collect();
                log!("  HIGH SIMILARITY ({}): {}", similar.len(), names.join(", "));
            }
            if !different.is_empty() {
                let names: Vec<String> = different.iter()
                    .map(|(name, kind, sim)| format!("{}:{} ({:.1}%)", name, kind, sim * 100.0))
                    .collect();
                log!("  DIFFERENT ({}): {}", different.len(), names.join(", "));
            }
        }
        
        log!("");
        
        // Pareto analysis of duplication patterns
        let mut total_methods = 0;
        let mut total_functions = 0;
        let mut identical_count = 0;
        let mut stub_count = 0;
        let mut high_sim_count = 0;
        let mut different_count = 0;
        
        for v in &all_violations {
            for analysis in &v.method_analyses {
                if analysis.is_method {
                    total_methods += 1;
                } else {
                    total_functions += 1;
                }
                match analysis.pattern {
                    DuplicationPattern::Identical => identical_count += 1,
                    DuplicationPattern::StubDelegation => stub_count += 1,
                    DuplicationPattern::HighSimilarity(_) => high_sim_count += 1,
                    DuplicationPattern::Different => different_count += 1,
                }
            }
        }
        let total = total_methods + total_functions;
        
        log!("{}", "=".repeat(80));
        log!("PARETO ANALYSIS: DUPLICATION PATTERNS");
        log!("{}", "=".repeat(80));
        
        let mut patterns = vec![
            ("IDENTICAL (exact duplication)", identical_count),
            ("STUB DELEGATION (one calls other)", stub_count),
            ("HIGH SIMILARITY (>80%)", high_sim_count),
            ("DIFFERENT (<80% similar)", different_count),
        ];
        patterns.sort_by(|a, b| b.1.cmp(&a.1));
        
        let mut cumulative = 0;
        for (pattern, count) in &patterns {
            cumulative += count;
            let percentage = (*count as f64 / total as f64) * 100.0;
            let cumulative_pct = (cumulative as f64 / total as f64) * 100.0;
            log!("{:6} ({:5.1}%, cumulative {:5.1}%): {}",
                format_number(*count), percentage, cumulative_pct, pattern);
        }
        log!("{}", "-".repeat(80));
        log!("TOTAL OVERLAPPING: {} ({} methods :m, {} functions :f)", 
            format_number(total), format_number(total_methods), format_number(total_functions));
        log!("");
        
        log!("✗ Found {} stub delegation violations in {} file(s)",
            format_number(all_violations.len()),
            format_number(files.len()));
        log!("Completed in {}ms", start.elapsed().as_millis());
        std::process::exit(1);
    } else {
        log!("✓ No stub delegation found in {} file(s)", format_number(files.len()));
        log!("Completed in {}ms", start.elapsed().as_millis());
        Ok(())
    }
}

