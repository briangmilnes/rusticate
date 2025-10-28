use anyhow::Result;
use rusticate::StandardArgs;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

fn main() -> Result<()> {
    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/compile.log").ok();

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

    log!("Entering directory '{}'", std::env::current_dir()?.display());
    println!();

    let current_dir = std::env::current_dir()?;
    
    // Determine mode based on StandardArgs
    let mode = if args.is_module_search {
        // Module mode: StandardArgs already found the module files
        if args.paths.is_empty() {
            eprintln!("Error: No module files found");
            return Ok(());
        }
        // Extract module name from first path (src file)
        let module_name = args.paths[0]
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        ("module", module_name)
    } else if args.paths.len() == 1 && args.paths[0].is_file() {
        // Single file mode
        ("file", args.paths[0].display().to_string())
    } else if args.paths.len() == 1 && args.paths[0].is_dir() {
        // Directory mode
        ("dir", args.paths[0].display().to_string())
    } else {
        // Codebase mode (default)
        ("codebase", String::new())
    };

    match mode.0 {
        "module" => {
            let module_name = &mode.1;
            log!("{}", "=".repeat(80));
            log!("COMPILING MODULE: {}", module_name);
            log!("{}", "=".repeat(80));
            println!();

            // Find src, test, and bench files for this module
            let src_file = find_module_file(&current_dir, "src", module_name)?;
            let test_files = find_test_files(&current_dir, module_name)?;
            let bench_files = find_bench_files(&current_dir, module_name)?;

            log!("Found module files:");
            if let Some(ref src) = src_file {
                log!("  src:   {}", src.display());
            } else {
                log!("  src:   (not found)");
            }
            log!("  tests: {}", test_files.len());
            for test in &test_files {
                log!("         {}", test.display());
            }
            log!("  bench: {}", bench_files.len());
            for bench in &bench_files {
                log!("         {}", bench.display());
            }
            println!();

            // Compile lib (includes src file)
            if src_file.is_some() {
                log!("{}", "=".repeat(80));
                log!("STEP 1: Compile library (includes src module)");
                log!("{}", "=".repeat(80));
                let success = run_compile(&current_dir, &["check", "--lib"])?;
                if !success {
                    log!("\n✗ COMPILE FAILED at: library compilation");
                    return Ok(());
                }
                log!("✓ Library compilation passed");
                println!();
            }

            // Compile each test
            for (idx, test_file) in test_files.iter().enumerate() {
                let test_name = extract_test_name(test_file);
                log!("{}", "=".repeat(80));
                log!("STEP {}: Compile test {}", idx + 2, test_name);
                log!("{}", "=".repeat(80));
                let success = run_compile(&current_dir, &["test", "--test", &test_name, "--no-run"])?;
                if !success {
                    log!("\n✗ COMPILE FAILED at: test {}", test_name);
                    return Ok(());
                }
                log!("✓ Test {} compilation passed", test_name);
                println!();
            }

            // Compile each benchmark
            for (idx, bench_file) in bench_files.iter().enumerate() {
                let bench_name = extract_bench_name(bench_file);
                log!("{}", "=".repeat(80));
                log!("STEP {}: Compile benchmark {}", idx + 2 + test_files.len(), bench_name);
                log!("{}", "=".repeat(80));
                let success = run_compile(&current_dir, &["bench", "--bench", &bench_name, "--no-run"])?;
                if !success {
                    log!("\n✗ COMPILE FAILED at: benchmark {}", bench_name);
                    return Ok(());
                }
                log!("✓ Benchmark {} compilation passed", bench_name);
                println!();
            }

            log!("{}", "=".repeat(80));
            log!("✓ ALL COMPILATION COMPLETE for module '{}'", module_name);
            log!("{}", "=".repeat(80));
        }
        "file" => {
            let file_path = PathBuf::from(&mode.1);
            log!("{}", "=".repeat(80));
            log!("CHECKING FILE: {}", file_path.display());
            log!("{}", "=".repeat(80));
            println!();
            
            // For a single file, just run cargo check --lib (will check the file as part of lib)
            let success = run_compile(&current_dir, &["check", "--lib"])?;
            if !success {
                log!("\n✗ COMPILE FAILED");
                return Ok(());
            }
            log!("✓ File check passed");
        }
        "dir" => {
            let dir_path = PathBuf::from(&mode.1);
            log!("{}", "=".repeat(80));
            log!("COMPILING DIRECTORY: {}", dir_path.display());
            log!("{}", "=".repeat(80));
            println!();
            
            // Just compile the whole lib/tests/benches - cargo will pick up changes
            let success = run_compile(&current_dir, &["check", "--lib", "--tests", "--benches"])?;
            if !success {
                log!("\n✗ COMPILE FAILED");
                return Ok(());
            }
            log!("✓ Directory compilation passed");
        }
        "codebase" => {
            log!("{}", "=".repeat(80));
            log!("STEP 1: COMPILE LIBRARY");
            log!("{}", "=".repeat(80));
            println!();
            
            let success = run_compile(&current_dir, &["build", "--lib"])?;
            if !success {
                log!("\n✗ COMPILE FAILED at: library");
                return Ok(());
            }
            log!("✓ Library compilation passed");
            println!();
            
            log!("{}", "=".repeat(80));
            log!("STEP 2: COMPILE ALL TESTS");
            log!("{}", "=".repeat(80));
            println!();
            
            let success = run_compile(&current_dir, &["test", "--no-run"])?;
            if !success {
                log!("\n✗ COMPILE FAILED at: tests");
                return Ok(());
            }
            log!("✓ All tests compilation passed");
            println!();
            
            log!("{}", "=".repeat(80));
            log!("STEP 3: COMPILE ALL BENCHMARKS");
            log!("{}", "=".repeat(80));
            println!();
            
            let success = run_compile(&current_dir, &["bench", "--no-run"])?;
            if !success {
                log!("\n✗ COMPILE FAILED at: benchmarks");
                return Ok(());
            }
            log!("✓ All benchmarks compilation passed");
        }
        _ => unreachable!(),
    }

    let elapsed = start_time.elapsed();
    log!("\nCompleted in {}ms", elapsed.as_millis());

    Ok(())
}

fn run_compile(cwd: &Path, args: &[&str]) -> Result<bool> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(cwd);
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Print all output
    for line in stdout.lines() {
        println!("{line}");
    }
    for line in stderr.lines() {
        println!("{line}");
    }

    Ok(output.status.success())
}

fn find_module_file(base: &Path, dir: &str, module_name: &str) -> Result<Option<PathBuf>> {
    let src_dir = base.join(dir);
    if !src_dir.exists() {
        return Ok(None);
    }

    // Look in all ChapNN subdirectories
    for entry in fs::read_dir(&src_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let dir_name = path.file_name().unwrap().to_str().unwrap();
            if dir_name.starts_with("Chap") {
                // Check for the module file
                let module_file = path.join(format!("{module_name}.rs"));
                if module_file.exists() {
                    return Ok(Some(module_file));
                }
            }
        }
    }

    Ok(None)
}

fn find_test_files(base: &Path, module_name: &str) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    let tests_dir = base.join("tests");
    if !tests_dir.exists() {
        return Ok(result);
    }

    for entry in fs::read_dir(&tests_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let dir_name = path.file_name().unwrap().to_str().unwrap();
            if dir_name.starts_with("Chap") || dir_name == "tests" {
                // Look for Test<ModuleName>.rs or Test*<ModuleName>*.rs
                for test_entry in fs::read_dir(&path)? {
                    let test_entry = test_entry?;
                    let test_path = test_entry.path();
                    if test_path.is_file() {
                        if let Some(name) = test_path.file_name() {
                            let name_str = name.to_str().unwrap();
                            if name_str.contains(module_name) && name_str.starts_with("Test") && name_str.ends_with(".rs") {
                                result.push(test_path);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(result)
}

fn find_bench_files(base: &Path, module_name: &str) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    let benches_dir = base.join("benches");
    if !benches_dir.exists() {
        return Ok(result);
    }

    for entry in fs::read_dir(&benches_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let dir_name = path.file_name().unwrap().to_str().unwrap();
            if dir_name.starts_with("Chap") || dir_name == "benches" {
                // Look for Bench<ModuleName>.rs or Bench*<ModuleName>*.rs
                for bench_entry in fs::read_dir(&path)? {
                    let bench_entry = bench_entry?;
                    let bench_path = bench_entry.path();
                    if bench_path.is_file() {
                        if let Some(name) = bench_path.file_name() {
                            let name_str = name.to_str().unwrap();
                            if name_str.contains(module_name) && name_str.starts_with("Bench") && name_str.ends_with(".rs") {
                                result.push(bench_path);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(result)
}

fn extract_test_name(path: &Path) -> String {
    path.file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}

fn extract_bench_name(path: &Path) -> String {
    path.file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}

