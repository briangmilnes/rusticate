// Copyright (C) Brian G. Milnes 2025

#[cfg(test)]
mod tests {
    use std::process::Command;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_review_inherent_and_trait_impl_on_apas() {
        // Get project root
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let binary_path = format!("{}/target/release/rusticate-review-inherent-and-trait-impl", manifest_dir);
        let test_dir = format!("{}/APAS-AI-copy/apas-ai", manifest_dir);
        
        let output = Command::new(&binary_path)
            .arg("-c")
            .current_dir(&test_dir)
            .output()
            .expect("Failed to execute command");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stderr.is_empty() {
            println!("STDERR: {}", stderr);
        }
        println!("STDOUT: {}", stdout);

        // Should find no issues (APAS-AI follows single implementation pattern)
        assert!(stdout.contains("✓ No issues found"), "Expected no issues, got: {}", stdout);
        assert_eq!(output.status.code(), Some(0), "Expected exit code 0");
    }
}

