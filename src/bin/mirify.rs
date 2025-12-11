use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

struct Args {
    codebase: PathBuf,
    max_projects: Option<usize>,
    jobs: usize,
    clean_first: bool,
    clean_artifacts: bool,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args_iter = std::env::args().skip(1);
        let mut codebase = None;
        let mut max_projects = None;
        let mut jobs = 1; // Default sequential for safety
        let mut clean_first = false;
        let mut clean_artifacts = false;

        while let Some(arg) = args_iter.next() {
            match arg.as_str() {
                "-C" | "--codebase" => {
                    codebase = Some(PathBuf::from(
                        args_iter
                            .next()
                            .context("Expected path after -C/--codebase")?
                    ));
                }
                "-m" | "--max-projects" => {
                    let max = args_iter
                        .next()
                        .context("Expected number after -m/--max-projects")?
                        .parse::<usize>()
                        .context("Invalid number for -m/--max-projects")?;
                    max_projects = Some(max);
                }
                "-j" | "--jobs" => {
                    jobs = args_iter
                        .next()
                        .context("Expected number after -j/--jobs")?
                        .parse::<usize>()
                        .context("Invalid number for -j/--jobs")?;
                    if jobs == 0 {
                        bail!("--jobs must be at least 1");
                    }
                }
                "--clean" => {
                    clean_first = true;
                }
                "--clean-artifacts" => {
                    clean_artifacts = true;
                }
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => {
                    bail!("Unknown argument: {}\nRun with --help for usage", arg);
                }
            }
        }

        let codebase = codebase.context("Missing required argument: -C/--codebase\nRun with --help for usage")?;

        if !codebase.exists() {
            bail!("Codebase path does not exist: {}", codebase.display());
        }
        if !codebase.is_dir() {
            bail!("Codebase path is not a directory: {}", codebase.display());
        }

        Ok(Args {
            codebase,
            max_projects,
            jobs,
            clean_first,
            clean_artifacts,
        })
    }
}

fn print_help() {
    println!(
        r#"rusticate-mirify - Generate MIR for Rust projects

USAGE:
    rusticate-mirify -C <PATH> [-m <N>] [-j <N>] [--clean]

OPTIONS:
    -C, --codebase <PATH>       Path to a project or directory of projects [required]
    -m, --max-projects <N>      Limit number of projects to process (default: unlimited)
    -j, --jobs <N>              Number of threads for each cargo build (default: 1)
                                Projects are built sequentially to avoid cargo package cache lock contention.
                                This flag controls the parallelism within each cargo invocation.
    --clean                     Run 'cargo clean' before generating MIR (removes everything)
    --clean-artifacts           Delete build artifacts but keep *.mir files (applies to all projects)
    -h, --help                  Print this help message

DESCRIPTION:
    Runs 'cargo check --tests --emit=mir' on Rust projects to generate MIR files.
    MIR (Mid-level Intermediate Representation) contains fully-typed function calls.
    
    Note: --tests is always used to include test code in the MIR output.
    
    Projects are processed sequentially (one at a time) to avoid cargo package cache locking.
    The -j flag controls how many threads each individual cargo uses for compilation.
    
    Caches: Skips projects that already have MIR files (unless --clean is used).
    
EXAMPLES:
    rusticate-mirify -C ~/projects/RustCodebases -m 50 -j 6
    rusticate-mirify -C ~/projects/my-project --clean
"#
    );
}

fn find_rust_projects(dir: &Path) -> Vec<PathBuf> {
    let mut projects = Vec::new();
    
    for entry in WalkDir::new(dir)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.join("Cargo.toml").exists() && path != dir {
            projects.push(path.to_path_buf());
        }
    }
    
    projects.sort();
    projects
}

fn check_mir_exists(project_path: &Path) -> bool {
    let target_dir = project_path.join("target/debug/deps");
    if !target_dir.exists() {
        return false;
    }
    
    if let Ok(entries) = fs::read_dir(&target_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("mir") {
                return true;
            }
        }
    }
    false
}

fn clean_project(project_path: &Path, log_file: Arc<Mutex<fs::File>>) -> Result<()> {
    let output = std::process::Command::new("cargo")
        .arg("clean")
        .current_dir(project_path)
        .output()
        .context("Failed to run cargo clean")?;
    
    if output.status.success() {
        let name = project_path.file_name().unwrap().to_string_lossy();
        if let Ok(mut log) = log_file.lock() {
            writeln!(log, "  Cleaned: {}", name).ok();
            log.flush().ok();
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Cargo clean failed:\n{}", stderr);
    }
    
    Ok(())
}

fn clean_artifacts_keep_mir(project_path: &Path) -> Result<()> {
    // Delete build artifacts but keep *.mir files
    // Clean ALL target directories (including workspace members)
    
    // Find all target directories within this project
    let output = std::process::Command::new("find")
        .arg(project_path)
        .arg("-maxdepth")
        .arg("5")  // Deep enough for workspace members
        .arg("-type")
        .arg("d")
        .arg("-name")
        .arg("target")
        .output()
        .context("Failed to find target directories")?;
    
    let target_dirs: Vec<PathBuf> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();
    
    if target_dirs.is_empty() {
        return Ok(());
    }
    
    for target_dir in target_dirs {
        // Delete file artifacts (all extensions except .mir)
        for ext in &["rmeta", "rlib", "so", "a", "dylib", "dll", "exe", "d", "o", "json"] {
            let _ = std::process::Command::new("find")
                .arg(&target_dir)
                .arg("-type")
                .arg("f")
                .arg("-name")
                .arg(format!("*.{}", ext))
                .arg("-delete")
                .output();
        }
        
        // Delete executables (files without extensions in deps/)
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "find '{}' -path '*/deps/*' -type f -executable ! -name '*.mir' -delete 2>/dev/null",
                target_dir.display()
            ))
            .output();
        
        // Delete incremental compilation cache (largest space hog!)
        let _ = std::process::Command::new("rm")
            .arg("-rf")
            .arg(target_dir.join("debug/incremental"))
            .arg(target_dir.join("release/incremental"))
            .output();
        
        // Delete build script outputs
        let _ = std::process::Command::new("rm")
            .arg("-rf")
            .arg(target_dir.join("debug/build"))
            .arg(target_dir.join("release/build"))
            .output();
        
        // Delete .fingerprint directories
        let _ = std::process::Command::new("rm")
            .arg("-rf")
            .arg(target_dir.join("debug/.fingerprint"))
            .arg(target_dir.join("release/.fingerprint"))
            .output();
        
        // Clean up empty directories
        let _ = std::process::Command::new("find")
            .arg(&target_dir)
            .arg("-type")
            .arg("d")
            .arg("-empty")
            .arg("-delete")
            .output();
    }
    
    Ok(())
}

fn mirify_project(project_path: &Path, clean_first: bool, jobs: usize, log_file: Arc<Mutex<fs::File>>, err_log: Arc<Mutex<fs::File>>) -> Result<()> {
    if clean_first {
        clean_project(project_path, Arc::clone(&log_file))?;
    }
    
    let output = std::process::Command::new("cargo")
        .arg("check")
        .arg("--tests")   // Include test code in MIR output
        .arg("--quiet")   // Suppress "Finished" and other progress messages
        .arg("-j")
        .arg(jobs.to_string())
        .current_dir(project_path)
        .env("RUSTFLAGS", "--emit=mir")
        .output()
        .context("Failed to run cargo check")?;
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // Always log stderr to error file
    if !stderr.is_empty() {
        if let Ok(mut err) = err_log.lock() {
            let name = project_path.file_name().unwrap().to_string_lossy();
            writeln!(err, "\n=== {} ===", name).ok();
            write!(err, "{}", stderr).ok();
            err.flush().ok();
        }
    }
    
    if !output.status.success() {
        bail!("Build failed (see rusticate-mirify.errs)");
    }
    
    Ok(())
}

fn main() -> Result<()> {
    let overall_start = std::time::Instant::now();
    let args = Args::parse()?;
    
    // Set up logging
    let log_path = PathBuf::from("analyses/rusticate-mirify.log");
    let err_path = PathBuf::from("analyses/rusticate-mirify.errs");
    fs::create_dir_all("analyses")?;
    let log_file = fs::File::create(&log_path)
        .context("Failed to create log file")?;
    let err_file = fs::File::create(&err_path)
        .context("Failed to create error file")?;
    let shared_log = Arc::new(Mutex::new(log_file));
    let shared_err = Arc::new(Mutex::new(err_file));
    
    // Log header
    {
        let mut log = shared_log.lock().unwrap();
        writeln!(log, "rusticate-mirify").ok();
        writeln!(log, "=================").ok();
        writeln!(log, "Command: {}", std::env::args().collect::<Vec<_>>().join(" ")).ok();
        writeln!(log, "Codebase: {}", args.codebase.display()).ok();
        writeln!(log, "Jobs: {}", args.jobs).ok();
        if let Some(max) = args.max_projects {
            writeln!(log, "Max projects: {}", max).ok();
        }
        writeln!(log, "Started: {:?}\n", overall_start).ok();
        log.flush().ok();
    }
    
    println!("rusticate-mirify");
    println!("=================");
    println!("Codebase: {}", args.codebase.display());
    println!("Jobs: {}", args.jobs);
    if let Some(max) = args.max_projects {
        println!("Max projects: {}", max);
    }
    println!();
    
    // Find projects
    let mut projects = if args.codebase.join("Cargo.toml").exists() {
        vec![args.codebase.clone()]
    } else {
        find_rust_projects(&args.codebase)
    };
    
    if projects.is_empty() {
        bail!("No Rust projects found in {}", args.codebase.display());
    }
    
    // Apply max limit
    if let Some(max) = args.max_projects {
        println!("Limiting to {} projects", max);
        if let Ok(mut log) = shared_log.lock() {
            writeln!(log, "Limiting to {} projects", max).ok();
        }
        projects.truncate(max);
    }
    
    println!("Found {} projects\n", projects.len());
    {
        let mut log = shared_log.lock().unwrap();
        writeln!(log, "Found {} projects\n", projects.len()).ok();
        log.flush().ok();
    }
    
    // Counters
    let total_projects = projects.len();
    let mut mir_reused = 0;
    let mut compiled = 0;
    let mut failed = 0;
    
    // Process projects sequentially
    let clean_first = args.clean_first;
    let clean_artifacts = args.clean_artifacts;
    let jobs = args.jobs;
    
    for project in projects {
        let name = project.file_name()
            .or_else(|| project.iter().last())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string());
        let project_start = std::time::Instant::now();
        
        // Check if MIR already exists (skip check if cleaning)
        if !clean_first && check_mir_exists(&project) {
            // Clean artifacts even for cached projects if requested
            if clean_artifacts {
                if let Err(e) = clean_artifacts_keep_mir(&project) {
                    let elapsed = project_start.elapsed();
                    let msg = format!("  [CACHED+CLEAN-FAILED] {} - {} ({:.2}s)", name, e, elapsed.as_secs_f64());
                    println!("{}", msg);
                    if let Ok(mut log) = shared_log.lock() {
                        writeln!(log, "{}", msg).ok();
                    }
                } else {
                    let elapsed = project_start.elapsed();
                    let msg = format!("  [CACHED+CLEANED] {} ({:.2}s)", name, elapsed.as_secs_f64());
                    println!("{}", msg);
                    if let Ok(mut log) = shared_log.lock() {
                        writeln!(log, "{}", msg).ok();
                    }
                }
            } else {
                let elapsed = project_start.elapsed();
                let msg = format!("  [CACHED] {} ({:.2}s)", name, elapsed.as_secs_f64());
                println!("{}", msg);
                if let Ok(mut log) = shared_log.lock() {
                    writeln!(log, "{}", msg).ok();
                }
            }
            
            mir_reused += 1;
        } else {
            let prefix = if clean_first {
                format!("  [CLEAN+BUILD] {}", name)
            } else {
                format!("  [BUILD]  {}", name)
            };
            print!("{} ... ", prefix);
            std::io::stdout().flush().ok();
            
            match mirify_project(&project, clean_first, jobs, Arc::clone(&shared_log), Arc::clone(&shared_err)) {
                Ok(()) => {
                    // Clean artifacts if requested
                    let msg = if clean_artifacts {
                        if let Err(e) = clean_artifacts_keep_mir(&project) {
                            let elapsed = project_start.elapsed();
                            format!("{} ... OK but cleanup failed: {} ({:.2}s)", prefix, e, elapsed.as_secs_f64())
                        } else {
                            let elapsed = project_start.elapsed();
                            format!("{} ... OK+CLEANED ({:.2}s)", prefix, elapsed.as_secs_f64())
                        }
                    } else {
                        let elapsed = project_start.elapsed();
                        format!("{} ... OK ({:.2}s)", prefix, elapsed.as_secs_f64())
                    };
                    
                    println!("{}", msg.trim_start_matches(&format!("{} ... ", prefix)));
                    
                    // Log immediately
                    if let Ok(mut log) = shared_log.lock() {
                        writeln!(log, "{}", msg).ok();
                        log.flush().ok();
                    }
                    
                    compiled += 1;
                }
                Err(e) => {
                    let elapsed = project_start.elapsed();
                    let msg = format!("{} ... FAILED: {} ({:.2}s)", prefix, e, elapsed.as_secs_f64());
                    println!("{}", msg.trim_start_matches(&format!("{} ... ", prefix)));
                    
                    // Log immediately
                    if let Ok(mut log) = shared_log.lock() {
                        writeln!(log, "{}", msg).ok();
                        log.flush().ok();
                    }
                    
                    failed += 1;
                }
            }
        }
    }
    
    let elapsed = overall_start.elapsed();
    
    // Final stats
    let build_success_pct = if total_projects > 0 {
        (compiled as f64 / total_projects as f64) * 100.0
    } else {
        0.0
    };
    
    println!("\n=== Summary ===");
    println!("Total projects: {}", total_projects);
    println!("  MIR cached:   {}", mir_reused);
    println!("  Compiled:     {} ({:.1}%)", compiled, build_success_pct);
    println!("  Failed:       {}", failed);
    println!("\nTOTAL TIME: {} ms ({:.2} seconds)", elapsed.as_millis(), elapsed.as_secs_f64());
    
    // Log summary
    {
        let mut log = shared_log.lock().unwrap();
        writeln!(log, "\n=== Summary ===").ok();
        writeln!(log, "Total projects: {}", total_projects).ok();
        writeln!(log, "  MIR cached:   {}", mir_reused).ok();
        writeln!(log, "  Compiled:     {} ({:.1}%)", compiled, build_success_pct).ok();
        writeln!(log, "  Failed:       {}", failed).ok();
        writeln!(log, "\nTOTAL TIME: {} ms ({:.2} seconds)", elapsed.as_millis(), elapsed.as_secs_f64()).ok();
        writeln!(log, "Finished: {:?}", std::time::Instant::now()).ok();
        log.flush().ok();
    }
    
    println!("\nLog: {}", log_path.display());
    
    Ok(())
}

