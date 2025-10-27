use std::process::{Command, exit};
use std::path::PathBuf;


macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("analyses/compile_src_tests_benches_run_tests.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    let start = std::time::Instant::now();
    let mut all_success = true;
    
    let arg = if args.len() < 2 {
        "-c" // Default to codebase
    } else {
        args[1].as_str()
    };
    
    match arg {
        "-c" | "--codebase" => {
            log!("=== Grinding entire codebase ===\n");
            all_success = grind_codebase();
        }
        "-m" | "--module" => {
            if args.len() < 3 {
                eprintln!("Error: -m requires a module name");
                exit(1);
            }
            let module = &args[2];
            log!("=== Grinding module: {} ===\n", module);
            all_success = grind_module(module);
        }
        "-d" | "--dir" => {
            if args.len() < 3 {
                eprintln!("Error: -d requires a directory");
                exit(1);
            }
            let dir = &args[2];
            log!("=== Grinding directory: {} ===\n", dir);
            all_success = grind_directory(dir);
        }
        "-f" | "--file" => {
            if args.len() < 3 {
                eprintln!("Error: -f requires a file path");
                exit(1);
            }
            let file = &args[2];
            log!("=== Grinding file: {} ===\n", file);
            // For a single file, try to infer the module name
            if let Some(module) = extract_module_from_path(file) {
                log!("Inferred module: {}\n", module);
                all_success = grind_module(&module);
            } else {
                eprintln!("Error: Cannot infer module from file path: {}", file);
                eprintln!("File path should contain Chap##/ModuleName.rs pattern");
                exit(1);
            }
        }
        "-h" | "--help" => {
            print_usage(&args[0]);
            exit(0);
        }
        _ => {
            eprintln!("Error: Unknown option: {}", args[1]);
            print_usage(&args[0]);
            exit(1);
        }
    }
    
    log!("\nCompleted in {}ms", start.elapsed().as_millis());
    
    if !all_success {
        exit(1);
    }
}

fn print_usage(program_name: &str) {
    let name = std::path::Path::new(program_name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(program_name);
    
    log!("Usage: {} [OPTIONS]", name);
    log!("");
    log!("Options:");
    log!("  -c, --codebase             Compile and test entire codebase");
    log!("  -d, --dir DIR              Compile and test specific directory");
    log!("  -f, --file FILE            Compile and test file's module");
    log!("  -m, --module NAME          Compile and test specific module");
    log!("  -h, --help                 Show this help message");
    log!("");
    log!("Examples:");
    log!("  {} -c                        # Grind entire codebase", name);
    log!("  {} -m BSTSetSplayMtEph       # Grind module + tests + benches", name);
    log!("  {} -d Chap37                 # Grind Chap37 directory", name);
    log!("  {} -f src/Chap37/BST.rs      # Infer module and grind", name);
}

fn extract_module_from_path(path: &str) -> Option<String> {
    // Extract module name from path like "src/Chap37/BSTSetSplayMtEph.rs"
    let path_buf = PathBuf::from(path);
    if let Some(file_stem) = path_buf.file_stem() {
        return Some(file_stem.to_string_lossy().to_string());
    }
    None
}

fn grind_codebase() -> bool {
    log!("Step 1/4: Compiling src...");
    if !run_command("cargo", &["build", "--release", "--lib"]) {
        return false;
    }
    log!("✓ src compiled successfully\n");
    
    log!("Step 2/4: Compiling all tests...");
    if !run_command("cargo", &["test", "--release", "--no-run"]) {
        return false;
    }
    log!("✓ tests compiled successfully\n");
    
    log!("Step 3/4: Compiling all benches...");
    if !run_command("cargo", &["bench", "--no-run"]) {
        return false;
    }
    log!("✓ benches compiled successfully\n");
    
    log!("Step 4/4: Running all tests...");
    if !run_command("cargo", &["test", "--release"]) {
        return false;
    }
    log!("✓ all tests passed\n");
    
    true
}

fn grind_module(module: &str) -> bool {
    // Step 1: Compile src (full lib)
    log!("Step 1/4: Compiling src...");
    if !run_command("cargo", &["build", "--release", "--lib"]) {
        eprintln!("✗ Failed to compile src");
        return false;
    }
    log!("✓ src compiled successfully\n");
    
    // Step 2: Compile test module
    let test_name = format!("Test{}", module);
    log!("Step 2/4: Compiling test {}...", test_name);
    if !run_command("cargo", &["test", "--release", "--test", &test_name, "--no-run"]) {
        eprintln!("✗ Failed to compile test {}", test_name);
        return false;
    }
    log!("✓ test {} compiled successfully\n", test_name);
    
    // Step 3: Compile bench module (if exists)
    let bench_name = format!("Bench{}", module);
    log!("Step 3/4: Compiling bench {}...", bench_name);
    let bench_exists = run_command("cargo", &["bench", "--bench", &bench_name, "--no-run"]);
    if bench_exists {
        log!("✓ bench {} compiled successfully\n", bench_name);
    } else {
        log!("⊘ bench {} does not exist (OK)\n", bench_name);
    }
    
    // Step 4: Run tests for this module
    log!("Step 4/4: Running tests for {}...", test_name);
    if !run_command("cargo", &["test", "--release", "--test", &test_name]) {
        eprintln!("✗ Tests failed for {}", test_name);
        return false;
    }
    log!("✓ all tests passed for {}\n", test_name);
    
    true
}

fn grind_directory(dir: &str) -> bool {
    // For directory, compile entire lib, then compile/run tests for that directory
    log!("Step 1/4: Compiling src...");
    if !run_command("cargo", &["build", "--release", "--lib"]) {
        eprintln!("✗ Failed to compile src");
        return false;
    }
    log!("✓ src compiled successfully\n");
    
    // Find all test files in tests/{dir}/
    let test_dir = format!("tests/{}", dir);
    log!("Step 2/4: Finding tests in {}...", test_dir);
    
    let test_files = match std::fs::read_dir(&test_dir) {
        Ok(entries) => {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().extension().map_or(false, |ext| ext == "rs")
                })
                .filter_map(|e| {
                    e.path().file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                })
                .collect::<Vec<_>>()
        }
        Err(_) => {
            eprintln!("⊘ No test directory found: {}", test_dir);
            Vec::new()
        }
    };
    
    if test_files.is_empty() {
        log!("⊘ No test files found in {}\n", test_dir);
    } else {
        log!("Found {} test file(s)\n", test_files.len());
        
        log!("Step 3/4: Compiling tests in {}...", test_dir);
        for test_name in &test_files {
            if !run_command("cargo", &["test", "--release", "--test", test_name, "--no-run"]) {
                eprintln!("✗ Failed to compile test {}", test_name);
                return false;
            }
        }
        log!("✓ all tests in {} compiled successfully\n", test_dir);
        
        log!("Step 4/4: Running tests in {}...", test_dir);
        for test_name in &test_files {
            if !run_command("cargo", &["test", "--release", "--test", test_name]) {
                eprintln!("✗ Tests failed for {}", test_name);
                return false;
            }
        }
        log!("✓ all tests in {} passed\n", test_dir);
    }
    
    // Find and compile bench files
    let bench_dir = format!("benches/{}", dir);
    log!("Checking for benches in {}...", bench_dir);
    
    let bench_files = match std::fs::read_dir(&bench_dir) {
        Ok(entries) => {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().extension().map_or(false, |ext| ext == "rs")
                })
                .filter_map(|e| {
                    e.path().file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                })
                .collect::<Vec<_>>()
        }
        Err(_) => {
            log!("⊘ No bench directory found: {}\n", bench_dir);
            Vec::new()
        }
    };
    
    if !bench_files.is_empty() {
        log!("Found {} bench file(s)", bench_files.len());
        for bench_name in &bench_files {
            if !run_command("cargo", &["bench", "--bench", bench_name, "--no-run"]) {
                eprintln!("✗ Failed to compile bench {}", bench_name);
                return false;
            }
        }
        log!("✓ all benches in {} compiled successfully\n", bench_dir);
    }
    
    true
}

fn run_command(program: &str, args: &[&str]) -> bool {
    use std::process::Stdio;
use std::fs;
    
    let status = Command::new(program)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to execute command");
    
    status.success()
}
