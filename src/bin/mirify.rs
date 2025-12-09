use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

struct Args {
    codebase: PathBuf,
    max_projects: Option<usize>,
    jobs: usize,
    clean_first: bool,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args_iter = std::env::args().skip(1);
        let mut codebase = None;
        let mut max_projects = None;
        let mut jobs = 1; // Default sequential for safety
        let mut clean_first = false;

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
    -j, --jobs <N>              Number of parallel builds (default: 1)
    --clean                     Run 'cargo clean' before generating MIR (removes old binaries)
    -h, --help                  Print this help message

DESCRIPTION:
    Runs 'cargo check --emit=mir' on Rust projects to generate MIR files.
    MIR (Mid-level Intermediate Representation) contains fully-typed function calls.
    
    Caches: Skips projects that already have MIR files.
    
EXAMPLES:
    rusticate-mirify -C ~/projects/RustCodebases -m 50 -j 4
    rusticate-mirify -C ~/projects/my-project
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

fn clean_project(project_path: &Path) -> Result<()> {
    let output = std::process::Command::new("cargo")
        .arg("clean")
        .current_dir(project_path)
        .output()
        .context("Failed to run cargo clean")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Cargo clean failed:\n{}", stderr);
    }
    
    Ok(())
}

fn mirify_project(project_path: &Path, clean_first: bool) -> Result<()> {
    if clean_first {
        clean_project(project_path)?;
    }
    
    let output = std::process::Command::new("cargo")
        .arg("check")
        .current_dir(project_path)
        .env("RUSTFLAGS", "--emit=mir")
        .output()
        .context("Failed to run cargo check")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Cargo check failed:\n{}", stderr);
    }
    
    Ok(())
}

fn main() -> Result<()> {
    let overall_start = std::time::Instant::now();
    let args = Args::parse()?;
    
    // Set up logging
    let log_path = PathBuf::from("analyses/mirify.log");
    fs::create_dir_all("analyses")?;
    let mut log_file = fs::File::create(&log_path)
        .context("Failed to create log file")?;
    
    macro_rules! log {
        ($($arg:tt)*) => {
            writeln!(log_file, $($arg)*).ok();
        };
    }
    
    // Log header
    log!("rusticate-mirify");
    log!("=================");
    log!("Command: {}", std::env::args().collect::<Vec<_>>().join(" "));
    log!("Codebase: {}", args.codebase.display());
    log!("Jobs: {}", args.jobs);
    if let Some(max) = args.max_projects {
        log!("Max projects: {}", max);
    }
    log!("Started: {:?}\n", overall_start);
    
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
        log!("Limiting to {} projects", max);
        println!("Limiting to {} projects", max);
        projects.truncate(max);
    }
    
    log!("Found {} projects\n", projects.len());
    println!("Found {} projects\n", projects.len());
    
    // Counters
    let total_projects = projects.len();
    let mir_reused = Arc::new(AtomicUsize::new(0));
    let compiled = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));
    
    // Process in parallel using thread pool
    let chunk_size = (projects.len() + args.jobs - 1) / args.jobs;
    let chunks: Vec<_> = projects.chunks(chunk_size).map(|c| c.to_vec()).collect();
    let clean_first = args.clean_first;
    
    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            let mir_reused = Arc::clone(&mir_reused);
            let compiled = Arc::clone(&compiled);
            let failed = Arc::clone(&failed);
            
            std::thread::spawn(move || {
                for project in chunk {
                    let name = project.file_name().unwrap().to_string_lossy();
                    
                    // Check if MIR already exists (skip check if cleaning)
                    if !clean_first && check_mir_exists(&project) {
                        println!("  [CACHED] {}", name);
                        mir_reused.fetch_add(1, Ordering::Relaxed);
                    } else {
                        if clean_first {
                            print!("  [CLEAN+BUILD] {} ... ", name);
                        } else {
                            print!("  [BUILD]  {} ... ", name);
                        }
                        std::io::stdout().flush().ok();
                        
                        match mirify_project(&project, clean_first) {
                            Ok(_) => {
                                println!("OK");
                                compiled.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(e) => {
                                println!("FAILED: {}", e);
                                failed.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }
            })
        })
        .collect();
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    let elapsed = overall_start.elapsed();
    
    // Final stats
    let reused = mir_reused.load(Ordering::Relaxed);
    let built = compiled.load(Ordering::Relaxed);
    let errors = failed.load(Ordering::Relaxed);
    
    println!("\n=== Summary ===");
    println!("Total projects: {}", total_projects);
    println!("  MIR cached:   {}", reused);
    println!("  Compiled:     {}", built);
    println!("  Failed:       {}", errors);
    println!("\nTOTAL TIME: {} ms ({:.2} seconds)", elapsed.as_millis(), elapsed.as_secs_f64());
    
    log!("\n=== Summary ===");
    log!("Total projects: {}", total_projects);
    log!("  MIR cached:   {}", reused);
    log!("  Compiled:     {}", built);
    log!("  Failed:       {}", errors);
    log!("\nTOTAL TIME: {} ms ({:.2} seconds)", elapsed.as_millis(), elapsed.as_secs_f64());
    log!("Finished: {:?}", std::time::Instant::now());
    
    println!("\nLog: {}", log_path.display());
    
    Ok(())
}

