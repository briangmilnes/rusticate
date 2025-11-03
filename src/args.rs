// Copyright (C) Brian G. Milnes 2025

//! Standard argument parsing for all Rusticate tools
//!
//! Provides consistent argument handling across all binaries

pub mod args {
    use anyhow::Result;
    use std::path::PathBuf;

    /// Standard arguments common to all Rusticate tools
    pub struct StandardArgs {
        /// Directories or files to analyze
        pub paths: Vec<PathBuf>,
        /// Whether any path is a module search result
        pub is_module_search: bool,
        /// Project-specific features to enable (e.g., "apas")
        pub project: Option<String>,
        /// Language variant (e.g., "Rust", "Verus")
        pub language: String,
        /// Repository scan mode - find all Cargo projects recursively
        pub repositories: Option<PathBuf>,
    }

    impl StandardArgs {
        /// Parse standard arguments from command line
        /// 
        /// Usage: tool [OPTIONS]
        /// 
        /// Options:
        /// - --codebase          Analyze src/, tests/, benches/ (default)
        /// - --dir DIR...        Analyze specific directories
        /// - --file FILE         Analyze a single file
        /// - --module NAME       Find module in src/ and its tests/benches
        /// - No args             Same as --codebase
        pub fn parse() -> Result<Self> {
            let args: Vec<String> = std::env::args().collect();
            
            // Check for help flag
            if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
                Self::print_usage(&args[0]);
                std::process::exit(0);
            }
            
            if args.len() == 1 {
                // No arguments - default to codebase (src/, tests/, benches/)
                let current_dir = std::env::current_dir()?;
                return Ok(StandardArgs { 
                    paths: vec![current_dir],
                    is_module_search: false,
                    project: None,
                    language: "Rust".to_string(),
                    repositories: None,
                });
            }
            
            let mut i = 1;
            let mut paths = Vec::new();
            let mut is_module_search = false;
            let mut project = None;
            let mut language = "Rust".to_string();
            let mut repositories = None;
            
            while i < args.len() {
                match args[i].as_str() {
                    "--codebase" | "--code-base" | "-c" => {
                        // Use current directory (will expand to src/, tests/, benches/)
                        let current_dir = std::env::current_dir()?;
                        paths.push(current_dir);
                        i += 1;
                    }
                    "--dir" | "-d" => {
                        // Collect all following non-flag arguments as directories
                        i += 1;
                        while i < args.len() && !args[i].starts_with('-') {
                            let current_dir = std::env::current_dir()?;
                            let dir_path = if args[i] == "." {
                                current_dir
                            } else if args[i].contains('/') || args[i].contains('\\') {
                                PathBuf::from(&args[i])
                            } else {
                                current_dir.join(&args[i])
                            };
                            
                            if !dir_path.exists() {
                                return Err(anyhow::anyhow!("Directory not found: {}", dir_path.display()));
                            }
                            if !dir_path.is_dir() {
                                return Err(anyhow::anyhow!("Not a directory: {}", dir_path.display()));
                            }
                            
                            paths.push(dir_path);
                            i += 1;
                        }
                    }
                    "--file" | "-f" => {
                        i += 1;
                        if i >= args.len() {
                            return Err(anyhow::anyhow!("--file requires a file path"));
                        }
                        let file_path = PathBuf::from(&args[i]);
                        if !file_path.exists() {
                            return Err(anyhow::anyhow!("File not found: {}", file_path.display()));
                        }
                        if !file_path.is_file() {
                            return Err(anyhow::anyhow!("Not a file: {}", file_path.display()));
                        }
                        paths.push(file_path);
                        i += 1;
                    }
                    "--module" | "-m" => {
                        i += 1;
                        if i >= args.len() {
                            return Err(anyhow::anyhow!("--module requires a module name"));
                        }
                        let module_result = Self::find_module(&args[i])?;
                        paths.extend(module_result.paths);
                        is_module_search = true;
                        i += 1;
                    }
                    "--project" | "-p" => {
                        i += 1;
                        if i >= args.len() {
                            return Err(anyhow::anyhow!("--project requires a project name"));
                        }
                        project = Some(args[i].clone());
                        i += 1;
                    }
                    "--language" | "-l" => {
                        i += 1;
                        if i >= args.len() {
                            return Err(anyhow::anyhow!("--language requires a language name"));
                        }
                        language = args[i].clone();
                        i += 1;
                    }
                    "--repositories" | "-r" => {
                        i += 1;
                        if i >= args.len() {
                            return Err(anyhow::anyhow!("--repositories requires a directory path"));
                        }
                        let repo_path = PathBuf::from(&args[i]);
                        if !repo_path.exists() {
                            return Err(anyhow::anyhow!("Repository directory not found: {}", repo_path.display()));
                        }
                        if !repo_path.is_dir() {
                            return Err(anyhow::anyhow!("Not a directory: {}", repo_path.display()));
                        }
                        repositories = Some(repo_path);
                        i += 1;
                    }
                    "--help" | "-h" => {
                        Self::print_usage(&args[0]);
                        std::process::exit(0);
                    }
                    "--dry-run" => {
                        // Tool-specific flag, ignore here (handled by individual tools)
                        i += 1;
                    }
                    other => {
                        return Err(anyhow::anyhow!("Unknown option: {other}"));
                    }
                }
            }
            
            // Validate that we have either paths or repositories, but not both
            if repositories.is_some() && !paths.is_empty() {
                return Err(anyhow::anyhow!("Cannot use --repositories with other path options"));
            }
            
            if repositories.is_none() && paths.is_empty() {
                return Err(anyhow::anyhow!("No paths specified"));
            }
            
            Ok(StandardArgs { paths, is_module_search, project, language, repositories })
        }
        
        /// Find a module by name in src/, and its corresponding test and bench files
        /// 
        /// Searches for:
        /// - src/**/ModuleName.rs (the source file)
        /// - tests/**/test_ModuleName.rs or tests/**/*ModuleName.rs (test file)
        /// - benches/**/bench_ModuleName.rs or benches/**/*ModuleName.rs (bench file)
        fn find_module(module_name: &str) -> Result<Self> {
            let current_dir = std::env::current_dir()?;
            let mut found_paths = Vec::new();
            
            // 1. Find the source file in src/
            let src_dir = current_dir.join("src");
            let module_file = format!("{module_name}.rs");
            let mut src_files = Vec::new();
            
            if src_dir.exists() {
                Self::search_for_file(&src_dir, &module_file, &mut src_files)?;
            }
            
            if src_files.is_empty() {
                return Err(anyhow::anyhow!(
                    "Module '{module_name}' not found in src/"
                ));
            }
            
            // Use the first source file found
            found_paths.push(src_files[0].clone());
            eprintln!("Found source: {}", src_files[0].display());
            
            // 2. Look for test file in tests/
            let tests_dir = current_dir.join("tests");
            if tests_dir.exists() {
                let mut test_files = Vec::new();
                
                // Try multiple naming patterns:
                // - test_{module}.rs (lowercase test_)
                // - Test{Module}.rs (capital Test)
                // - {Module}.rs (no prefix)
                let test_patterns = vec![
                    format!("test_{}.rs", module_name),
                    format!("Test{}.rs", module_name),
                    module_file.clone(),
                ];
                
                for pattern in test_patterns {
                    Self::search_for_file(&tests_dir, &pattern, &mut test_files)?;
                    if !test_files.is_empty() {
                        break;
                    }
                }
                
                if !test_files.is_empty() {
                    found_paths.push(test_files[0].clone());
                    eprintln!("Found test: {}", test_files[0].display());
                }
            }
            
            // 3. Look for bench file in benches/
            let benches_dir = current_dir.join("benches");
            if benches_dir.exists() {
                let mut bench_files = Vec::new();
                
                // Try multiple naming patterns:
                // - bench_{module}.rs (lowercase bench_)
                // - Bench{Module}.rs (capital Bench)
                // - {Module}.rs (no prefix)
                let bench_patterns = vec![
                    format!("bench_{}.rs", module_name),
                    format!("Bench{}.rs", module_name),
                    module_file.clone(),
                ];
                
                for pattern in bench_patterns {
                    Self::search_for_file(&benches_dir, &pattern, &mut bench_files)?;
                    if !bench_files.is_empty() {
                        break;
                    }
                }
                
                if !bench_files.is_empty() {
                    found_paths.push(bench_files[0].clone());
                    eprintln!("Found bench: {}", bench_files[0].display());
                }
            }
            
            Ok(StandardArgs { 
                paths: found_paths,
                is_module_search: true,
                project: None,
                language: "Rust".to_string(),
                repositories: None,
            })
        }
        
        /// Recursively search for a file
        fn search_for_file(dir: &PathBuf, filename: &str, results: &mut Vec<PathBuf>) -> Result<()> {
            // Skip directories that should be excluded
            if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str()) {
                if dir_name == "attic" || dir_name == "target" || dir_name.starts_with('.') {
                    return Ok(());
                }
            }
            
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.file_name().is_some_and(|n| n == filename) {
                        results.push(path);
                    } else if path.is_dir() {
                        Self::search_for_file(&path, filename, results)?;
                    }
                }
            }
            Ok(())
        }
        
        /// Print usage information
        fn print_usage(program_name: &str) {
            let name = std::path::Path::new(program_name)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(program_name);
            
            println!("Usage: {name} [OPTIONS]");
            println!();
            println!("Options:");
            println!("  -c, --codebase             Analyze src/, tests/, benches/ (default)");
            println!("  -d, --dir DIR [DIR...]     Analyze specific directories");
            println!("  -f, --file FILE            Analyze a single file");
            println!("  -m, --module NAME          Find module in src/ and its tests/benches");
            println!("  -r, --repositories DIR     Scan for all Cargo projects under DIR");
            println!("  -p, --project NAME         Enable project-specific tools (e.g., 'APAS')");
            println!("  -l, --language NAME        Language variant: 'Rust' (default) or 'Verus'");
            println!("  -h, --help                 Show this help message");
            println!();
            println!("Examples:");
            println!("  {name}                           # Analyze codebase (src/, tests/, benches/)");
            println!("  {name} -c                        # Same as above");
            println!("  {name} -d src tests benches      # Analyze multiple directories");
            println!("  {name} -d src                    # Analyze just src/");
            println!("  {name} -f src/lib.rs             # Analyze single file");
            println!("  {name} -m ArraySeqStEph          # Analyze module + tests + benches");
            println!("  {name} -r ~/projects/repos       # Analyze all Cargo projects in directory");
        }

        /// Get all paths
        pub fn paths(&self) -> &[PathBuf] {
            &self.paths
        }

        /// Get the base directory path (first path, or its parent if it's a file)
        pub fn base_dir(&self) -> PathBuf {
            if self.paths.is_empty() {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            } else if self.paths[0].is_file() {
                self.paths[0].parent().unwrap_or(&self.paths[0]).to_path_buf()
            } else {
                self.paths[0].clone()
            }
        }
        
        /// Find all Cargo projects recursively under a directory
        /// 
        /// Returns a list of project root directories (directories containing Cargo.toml)
        pub fn find_cargo_projects(dir: &PathBuf) -> Vec<PathBuf> {
            let mut projects = Vec::new();
            Self::find_cargo_projects_recursive(dir, &mut projects);
            projects.sort();
            projects
        }
        
        fn find_cargo_projects_recursive(dir: &PathBuf, projects: &mut Vec<PathBuf>) {
            // Skip excluded directories
            if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str()) {
                if dir_name == "target" || dir_name == "attic" || dir_name.starts_with('.') {
                    return;
                }
            }
            
            // Check if this directory has a Cargo.toml
            let cargo_toml = dir.join("Cargo.toml");
            if cargo_toml.exists() && cargo_toml.is_file() {
                projects.push(dir.clone());
                // Don't recurse into subdirectories of projects
                return;
            }
            
            // Recurse into subdirectories
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        Self::find_cargo_projects_recursive(&path, projects);
                    }
                }
            }
        }
        
        /// Get directories to search for Rust files
        /// 
        /// If paths are files, returns them directly
        /// If paths are directories, returns them
        /// If a path is a project root, expands to src/, tests/, benches/
        pub fn get_search_dirs(&self) -> Vec<PathBuf> {
            let mut dirs = Vec::new();
            
            for path in &self.paths {
                if path.is_file() {
                    dirs.push(path.clone());
                } else if path.is_dir() {
                    // Check if this looks like a project root (has src/, tests/, or benches/)
                    let has_src = path.join("src").exists();
                    let has_tests = path.join("tests").exists();
                    let has_benches = path.join("benches").exists();
                    
                    if (has_src || has_tests || has_benches) && 
                       !path.file_name().is_some_and(|n| n == "src" || n == "tests" || n == "benches") {
                        // This is a project root - expand to standard directories
                        if has_src {
                            dirs.push(path.join("src"));
                        }
                        if has_tests {
                            dirs.push(path.join("tests"));
                        }
                        if has_benches {
                            dirs.push(path.join("benches"));
                        }
                    } else {
                        // This is a specific directory
                        dirs.push(path.clone());
                    }
                }
            }
            
            dirs
        }
    }

    /// Format a number with comma separators for readability
    /// 
    /// Examples:
    /// - 1234 -> "1,234"
    /// - 156036 -> "156,036"
    /// - 1000000 -> "1,000,000"
    pub fn format_number(n: usize) -> String {
        let s = n.to_string();
        let mut result = String::new();
        let mut count = 0;
        
        for c in s.chars().rev() {
            if count > 0 && count % 3 == 0 {
                result.push(',');
            }
            result.push(c);
            count += 1;
        }
        
        result.chars().rev().collect()
    }
    
    /// Find all Rust files recursively in one or more directories
    /// 
    /// Recursively searches directories for .rs files.
    /// Handles both single directories and multiple directories.
    pub fn find_rust_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
        fn search_dir(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
            if !dir.exists() {
                return;
            }
            
            // Skip directories that should be excluded
            if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str()) {
                if dir_name == "attic" || dir_name == "target" || dir_name.starts_with('.') {
                    return;
                }
            }
            
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().is_some_and(|e| e == "rs") {
                        files.push(path);
                    } else if path.is_dir() {
                        search_dir(&path, files);
                    }
                }
            }
        }
        
        let mut all_files = Vec::new();
        for path in dirs {
            if path.is_file() {
                // Direct file - add it if it's a .rs file
                if path.extension().is_some_and(|e| e == "rs") {
                    all_files.push(path.clone());
                }
            } else if path.is_dir() {
                // Directory - search recursively
                search_dir(path, &mut all_files);
            }
        }
        // Sort for deterministic, reproducible output across all tools
        all_files.sort();
        all_files
    }
    
    /// Get standard search directories for a base path
    /// 
    /// Returns src/, tests/, benches/ under the base path
    pub fn get_search_dirs(base: &PathBuf) -> Vec<PathBuf> {
        vec![
            base.join("src"),
            base.join("tests"),
            base.join("benches"),
        ]
    }
}

