// Copyright (C) Brian G. Milnes 2025

#[cfg(test)]
mod tests {
    use std::process::Command;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_review_helper_inherent_impls_on_apas() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let binary_path = format!("{}/target/release/rusticate-review-helper-inherent-impls", manifest_dir);
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

        // Should find violations
        assert!(stdout.contains("ONLY PRIVATE HELPERS"), "Expected to find private helper impls");
        assert!(stdout.contains("SUMMARY"), "Expected summary section");
        
        // Parse the summary numbers
        let only_private = stdout.lines()
            .find(|l| l.contains("Only private helpers (ELIMINATE):"))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|s| s.trim().parse::<usize>().ok())
            .expect("Expected to parse 'only private' count");
        
        // Should find at least 100 violations (as of commit e06fb8d)
        assert!(only_private >= 100, "Expected at least 100 private helper impls, got {}", only_private);
        
        // Should exit with code 1 (violations found)
        assert_eq!(output.status.code(), Some(1), "Expected exit code 1 for violations");
    }
}

