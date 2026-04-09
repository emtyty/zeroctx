use crate::config::Config;
use crate::core::types::FilterResult;
use crate::filters::OutputFilter;

pub struct RustFilter;

impl OutputFilter for RustFilter {
    fn name(&self) -> &str {
        "rust"
    }

    fn matches(&self, command: &str) -> bool {
        command.starts_with("cargo ")
    }

    fn filter(&self, output: &str, _config: &Config) -> FilterResult {
        let original_lines = output.lines().count();

        let filtered = if output.contains("test result:") {
            filter_cargo_test(output)
        } else if output.contains("warning[") || output.contains("error[") {
            filter_cargo_build(output)
        } else {
            strip_noise(output)
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

/// Cargo test: extract FAILED + panic message, skip passing, keep summary
fn filter_cargo_test(output: &str) -> String {
    let mut result = Vec::new();
    let mut in_failure = false;
    let mut failure_context = 0;
    let has_failures = output.contains("FAILED") || output.contains("panicked");

    for line in output.lines() {
        let trimmed = line.trim();

        // Summary lines (always keep)
        if trimmed.starts_with("test result:") || trimmed.starts_with("running ") {
            result.push(line.to_string());
            continue;
        }

        // If all tests pass, just show summary
        if !has_failures && trimmed.starts_with("test ") && trimmed.contains("... ok") {
            continue; // Skip passing tests
        }

        // Failed test line
        if trimmed.starts_with("test ") && trimmed.contains("FAILED") {
            result.push(line.to_string());
            continue;
        }

        // Panic message
        if trimmed.contains("panicked at") || trimmed.starts_with("thread '") {
            in_failure = true;
            failure_context = 0;
            result.push(line.to_string());
            continue;
        }

        // Failure context (a few lines after panic)
        if in_failure {
            failure_context += 1;
            if failure_context <= 5 {
                result.push(line.to_string());
            }
            if trimmed.is_empty() || failure_context > 5 {
                in_failure = false;
            }
            continue;
        }

        // "failures:" section
        if trimmed == "failures:" || trimmed == "failures:" {
            result.push(line.to_string());
            continue;
        }
    }

    if result.is_empty() {
        strip_noise(output)
    } else {
        result.join("\n")
    }
}

/// Cargo build/clippy: errors + warnings only, strip Compiling noise
fn filter_cargo_build(output: &str) -> String {
    use std::collections::HashMap;

    let mut errors = Vec::new();
    let mut warnings_by_lint: HashMap<String, (usize, String)> = HashMap::new();
    let mut summary = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Always skip noise
        if trimmed.starts_with("Compiling ")
            || trimmed.starts_with("Downloading ")
            || trimmed.starts_with("Downloaded ")
            || trimmed.starts_with("Updating ")
            || trimmed.starts_with("Blocking ")
        {
            continue;
        }

        // Error lines (always keep)
        if trimmed.starts_with("error") {
            errors.push(line.to_string());
            continue;
        }

        // Warning lines — group by lint name for clippy
        if trimmed.starts_with("warning:") || trimmed.starts_with("warning[") {
            let lint = if let Some(start) = trimmed.find('[') {
                if let Some(end) = trimmed.find(']') {
                    trimmed[start + 1..end].to_string()
                } else {
                    "misc".to_string()
                }
            } else {
                "misc".to_string()
            };
            let entry = warnings_by_lint.entry(lint).or_insert_with(|| (0, line.to_string()));
            entry.0 += 1;
            continue;
        }

        // Summary lines
        if trimmed.starts_with("Finished ")
            || trimmed.contains("error(s)")
            || trimmed.contains("warning(s)")
            || trimmed.starts_with("For more information")
        {
            summary.push(line.to_string());
        }
    }

    let mut result = Vec::new();

    // Errors first
    for e in &errors {
        result.push(e.clone());
    }

    // Grouped warnings
    let mut warns: Vec<(String, (usize, String))> = warnings_by_lint.into_iter().collect();
    warns.sort_by(|a, b| b.1.0.cmp(&a.1.0));
    for (lint, (count, example)) in &warns {
        if *count == 1 {
            result.push(example.clone());
        } else {
            result.push(format!("warning[{}] (x{})", lint, count));
        }
    }

    // Summary
    for s in &summary {
        result.push(s.clone());
    }

    if result.is_empty() {
        strip_noise(output)
    } else {
        result.join("\n")
    }
}

fn strip_noise(output: &str) -> String {
    output
        .lines()
        .filter(|line| {
            let t = line.trim();
            !t.starts_with("Compiling ")
                && !t.starts_with("Downloading ")
                && !t.starts_with("Downloaded ")
                && !t.starts_with("Updating ")
                && !t.starts_with("Blocking ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
