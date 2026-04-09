use once_cell::sync::Lazy;
use regex::Regex;

use crate::config::Config;
use crate::core::types::FilterResult;
use crate::filters::OutputFilter;

pub struct SystemFilter;

impl OutputFilter for SystemFilter {
    fn name(&self) -> &str {
        "system"
    }

    fn matches(&self, command: &str) -> bool {
        let cmd = command.split_whitespace().next().unwrap_or("");
        matches!(
            cmd,
            "ls" | "dir" | "tree" | "cat" | "head" | "tail" | "grep"
                | "rg" | "find" | "fd" | "wc" | "env" | "printenv"
        )
    }

    fn filter(&self, output: &str, config: &Config) -> FilterResult {
        let original_lines = output.lines().count();

        let filtered = if output.lines().any(|l| l.contains('=') && !l.contains(' ')) {
            // Looks like env output (KEY=VALUE lines)
            filter_env(output)
        } else {
            // Generic truncation with grouping
            truncate_grouped(output, config.limits.grep_max_results)
        };

        let filtered_lines = filtered.lines().count();
        let savings = if original_lines > 0 {
            (1.0 - filtered_lines as f64 / original_lines as f64) * 100.0
        } else {
            0.0
        };

        FilterResult {
            output: filtered,
            original_lines,
            filtered_lines,
            savings_percent: savings,
        }
    }
}

/// Env: redact sensitive values
fn filter_env(output: &str) -> String {
    static SECRET_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)(KEY|TOKEN|SECRET|PASSWORD|PASS|API|AUTH|CREDENTIAL|PRIVATE)").expect("valid regex")
    });

    let mut result = Vec::new();
    for line in output.lines() {
        if let Some(eq_pos) = line.find('=') {
            let key = &line[..eq_pos];
            let value = &line[eq_pos + 1..];
            if SECRET_RE.is_match(key) && !value.is_empty() {
                result.push(format!("{}=***REDACTED***", key));
            } else {
                result.push(line.to_string());
            }
        } else {
            result.push(line.to_string());
        }
    }
    result.join("\n")
}

/// Truncate with count of remaining lines
fn truncate_grouped(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= max_lines {
        return output.to_string();
    }

    // Collapse repeated identical lines
    let mut result = Vec::new();
    let mut prev: Option<&str> = None;
    let mut repeat_count = 0;

    for line in &lines {
        if Some(*line) == prev {
            repeat_count += 1;
        } else {
            if repeat_count > 1 {
                result.push(format!("  (repeated x{})", repeat_count));
            }
            if result.len() >= max_lines {
                result.push(format!("... ({} more lines)", lines.len() - result.len()));
                break;
            }
            result.push(line.to_string());
            repeat_count = 1;
        }
        prev = Some(line);
    }

    if repeat_count > 1 {
        result.push(format!("  (repeated x{})", repeat_count));
    }

    result.join("\n")
}
