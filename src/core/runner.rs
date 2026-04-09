use anyhow::{Context, Result};
use std::process::Command;
use tracing::{debug, warn};

use crate::config::Config;

/// Maximum output size in bytes before truncation (configurable).
const DEFAULT_MAX_OUTPUT_SIZE: usize = 100 * 1024 * 1024; // 100MB

/// Result of executing a shell command.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    /// Whether output was truncated due to size limits
    pub truncated: bool,
}

/// Execute a shell command with memory limits and capture output.
///
/// Unlike RTK's runner which has no size limits (can OOM on large outputs),
/// this implementation caps output at a configurable maximum.
pub fn execute(cmd: &str, args: &[&str], config: &Config) -> Result<CommandOutput> {
    let max_size = config
        .limits
        .max_output_size_bytes
        .unwrap_or(DEFAULT_MAX_OUTPUT_SIZE);

    debug!(command = cmd, args = ?args, "Executing command");

    let output = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("Failed to execute: {} {}", cmd, args.join(" ")))?;

    let exit_code = output.status.code().unwrap_or(-1);

    let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let mut truncated = false;

    // Enforce memory limits (fixes RTK issue: toml_filter.rs:430 loads entire output)
    if stdout.len() > max_size {
        warn!(
            size = stdout.len(),
            max = max_size,
            "Truncating stdout (exceeded max output size)"
        );
        stdout.truncate(max_size);
        stdout.push_str("\n... [truncated: output exceeded size limit]");
        truncated = true;
    }
    if stderr.len() > max_size {
        warn!(
            size = stderr.len(),
            max = max_size,
            "Truncating stderr (exceeded max output size)"
        );
        stderr.truncate(max_size);
        stderr.push_str("\n... [truncated: output exceeded size limit]");
        truncated = true;
    }

    // Strip ANSI escape codes
    let stdout = strip_ansi(&stdout);
    let stderr = strip_ansi(&stderr);

    debug!(exit_code, stdout_len = stdout.len(), stderr_len = stderr.len(), "Command completed");

    Ok(CommandOutput {
        stdout,
        stderr,
        exit_code,
        truncated,
    })
}

/// Execute a raw shell command string (parsed by the shell).
pub fn execute_shell(command: &str, config: &Config) -> Result<CommandOutput> {
    let max_size = config
        .limits
        .max_output_size_bytes
        .unwrap_or(DEFAULT_MAX_OUTPUT_SIZE);

    debug!(command, "Executing shell command");

    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let flag = if cfg!(windows) { "/C" } else { "-c" };

    let output = Command::new(shell)
        .args([flag, command])
        .output()
        .with_context(|| format!("Failed to execute shell command: {}", command))?;

    let exit_code = output.status.code().unwrap_or(-1);

    let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let mut truncated = false;

    if stdout.len() > max_size {
        stdout.truncate(max_size);
        stdout.push_str("\n... [truncated]");
        truncated = true;
    }
    if stderr.len() > max_size {
        stderr.truncate(max_size);
        stderr.push_str("\n... [truncated]");
        truncated = true;
    }

    let stdout = strip_ansi(&stdout);
    let stderr = strip_ansi(&stderr);

    Ok(CommandOutput {
        stdout,
        stderr,
        exit_code,
        truncated,
    })
}

/// Strip ANSI escape codes from a string.
fn strip_ansi(s: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static ANSI_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]")
            .expect("ANSI regex is a valid compile-time constant")
    });

    ANSI_RE.replace_all(s, "").to_string()
}

/// Estimate token count from text (rough: 1 token ≈ 4 chars).
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[31mERROR\x1b[0m: something failed";
        assert_eq!(strip_ansi(input), "ERROR: something failed");
    }

    #[test]
    fn test_strip_ansi_no_codes() {
        let input = "plain text";
        assert_eq!(strip_ansi(input), "plain text");
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("hello"), 2); // 5 chars / 4 ≈ 2
        assert_eq!(estimate_tokens("hello world test"), 4); // 16 / 4 = 4
    }
}
