//! Validate and inspect rusticate-analyze-modules-mir JSON output
//!
//! Usage:
//!   rusticate-validate-mir-json [json_path] [schema_path]
//!
//! Defaults to analyses/rusticate-analyze-modules-mir.json and the corresponding schema.

use std::fs;
use serde_json::Value;
use anyhow::{Context, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    let json_path = args.get(1)
        .map(|s| s.as_str())
        .unwrap_or("analyses/rusticate-analyze-modules-mir.json");
    let schema_path = args.get(2)
        .map(|s| s.as_str())
        .unwrap_or("schemas/rusticate-analyze-modules-mir.schema.json");
    
    println!("rusticate-validate-mir-json");
    println!("===========================");
    println!("JSON:   {}", json_path);
    println!("Schema: {}", schema_path);
    println!();
    
    // Load schema
    let schema_str = fs::read_to_string(schema_path)
        .with_context(|| format!("Failed to read schema: {}", schema_path))?;
    let schema: Value = serde_json::from_str(&schema_str)
        .context("Failed to parse schema JSON")?;
    
    // Load data  
    let data_str = fs::read_to_string(json_path)
        .with_context(|| format!("Failed to read JSON: {}", json_path))?;
    let data: Value = serde_json::from_str(&data_str)
        .context("Failed to parse data JSON")?;
    
    // Validate using jsonschema crate
    let validator = jsonschema::validator_for(&schema)
        .context("Failed to compile schema")?;
    
    if validator.is_valid(&data) {
        println!("✓ JSON validates against schema!\n");
    } else {
        println!("✗ Validation errors:");
        let result = validator.iter_errors(&data);
        for error in result {
            println!("  - {}", error);
        }
        println!();
    }
    
    // Print summary
    println!("=== Summary ===");
    let summary = &data["summary"];
    println!("Total projects:      {:>6}", summary["total_projects"]);
    println!("Total crates:        {:>6}", summary["total_crates"]);
    println!("Crates with stdlib:  {:>6}", summary["crates_with_stdlib"]);
    println!("Unique modules:      {:>6}", summary["unique_modules"]);
    println!("Unique types:        {:>6}", summary["unique_types"]);
    println!("Unique traits:       {:>6}", summary["unique_traits"]);
    println!("Unique methods:      {:>6}", summary["unique_methods"]);
    
    println!("\n=== Full Support Coverage Requirements ===");
    println!("{:>5}  {:>8}  {:>8}  {:>8}  {:>8}", "Pct", "Modules", "Types", "Traits", "Methods");
    println!("{}", "-".repeat(50));
    for pct in ["70", "80", "90", "100"] {
        let key = format!("coverage_to_support_{}_pct", pct);
        let cov = &summary[&key];
        println!("{:>4}%  {:>8}  {:>8}  {:>8}  {:>8}",
            pct,
            cov["modules"], cov["types"], cov["traits"], cov["methods"]);
    }
    
    // Show greedy cover details for each category
    for (category, name) in [("modules", "Modules"), ("types", "Types"), ("traits", "Traits"), ("methods", "Methods")] {
        println!("\n=== Greedy Full Support: {} ===", name);
        
        for pct in ["70", "80", "90", "100"] {
            let milestone = &data["analysis"]["greedy_cover"][category]["full_support"]["milestones"][pct];
            
            if milestone.is_null() { continue; }
            
            let target = milestone["target_crates"].as_u64().unwrap_or(0);
            let actual = milestone["actual_coverage"].as_f64().unwrap_or(0.0);
            let items = milestone["items"].as_array();
            
            println!("\n--- {}% (target: {} crates, actual: {:.2}%) ---", pct, target, actual);
            
            if let Some(items) = items {
                println!("Items needed: {}", items.len());
                let show_count = if pct == "70" { 15 } else { 5 };
                for item in items.iter().take(show_count) {
                    println!("  {:4}. {:45} +{:>5} ({:>7.4}%)",
                        item["rank"],
                        truncate(item["name"].as_str().unwrap_or(""), 45),
                        item["crates_added"],
                        item["cumulative_coverage"].as_f64().unwrap_or(0.0));
                }
                if items.len() > show_count {
                    println!("  ... {} more", items.len() - show_count);
                }
            }
        }
    }
    
    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len-3])
    }
}

