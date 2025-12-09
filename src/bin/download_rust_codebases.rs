use anyhow::{Context, Result};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

struct Args {
    input: PathBuf,
    output_dir: PathBuf,
    jobs: usize,
    shallow: bool,
    github_only: bool,
    skip_existing: bool,
    dry_run: bool,
    max_repos: Option<usize>,
    start_at: usize,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args = std::env::args().skip(1);
        let mut input = PathBuf::from("analyses/top1000_unique_repos.txt");
        let mut output_dir = PathBuf::from(
            std::env::var("HOME").unwrap_or_else(|_| "/home".to_string())
        ).join("projects/RustCodebases");
        let mut jobs = 4;
        let mut shallow = true;
        let mut github_only = true;
        let mut skip_existing = true;
        let mut dry_run = false;
        let mut max_repos = None;
        let mut start_at = 0;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-i" | "--input" => {
                    input = PathBuf::from(
                        args.next()
                            .context("Expected value after --input")?
                    );
                }
                "-o" | "--output-dir" => {
                    output_dir = PathBuf::from(
                        args.next()
                            .context("Expected value after --output-dir")?
                    );
                }
                "-j" | "--jobs" => {
                    jobs = args
                        .next()
                        .context("Expected value after --jobs")?
                        .parse()
                        .context("Invalid number for --jobs")?;
                }
                "--shallow" => shallow = true,
                "--full" => shallow = false,
                "--github-only" => github_only = true,
                "--include-all" => github_only = false,
                "--skip-existing" => skip_existing = true,
                "--overwrite" => skip_existing = false,
                "--dry-run" => dry_run = true,
                "--max-repos" => {
                    let n: usize = args
                        .next()
                        .context("Expected value after --max-repos")?
                        .parse()
                        .context("Invalid number for --max-repos")?;
                    max_repos = Some(n);
                }
                "--start-at" => {
                    start_at = args
                        .next()
                        .context("Expected value after --start-at")?
                        .parse()
                        .context("Invalid number for --start-at")?;
                }
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => {
                    eprintln!("Unknown option: {}", arg);
                    print_help();
                    std::process::exit(1);
                }
            }
        }

        Ok(Args {
            input,
            output_dir,
            jobs,
            shallow,
            github_only,
            skip_existing,
            dry_run,
            max_repos,
            start_at,
        })
    }
}

fn print_help() {
    println!(
        r#"rusticate-download-rust-codebases - Clone Rust crate repositories

USAGE:
    rusticate-download-rust-codebases [OPTIONS]

OPTIONS:
    -i, --input <FILE>        Input file with repository list
                              (default: analyses/top1000_unique_repos.txt)
    -o, --output-dir <DIR>    Output directory (default: ~/projects/RustCodebases)
    -j, --jobs <N>            Number of parallel clones (default: 4)
    --shallow                 Use shallow clone (--depth 1) [default]
    --full                    Use full clone with history
    --github-only             Only clone GitHub repositories [default]
    --include-all             Include GitLab and other sources
    --skip-existing           Skip repositories that already exist [default]
    --overwrite               Re-clone even if repository exists
    --dry-run                 Show what would be cloned without actually cloning
    --max-repos <N>           Limit number of repositories to clone (default: unlimited)
    --start-at <N>            Start cloning from repository N (for resuming)
    -h, --help                Print this help message

EXAMPLES:
    # Clone top 1000 (default settings: shallow, GitHub only, skip existing)
    rusticate-download-rust-codebases

    # Clone with 8 parallel jobs
    rusticate-download-rust-codebases -j 8

    # Do a dry run first
    rusticate-download-rust-codebases --dry-run

    # Clone first 100 repos only
    rusticate-download-rust-codebases --max-repos 100

    # Resume from repo 250
    rusticate-download-rust-codebases --start-at 250
"#
    );
}

#[derive(Debug, Clone)]
struct Repository {
    source: String,
    path: String,
    url: String,
}

fn parse_repos(input_file: &Path, github_only: bool) -> Result<Vec<Repository>> {
    let file = fs::File::open(input_file)
        .with_context(|| format!("Failed to open input file: {}", input_file.display()))?;
    let reader = BufReader::new(file);

    let mut repos = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() != 2 {
            eprintln!("Warning: Skipping malformed line: {}", line);
            continue;
        }

        let source = parts[0];
        let path = parts[1];

        if github_only && source != "github" {
            continue;
        }

        let url = match source {
            "github" => format!("https://github.com/{}.git", path),
            "gitlab" => format!("https://gitlab.com/{}.git", path),
            _ => {
                eprintln!("Warning: Unknown source '{}' for repo: {}", source, path);
                continue;
            }
        };

        repos.push(Repository {
            source: source.to_string(),
            path: path.to_string(),
            url,
        });
    }

    Ok(repos)
}

fn clone_repo(
    repo: &Repository,
    output_dir: &Path,
    shallow: bool,
    skip_existing: bool,
) -> Result<bool> {
    // Extract repo name from path (last component)
    let repo_name = repo
        .path
        .split('/')
        .last()
        .context("Invalid repository path")?;
    let target_dir = output_dir.join(repo_name);

    // Check if already exists
    if target_dir.exists() {
        if skip_existing {
            return Ok(false); // Skipped
        } else {
            // Remove existing and re-clone
            fs::remove_dir_all(&target_dir)
                .with_context(|| format!("Failed to remove existing directory: {}", target_dir.display()))?;
        }
    }

    // Build git clone command
    let mut cmd = Command::new("git");
    cmd.arg("clone");

    if shallow {
        cmd.arg("--depth").arg("1");
    }

    cmd.arg(&repo.url).arg(&target_dir);
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null());

    let status = cmd.status()
        .context("Failed to execute git clone")?;

    if !status.success() {
        anyhow::bail!("Git clone failed for {}", repo.url);
    }

    Ok(true) // Cloned
}

fn main() -> Result<()> {
    let start_time = Instant::now();

    let args = Args::parse()?;

    // Create log file
    let log_file_path = PathBuf::from("analyses/download_rust_codebases.log");
    fs::create_dir_all("analyses").ok();
    let mut log_file = fs::File::create(&log_file_path)
        .context("Failed to create log file")?;

    writeln!(log_file, "rusticate-download-rust-codebases")?;
    writeln!(log_file, "Started at: {:?}", start_time)?;
    writeln!(log_file, "Settings:")?;
    writeln!(log_file, "  Input: {}", args.input.display())?;
    writeln!(log_file, "  Output: {}", args.output_dir.display())?;
    writeln!(log_file, "  Jobs: {}", args.jobs)?;
    writeln!(log_file, "  Shallow: {}", args.shallow)?;
    writeln!(log_file, "  GitHub only: {}", args.github_only)?;
    writeln!(log_file, "  Skip existing: {}", args.skip_existing)?;
    writeln!(log_file, "  Dry run: {}", args.dry_run)?;
    writeln!(log_file, "  Max repos: {:?}", args.max_repos)?;
    writeln!(log_file, "  Start at: {}", args.start_at)?;
    writeln!(log_file)?;

    println!("rusticate-download-rust-codebases");
    println!("==================================");
    println!("Input:        {}", args.input.display());
    println!("Output:       {}", args.output_dir.display());
    println!("Jobs:         {}", args.jobs);
    println!("Clone mode:   {}", if args.shallow { "shallow" } else { "full" });
    println!("Sources:      {}", if args.github_only { "GitHub only" } else { "all" });
    println!("Existing:     {}", if args.skip_existing { "skip" } else { "overwrite" });
    println!();

    // Parse repository list
    let mut repos = parse_repos(&args.input, args.github_only)?;
    println!("Loaded {} repositories", repos.len());
    writeln!(log_file, "Loaded {} repositories", repos.len())?;

    // Apply start_at and max_repos filters
    if args.start_at > 0 {
        if args.start_at >= repos.len() {
            anyhow::bail!("--start-at {} is beyond the repository count ({})", args.start_at, repos.len());
        }
        repos = repos.into_iter().skip(args.start_at).collect();
        println!("Starting from repository #{}", args.start_at);
    }

    if let Some(max) = args.max_repos {
        repos.truncate(max);
        println!("Limited to {} repositories", repos.len());
    }

    if args.dry_run {
        println!("\n=== DRY RUN - Would clone {} repositories ===", repos.len());
        writeln!(log_file, "\nDRY RUN - Would clone {} repositories", repos.len())?;
        for (idx, repo) in repos.iter().enumerate() {
            println!("{:4}. {} ({})", idx + 1, repo.path, repo.url);
            writeln!(log_file, "{:4}. {} ({})", idx + 1, repo.path, repo.url)?;
        }
        return Ok(());
    }

    // Create output directory
    fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("Failed to create output directory: {}", args.output_dir.display()))?;

    println!("Cloning {} repositories with {} parallel jobs...\n", repos.len(), args.jobs);

    // Counters
    let total = repos.len();
    let cloned = Arc::new(AtomicUsize::new(0));
    let skipped = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));

    // Clone repositories using simple thread pool
    let chunk_size = (repos.len() + args.jobs - 1) / args.jobs;
    let mut handles = Vec::new();

    for (thread_idx, chunk) in repos.chunks(chunk_size).enumerate() {
        let chunk = chunk.to_vec();
        let output_dir = args.output_dir.clone();
        let shallow = args.shallow;
        let skip_existing = args.skip_existing;
        let cloned = Arc::clone(&cloned);
        let skipped = Arc::clone(&skipped);
        let failed = Arc::clone(&failed);

        let handle = std::thread::spawn(move || {
            for repo in chunk {
                match clone_repo(&repo, &output_dir, shallow, skip_existing) {
                    Ok(true) => {
                        let count = cloned.fetch_add(1, Ordering::SeqCst) + 1;
                        println!("[T{}] ✓ Cloned {}/{}: {}", thread_idx, count, total, repo.path);
                    }
                    Ok(false) => {
                        let count = skipped.fetch_add(1, Ordering::SeqCst) + 1;
                        println!("[T{}] ⊘ Skipped {}: {} (already exists)", thread_idx, count, repo.path);
                    }
                    Err(e) => {
                        let count = failed.fetch_add(1, Ordering::SeqCst) + 1;
                        eprintln!("[T{}] ✗ Failed {}: {} - {}", thread_idx, count, repo.path, e);
                    }
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    let elapsed = start_time.elapsed();
    let cloned_count = cloned.load(Ordering::SeqCst);
    let skipped_count = skipped.load(Ordering::SeqCst);
    let failed_count = failed.load(Ordering::SeqCst);

    println!("\n=== Summary ===");
    println!("Total repositories: {}", total);
    println!("  Cloned:           {}", cloned_count);
    println!("  Skipped:          {}", skipped_count);
    println!("  Failed:           {}", failed_count);
    println!("Completed in {} ms.", elapsed.as_millis());

    writeln!(log_file, "\n=== Summary ===")?;
    writeln!(log_file, "Total repositories: {}", total)?;
    writeln!(log_file, "  Cloned:           {}", cloned_count)?;
    writeln!(log_file, "  Skipped:          {}", skipped_count)?;
    writeln!(log_file, "  Failed:           {}", failed_count)?;
    writeln!(log_file, "Completed in {} ms.", elapsed.as_millis())?;

    println!("\nLog written to: {}", log_file_path.display());

    Ok(())
}

