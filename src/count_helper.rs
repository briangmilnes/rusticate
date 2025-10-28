// Copyright (C) Brian G. Milnes 2025

//! Helper infrastructure for count tools
//! 
//! Provides consistent structure for all count-* binaries:
//! - Categorization of files (src/tests/benches)
//! - Per-file counting
//! - Aggregation by section
//! - Detailed output with summary

pub mod count_helper {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use anyhow::Result;
    use crate::args::args::format_number;

    /// Run a count tool with standard structure
    /// 
    /// Takes a counting function that counts occurrences in a single file.
    /// Handles categorization, aggregation, and output formatting.
    /// 
    /// # Arguments
    /// * `paths` - Paths to analyze
    /// * `base_dir` - Base directory for relative paths
    /// * `count_fn` - Function to count occurrences in a file
    /// * `item_name` - Name of item being counted (e.g., "'as' expressions")
    /// 
    /// # Returns
    /// Summary string for final output
    pub fn run_count<F>(
        paths: &[PathBuf],
        base_dir: &Path,
        count_fn: F,
        item_name: &str,
    ) -> Result<String>
    where
        F: Fn(&Path) -> Result<usize>,
    {
        use crate::find_rust_files;
        
        // Categorize paths
        let mut src_dirs = Vec::new();
        let mut tests_dirs = Vec::new();
        let mut benches_dirs = Vec::new();
        let mut other_dirs = Vec::new();
        
        for path in paths {
            let path_str = path.to_string_lossy();
            if path_str.contains("/src/") || path_str.ends_with("/src") {
                src_dirs.push(path.clone());
            } else if path_str.contains("/tests/") || path_str.ends_with("/tests") {
                tests_dirs.push(path.clone());
            } else if path_str.contains("/benches/") || path_str.ends_with("/benches") {
                benches_dirs.push(path.clone());
            } else {
                other_dirs.push(path.clone());
            }
        }
        
        let mut section_counts = BTreeMap::new();
        let mut section_totals: BTreeMap<&str, usize> = BTreeMap::new();
        
        // Process each category
        let categories = [
            ("src", &src_dirs),
            ("tests", &tests_dirs),
            ("benches", &benches_dirs),
        ];
        
        for (name, dirs) in &categories {
            if dirs.is_empty() {
                continue;
            }
            
            let files = find_rust_files(dirs);
            let mut file_counts = Vec::new();
            
            for file in files {
                match count_fn(&file) {
                    Ok(count) => {
                        if let Ok(rel_path) = file.strip_prefix(base_dir) {
                            file_counts.push((rel_path.display().to_string(), count));
                        } else {
                            file_counts.push((file.display().to_string(), count));
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to parse {}: {}", file.display(), e);
                    }
                }
            }
            
            if !file_counts.is_empty() {
                let total: usize = file_counts.iter().map(|(_, c)| c).sum();
                section_totals.insert(name, total);
                section_counts.insert(*name, file_counts);
            }
        }
        
        // Process other files/dirs
        if !other_dirs.is_empty() {
            let files: Vec<_> = other_dirs.iter()
                .filter(|p| p.is_file())
                .cloned()
                .collect();
            
            if !files.is_empty() {
                let mut file_counts = Vec::new();
                for file in files {
                    match count_fn(&file) {
                        Ok(count) => {
                            if let Ok(rel_path) = file.strip_prefix(base_dir) {
                                file_counts.push((rel_path.display().to_string(), count));
                            } else {
                                file_counts.push((file.display().to_string(), count));
                            }
                        }
                        Err(e) => {
                            eprintln!("Warning: Failed to parse {}: {}", file.display(), e);
                        }
                    }
                }
                
                if !file_counts.is_empty() {
                    let total: usize = file_counts.iter().map(|(_, c)| c).sum();
                    section_totals.insert("other", total);
                    section_counts.insert("other", file_counts);
                }
            }
        }
        
        // Print detailed output
        print_detailed_counts(&section_counts);
        
        // Build summary
        let summary = build_summary(&section_totals, item_name);
        Ok(summary)
    }

    /// Print detailed per-file counts
    fn print_detailed_counts(section_counts: &BTreeMap<&str, Vec<(String, usize)>>) {
        let section_order = ["src", "tests", "benches", "other"];
        
        for section in &section_order {
            if let Some(files) = section_counts.get(section) {
                println!("{section}:");
                for (file, count) in files {
                    println!("  {}: {}", file, format_number(*count));
                }
                println!();
            }
        }
    }

    /// Build summary line with proper units
    fn build_summary(section_totals: &BTreeMap<&str, usize>, item_name: &str) -> String {
        let mut summary_parts = Vec::new();
        
        let section_order = ["src", "tests", "benches", "other"];
        for section in &section_order {
            if let Some(&count) = section_totals.get(section) {
                summary_parts.push(format!("{} {}", section, format_number(count)));
            }
        }
        
        if !summary_parts.is_empty() {
            let total: usize = section_totals.values().sum();
            summary_parts.push(format!("total {}", format_number(total)));
        }
        
        format!("Summary: {} {}", summary_parts.join(", "), item_name)
    }
}

