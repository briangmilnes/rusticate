// Copyright (C) Brian G. Milnes 2025

//! Common test utilities for integration tests

use anyhow::{Result, Context};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct TestContext {
    pub apas_path: PathBuf,
    pub expected_commit: String,
}

impl TestContext {
    /// Ensure APAS-AI-copy is checked out at the commit specified in the Python script
    pub fn ensure_apas_at_script_commit(script_path: &str) -> Result<Self> {
        let script_full_path = PathBuf::from("scripts").join(script_path);
        let expected_commit = Self::extract_commit_from_script(&script_full_path)?;
        let apas_path = PathBuf::from("APAS-AI-copy/apas-ai");
        
        // Get current commit
        let current_commit = Self::get_current_commit(&apas_path)?;
        
        // Checkout if needed
        if current_commit != expected_commit {
            eprintln!("Checking out APAS to commit {expected_commit}");
            Self::checkout_commit(&apas_path, &expected_commit)?;
        }
        
        Ok(TestContext {
            apas_path,
            expected_commit,
        })
    }
    
    /// Extract git commit from Python script comment
    fn extract_commit_from_script(path: &Path) -> Result<String> {
        let content = std::fs::read_to_string(path)
            .context(format!("Failed to read script: {}", path.display()))?;
        
        for line in content.lines() {
            if line.starts_with("# Git commit:") {
                let commit = line.split(':').nth(1)
                    .context("Invalid commit line format")?
                    .trim()
                    .to_string();
                return Ok(commit);
            }
        }
        
        Err(anyhow::anyhow!("No git commit found in script: {}", path.display()))
    }
    
    /// Get current commit of a git repository
    fn get_current_commit(repo: &Path) -> Result<String> {
        let output = Command::new("git")
            .args(["-C", repo.to_str().unwrap(), "rev-parse", "HEAD"])
            .output()
            .context("Failed to run git rev-parse")?;
        
        if !output.status.success() {
            return Err(anyhow::anyhow!("git rev-parse failed"));
        }
        
        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }
    
    /// Checkout a specific commit
    fn checkout_commit(repo: &Path, commit: &str) -> Result<()> {
        let status = Command::new("git")
            .args(["-C", repo.to_str().unwrap(), "checkout", commit])
            .status()
            .context("Failed to run git checkout")?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("git checkout failed for commit {commit}"));
        }
        
        Ok(())
    }
}

/// Parse a number with possible commas (e.g., "1,234" -> 1234)
pub fn parse_number(s: &str) -> Result<usize> {
    let cleaned = s.replace(",", "");
    cleaned.parse::<usize>()
        .context(format!("Failed to parse number: {s}"))
}

