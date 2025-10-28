// Copyright (C) Brian G. Milnes 2025

//! Logging infrastructure for rusticate tools
//! 
//! Provides consistent logging to files organized by tool and date:
//! - logs/<tool-name>/<date>/run-<timestamp>.log
//! 
//! Each tool gets its own directory, with subdirectories by date.
//! Multiple runs on the same day create timestamped log files.

pub mod logging {
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use chrono::{Local, DateTime};
    use anyhow::Result;

    /// Logger for a rusticate tool
    pub struct ToolLogger {
        log_file: Option<fs::File>,
        log_path: Option<PathBuf>,
        _tool_name: String,
        start_time: DateTime<Local>,
    }

    impl ToolLogger {
        /// Create a disabled logger (no file output)
        /// 
        /// Used when logging is not desired but ToolLogger interface is needed
        pub fn new_disabled() -> Self {
            ToolLogger {
                log_file: None,
                log_path: None,
                _tool_name: String::new(),
                start_time: Local::now(),
            }
        }

        /// Create a new logger for a tool
        /// 
        /// Creates log directory structure: logs/<tool-name>/<YYYY-MM-DD>/run-<HH-MM-SS>.log
        /// If log creation fails, continues without logging (degrades gracefully)
        pub fn new(tool_name: &str) -> Self {
            let start_time = Local::now();
            
            // Try to create log file
            let (log_file, log_path) = match Self::create_log_file(tool_name, &start_time) {
                Ok((file, path)) => (Some(file), Some(path)),
                Err(e) => {
                    eprintln!("Warning: Could not create log file: {e}");
                    eprintln!("Continuing without logging...");
                    (None, None)
                }
            };

            ToolLogger {
                log_file,
                log_path,
                _tool_name: tool_name.to_string(),
                start_time,
            }
        }

        /// Create the log file and directory structure
        fn create_log_file(tool_name: &str, start_time: &DateTime<Local>) -> Result<(fs::File, PathBuf)> {
            // Create directory structure: logs/<tool-name>/<YYYY-MM-DD>
            let date_str = start_time.format("%Y-%m-%d").to_string();
            let time_str = start_time.format("%H-%M-%S").to_string();
            
            let log_dir = PathBuf::from("logs")
                .join(tool_name)
                .join(&date_str);
            
            fs::create_dir_all(&log_dir)?;
            
            // Create log file: run-<HH-MM-SS>.log
            let log_path = log_dir.join(format!("run-{time_str}.log"));
            let log_file = fs::File::create(&log_path)?;
            
            Ok((log_file, log_path))
        }

        /// Log a message to both stdout and the log file
        pub fn log(&mut self, message: &str) {
            // Always print to stdout
            println!("{message}");
            
            // Write to log file if available
            if let Some(ref mut file) = self.log_file {
                let _ = writeln!(file, "{message}");
            }
        }

        /// Log without printing to stdout (log file only)
        pub fn log_silent(&mut self, message: &str) {
            if let Some(ref mut file) = self.log_file {
                let _ = writeln!(file, "{message}");
            }
        }

        /// Get the path to the log file (if logging is enabled)
        pub fn log_path(&self) -> Option<&Path> {
            self.log_path.as_deref()
        }

        /// Finalize the log with summary information
        pub fn finalize(&mut self, summary: &str) {
            let end_time = Local::now();
            let duration = end_time.signed_duration_since(self.start_time);
            
            self.log("");
            self.log("=== Run Summary ===");
            self.log(summary);
            self.log(&format!("Started: {}", self.start_time.format("%Y-%m-%d %H:%M:%S")));
            self.log(&format!("Ended: {}", end_time.format("%Y-%m-%d %H:%M:%S")));
            self.log(&format!("Duration: {}ms", duration.num_milliseconds()));
            
            if let Some(ref path) = self.log_path {
                self.log(&format!("Log saved to: {}", path.display()));
            }
        }
    }

    impl Drop for ToolLogger {
        fn drop(&mut self) {
            // Flush the log file on drop
            if let Some(ref mut file) = self.log_file {
                let _ = file.flush();
            }
        }
    }
}

