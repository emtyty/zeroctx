use crate::config::Config;
use crate::core::types::FilterResult;
use crate::filters::OutputFilter;

pub struct JavaScriptFilter;

impl OutputFilter for JavaScriptFilter {
    fn name(&self) -> &str {
        "javascript"
    }

    fn matches(&self, command: &str) -> bool {
        let first = command.split_whitespace().next().unwrap_or("");
        matches!(
            first,
            "npm" | "npx" | "pnpm" | "yarn" | "node" | "tsc" | "eslint"
                | "biome" | "jest" | "vitest" | "prettier" | "next"
                | "playwright" | "prisma"
        )
    }

    fn filter(&self, output: &str, _config: &Config) -> FilterResult {
        let original_lines = output.lines().count();

        let filtered = if output.contains("Tests:") || output.contains("Test Suites:") || output.contains("FAIL ") || output.contains("PASS ") {
            filter_test_runner(output)
        } else if output.contains("error TS") || output.contains("error ts") {
            filter_tsc(output)
        } else if output.contains("problems (") || output.contains("errors and") || output.contains("warning") {
            filter_linter(output)
        } else {
            strip_npm_noise(output)
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

/// Jest/Vitest: extract FAIL suites + first assertion error, skip PASS
fn filter_test_runner(output: &str) -> String {
    let mut result = Vec::new();
    let mut in_failure = false;
    let mut failure_lines = 0;
    let max_failure_lines = 10;
    let has_failures = output.contains("FAIL ") || output.contains("FAILED");

    for line in output.lines() {
        let trimmed = line.trim();

        // Summary lines (always keep)
        if trimmed.starts_with("Tests:") || trimmed.starts_with("Test Suites:") || trimmed.starts_with("Time:") || trimmed.starts_with("Ran all") {
            result.push(line.to_string());
            continue;
        }

        // PASS lines — skip if there are failures
        if trimmed.starts_with("PASS ") && has_failures {
            continue;
        }

        // FAIL suite
        if trimmed.starts_with("FAIL ") {
            result.push(line.to_string());
            in_failure = true;
            failure_lines = 0;
            continue;
        }

        // Failure assertion lines
        if trimmed.starts_with("●") || trimmed.starts_with("✕") || trimmed.starts_with("✗") || trimmed.starts_with("×") {
            result.push(line.to_string());
            in_failure = true;
            failure_lines = 0;
            continue;
        }

        if in_failure {
            failure_lines += 1;
            if failure_lines <= max_failure_lines {
                result.push(line.to_string());
            }
            if trimmed.is_empty() || failure_lines > max_failure_lines {
                if failure_lines > max_failure_lines {
                    result.push("  ...".to_string());
                }
                in_failure = false;
            }
        }
    }

    if result.is_empty() {
        strip_npm_noise(output)
    } else {
        result.join("\n")
    }
}

/// TSC: deduplicate type errors by error code
fn filter_tsc(output: &str) -> String {
    use std::collections::HashMap;

    let mut by_code: HashMap<String, (usize, String)> = HashMap::new();
    let mut other = Vec::new();

    for line in output.lines() {
        // TSC format: file.ts(10,5): error TS2345: ...
        if let Some(ts_pos) = line.find("error TS") {
            let code_and_rest = &line[ts_pos..];
            let code = code_and_rest.split(':').next().unwrap_or("").trim();
            let entry = by_code.entry(code.to_string()).or_insert_with(|| (0, line.to_string()));
            entry.0 += 1;
        } else if line.contains("Found ") && line.contains("error") {
            other.push(line.to_string()); // Summary
        }
    }

    let mut result = Vec::new();
    let mut codes: Vec<(String, (usize, String))> = by_code.into_iter().collect();
    codes.sort_by(|a, b| b.1.0.cmp(&a.1.0));

    for (code, (count, example)) in &codes {
        if *count == 1 {
            result.push(example.clone());
        } else {
            result.push(format!("{} (x{}): {}", code, count, example));
        }
    }
    result.extend(other);

    if result.is_empty() {
        output.to_string()
    } else {
        result.join("\n")
    }
}

/// ESLint/Biome: group by rule
fn filter_linter(output: &str) -> String {
    use std::collections::HashMap;

    let mut by_rule: HashMap<String, usize> = HashMap::new();
    let mut errors = Vec::new();
    let mut summary = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Lines with rule references
        if trimmed.contains("error ") || trimmed.contains("warning ") {
            // Try to extract rule name (last word or word in parens)
            if let Some(paren_start) = trimmed.rfind('(') {
                if let Some(paren_end) = trimmed.rfind(')') {
                    let rule = &trimmed[paren_start + 1..paren_end];
                    *by_rule.entry(rule.to_string()).or_insert(0) += 1;
                    continue;
                }
            }
            errors.push(line.to_string());
        } else if trimmed.contains("problems") || trimmed.contains("errors and") {
            summary.push(line.to_string());
        }
    }

    let mut result = Vec::new();
    let mut rules: Vec<(String, usize)> = by_rule.into_iter().collect();
    rules.sort_by(|a, b| b.1.cmp(&a.1));

    for (rule, count) in &rules {
        result.push(format!("  {} (x{})", rule, count));
    }
    result.extend(errors);
    result.extend(summary);

    if result.is_empty() {
        output.to_string()
    } else {
        result.join("\n")
    }
}

fn strip_npm_noise(output: &str) -> String {
    output
        .lines()
        .filter(|line| {
            let t = line.trim();
            !t.starts_with("npm WARN")
                && !t.starts_with("npm notice")
                && !t.contains("added ")
                && !t.contains("up to date")
                && !t.starts_with("⸩")
                && !t.starts_with("⸨")
                && !t.is_empty()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
