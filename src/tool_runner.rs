// Copyright (C) Brian G. Milnes 2025

//! Tool runner infrastructure for rusticate binaries
//! 
//! Provides a consistent wrapper for all rusticate tools that handles:
//! - Timing measurement
//! - Directory context for Emacs compile-mode
//! - Optional logging to files
//! - Standard error handling

pub mod tool_runner {
    use std::time::Instant;
    use std::path::PathBuf;
    use anyhow::Result;
    use crate::logging::logging::ToolLogger;

    /// Configuration for a tool run
    pub struct ToolConfig {
        /// Name of the tool (for logging directory)
        pub tool_name: String,
        /// Base directory to display in "Entering directory"
        pub base_dir: PathBuf,
        /// Whether to enable file logging
        pub enable_logging: bool,
    }

    impl ToolConfig {
        /// Create a basic config with just tool name and base directory
        pub fn new(tool_name: &str, base_dir: PathBuf) -> Self {
            ToolConfig {
                tool_name: tool_name.to_string(),
                base_dir,
                enable_logging: false, // Disabled by default for now
            }
        }
    }

    /// Run a tool with standard timing, context, and optional logging
    /// 
    /// Usage:
    /// ```no_run
    /// let config = ToolConfig::new("review-string-hacking", base_dir);
    /// run_tool(config, |logger| {
    ///     logger.log("Starting analysis...");
    ///     // Your tool logic here
    ///     Ok("Summary: X files checked".to_string())
    /// })?;
    /// ```
    pub fn run_tool<F>(config: ToolConfig, tool_fn: F) -> Result<()>
    where
        F: FnOnce(&mut ToolLogger) -> Result<String>,
    {
        let start = Instant::now();
        
        // Print directory context (for Emacs compile-mode)
        println!("Entering directory '{}'", config.base_dir.display());
        println!();
        
        // Create logger (may be no-op if logging disabled)
        let mut logger = if config.enable_logging {
            ToolLogger::new(&config.tool_name)
        } else {
            // Create a stub logger that doesn't write to files
            ToolLogger::new_disabled()
        };
        
        // Run the tool logic
        let summary = tool_fn(&mut logger)?;
        
        // Print timing
        println!();
        println!("{summary}");
        println!("Completed in {}ms", start.elapsed().as_millis());
        
        // Finalize logger if enabled
        if config.enable_logging {
            logger.finalize(&summary);
        }
        
        Ok(())
    }

    /// Simple runner without logging support (just timing and context)
    /// 
    /// For tools that don't need logging yet.
    pub fn run_simple<F>(_tool_name: &str, base_dir: PathBuf, tool_fn: F) -> Result<()>
    where
        F: FnOnce() -> Result<String>,
    {
        let start = Instant::now();
        
        // Print directory context
        println!("Entering directory '{}'", base_dir.display());
        println!();
        
        // Run the tool logic
        let summary = tool_fn()?;
        
        // Print timing
        println!();
        println!("{summary}");
        println!("Completed in {}ms", start.elapsed().as_millis());
        
        Ok(())
    }
}

