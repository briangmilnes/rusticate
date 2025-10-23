//! Rusticate: Dispatcher for review tools
//!
//! Usage: rusticate <review-name> [args...]
//!
//! Examples:
//!   rusticate redundant-inherent-impls -c
//!   rusticate stt-compliance -c
//!   rusticate string-hacking -f src/lib.rs
//!
//! Available reviews:
//!   - comment-placement
//!   - duplicate-methods
//!   - impl-trait-bounds
//!   - inherent-and-trait-impl
//!   - inherent-plus-trait-impl
//!   - internal-method-impls
//!   - no-trait-method-duplication
//!   - non-wildcard-uses
//!   - public-only-inherent-impls
//!   - qualified-paths
//!   - redundant-inherent-impls
//!   - single-trait-impl
//!   - stt-compliance
//!   - string-hacking
//!   - stub-delegation
//!   - trait-bound-mismatches
//!   - trait-definition-order
//!   - trait-method-conflicts
//!   - trait-self-usage
//!   - typeclasses

use std::env;
use std::process::{Command, exit};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: rusticate <review-name> [args...]");
        eprintln!();
        eprintln!("Available reviews:");
        eprintln!("  comment-placement              - Check comment placement in AST");
        eprintln!("  duplicate-methods              - Find duplicate method names");
        eprintln!("  impl-trait-bounds              - Show inherent impls with generics");
        eprintln!("  inherent-and-trait-impl        - Check inherent+trait impl patterns");
        eprintln!("  inherent-plus-trait-impl       - Find structs with both impl types");
        eprintln!("  internal-method-impls          - Find internal method patterns");
        eprintln!("  no-trait-method-duplication    - Check trait method duplication");
        eprintln!("  non-wildcard-uses              - Find non-wildcard imports");
        eprintln!("  public-only-inherent-impls     - Check public-only inherent impls");
        eprintln!("  qualified-paths                - Find long qualified paths");
        eprintln!("  redundant-inherent-impls       - Find redundant inherent impls");
        eprintln!("  single-trait-impl              - Check single trait impl pattern");
        eprintln!("  stt-compliance                 - Check StT trait requirements");
        eprintln!("  string-hacking                 - Find string manipulation code");
        eprintln!("  stub-delegation                - Find stub delegation patterns");
        eprintln!("  trait-bound-mismatches         - Check trait bound consistency");
        eprintln!("  trait-definition-order         - Check trait definition ordering");
        eprintln!("  trait-method-conflicts         - Find trait method name conflicts");
        eprintln!("  trait-self-usage               - Check Self usage in traits");
        eprintln!("  typeclasses                    - Check typeclass patterns");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  rusticate redundant-inherent-impls -c");
        eprintln!("  rusticate stt-compliance -c");
        eprintln!("  rusticate string-hacking -f src/lib.rs");
        exit(1);
    }

    let review_name = &args[1];
    let binary_name = format!("rusticate-review-{}", review_name);

    // Get the directory where this binary is located
    let current_exe = env::current_exe().unwrap_or_else(|_| {
        eprintln!("Error: Could not determine current executable path");
        exit(1);
    });

    let bin_dir = current_exe.parent().unwrap_or_else(|| {
        eprintln!("Error: Could not determine binary directory");
        exit(1);
    });

    let target_binary = bin_dir.join(&binary_name);

    // Check if the target binary exists
    if !target_binary.exists() {
        eprintln!("Error: Review tool '{}' not found", review_name);
        eprintln!("Expected binary: {}", target_binary.display());
        eprintln!();
        eprintln!("Run 'rusticate' without arguments to see available reviews.");
        exit(1);
    }

    // Execute the target binary with remaining arguments
    let remaining_args = &args[2..];
    
    let status = Command::new(&target_binary)
        .args(remaining_args)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Error executing {}: {}", binary_name, e);
            exit(1);
        });

    // Exit with the same code as the child process
    exit(status.code().unwrap_or(1));
}

