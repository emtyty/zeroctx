use crate::config::Config;
use crate::core::types::FilterResult;
use crate::filters::OutputFilter;

pub struct PythonFilter;

impl OutputFilter for PythonFilter {
    fn name(&self) -> &str {
        "python"
    }

    fn matches(&self, command: &str) -> bool {
        command.starts_with("pytest")
            || command.starts_with("python -m pytest")
            || command.starts_with("ruff ")
            || command.starts_with("mypy ")
            || command.starts_with("pip ")
    }

    fn filter(&self, output: &str, _config: &Config) -> FilterResult {
        let original_lines = output.lines().count();

        let filtered = if output.contains("FAILED") || output.contains("PASSED") || output.contains("passed") || output.contains("failed") {
            filter_pytest(output)
        } else if output.contains("ruff") || output.contains("Found ") {
            filter_ruff(output)
        } else if output.contains("error:") && output.contains("mypy") {
            filter_mypy(output)
        } else {
            output.to_string()
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

/// Pytest: extract FAILED tests + first error line + summary
fn filter_pytest(output: &str) -> String {
    let mut result = Vec::new();
    let mut in_failure = false;
    let mut failure_lines = 0;
    let max_failure_lines = 10;

    for line in output.lines() {
        let trimmed = line.trim();

        // Summary line (always keep)
        if trimmed.starts_with("=") && (trimmed.contains("passed") || trimmed.contains("failed") || trimmed.contains("error")) {
            result.push(line.to_string());
            continue;
        }

        // FAILED marker
        if trimmed.starts_with("FAILED ") || trimmed.starts_with("ERRORS") {
            in_failure = true;
            failure_lines = 0;
            result.push(line.to_string());
            continue;
        }

        // Failure section header
        if trimmed.starts_with("_") && trimmed.ends_with("_") && trimmed.len() > 10 {
            in_failure = true;
            failure_lines = 0;
            result.push(line.to_string());
            continue;
        }

        // Short test results line (e.g., "test_foo.py::test_bar PASSED")
        if trimmed.contains("PASSED") && !in_failure {
            continue; // Skip passing tests
        }

        // Error lines in failure section
        if in_failure {
            failure_lines += 1;
            if failure_lines <= max_failure_lines {
                result.push(line.to_string());
            } else if failure_lines == max_failure_lines + 1 {
                result.push("  ...".to_string());
            }
            // Reset at section breaks
            if trimmed.is_empty() || trimmed.starts_with("=") {
                in_failure = false;
            }
            continue;
        }

        // "E " assertion errors (always keep)
        if trimmed.starts_with("E ") || trimmed.starts_with("> ") {
            result.push(line.to_string());
            continue;
        }

        // "short test summary" header
        if trimmed.contains("short test summary") {
            result.push(line.to_string());
            continue;
        }
    }

    if result.is_empty() {
        output.to_string()
    } else {
        result.join("\n")
    }
}

/// Ruff: group by rule code, show count + first example
fn filter_ruff(output: &str) -> String {
    use std::collections::HashMap;

    let mut by_rule: HashMap<String, (usize, String)> = HashMap::new();
    let mut summary_lines = Vec::new();

    for line in output.lines() {
        // Ruff format: "file.py:10:5: E501 Line too long"
        if let Some(rule_start) = line.find(": ") {
            let after_colon = &line[rule_start + 2..];
            let rule_code = after_colon.split_whitespace().next().unwrap_or("");
            if rule_code.len() >= 2 && rule_code.len() <= 8 {
                let entry = by_rule.entry(rule_code.to_string()).or_insert_with(|| (0, line.to_string()));
                entry.0 += 1;
                continue;
            }
        }
        // Summary/header lines
        if line.contains("Found ") || line.contains("fixable") || !line.trim().is_empty() {
            summary_lines.push(line.to_string());
        }
    }

    let mut result = Vec::new();
    let mut rules: Vec<(String, (usize, String))> = by_rule.into_iter().collect();
    rules.sort_by(|a, b| b.1.0.cmp(&a.1.0)); // Most frequent first

    for (rule, (count, example)) in &rules {
        if *count == 1 {
            result.push(example.clone());
        } else {
            result.push(format!("{} (x{}): {}", rule, count, example));
        }
    }

    for sl in &summary_lines {
        result.push(sl.clone());
    }

    if result.is_empty() {
        output.to_string()
    } else {
        result.join("\n")
    }
}

/// Mypy: deduplicate repeated errors, group by file
fn filter_mypy(output: &str) -> String {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for line in output.lines() {
        // Mypy format: "file.py:10: error: ..."
        if line.contains("error:") || line.contains("note:") {
            // Deduplicate by error message (ignore file:line prefix)
            let msg_part = line.split("error:").nth(1).or_else(|| line.split("note:").nth(1)).unwrap_or(line);
            if seen.insert(msg_part.trim().to_string()) {
                result.push(line.to_string());
            }
        } else if line.contains("Found ") || line.starts_with("Success") {
            result.push(line.to_string());
        }
    }

    if result.is_empty() {
        output.to_string()
    } else {
        result.join("\n")
    }
}
