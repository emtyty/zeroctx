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

/// Compress Glob tool results: group file paths by directory prefix.
///
/// 200 individual paths → grouped summary with counts per directory.
pub fn compress_glob_results(output: &str) -> String {
    let paths: Vec<&str> = output.lines().filter(|l| !l.is_empty()).collect();
    if paths.len() <= 20 {
        return output.to_string(); // Small result: return as-is
    }

    // Group by parent directory
    let mut dirs: std::collections::BTreeMap<String, Vec<&str>> = std::collections::BTreeMap::new();
    for path in &paths {
        let parent = std::path::Path::new(path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".");
        dirs.entry(parent.to_string()).or_default().push(path);
    }

    let mut result = Vec::new();
    result.push(format!("// {} files matched:", paths.len()));

    const MAX_DIRS: usize = 10;
    const MAX_FILES_PER_DIR: usize = 5;

    for (dir, files) in dirs.iter().take(MAX_DIRS) {
        if files.len() == 1 {
            result.push(format!("  {}", files[0]));
        } else {
            // Show directory with file count and first few filenames
            let names: Vec<&str> = files
                .iter()
                .take(MAX_FILES_PER_DIR)
                .map(|p| std::path::Path::new(p).file_name().and_then(|n| n.to_str()).unwrap_or(p))
                .collect();
            let suffix = if files.len() > MAX_FILES_PER_DIR {
                format!(", +{} more", files.len() - MAX_FILES_PER_DIR)
            } else {
                String::new()
            };
            result.push(format!("  {}/ ({} files: {}{})", dir, files.len(), names.join(", "), suffix));
        }
    }

    if dirs.len() > MAX_DIRS {
        result.push(format!("  ... ({} more directories)", dirs.len() - MAX_DIRS));
    }

    result.join("\n")
}

/// Compress Grep tool results: group matches by file with line previews.
///
/// 500 grep matches → file-level summary with first match per file.
pub fn compress_grep_results(output: &str) -> String {
    let lines: Vec<&str> = output.lines().filter(|l| !l.is_empty()).collect();
    if lines.len() <= 15 {
        return output.to_string(); // Small result: return as-is
    }

    // Group matches by file (ripgrep format: "path:line:content" or "path:content")
    let mut file_matches: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut unmatched_lines: Vec<&str> = Vec::new();

    for line in &lines {
        // Try to parse as "filepath:line_num:content" or "filepath:content"
        if let Some((file, rest)) = split_grep_line(line) {
            file_matches.entry(file).or_default().push(rest);
        } else {
            unmatched_lines.push(line);
        }
    }

    if file_matches.is_empty() {
        // Can't parse — fall back to line-limit truncation
        let mut result: Vec<&str> = lines.iter().take(30).cloned().collect();
        if lines.len() > 30 {
            result.push("... (output truncated, run command directly for full results)");
        }
        return result.join("\n");
    }

    let total_matches: usize = file_matches.values().map(|v| v.len()).sum();
    let mut result = Vec::new();
    result.push(format!("// {} matches across {} files:", total_matches, file_matches.len()));

    const MAX_FILES: usize = 15;
    const MAX_LINES_PER_FILE: usize = 3;

    for (file, matches) in file_matches.iter().take(MAX_FILES) {
        result.push(format!("  {} ({} match{})", file, matches.len(), if matches.len() == 1 { "" } else { "es" }));
        for m in matches.iter().take(MAX_LINES_PER_FILE) {
            let preview = m.trim();
            let truncated = if preview.len() > 120 {
                format!("{}...", &preview[..120])
            } else {
                preview.to_string()
            };
            result.push(format!("    > {}", truncated));
        }
        if matches.len() > MAX_LINES_PER_FILE {
            result.push(format!("    ... ({} more matches)", matches.len() - MAX_LINES_PER_FILE));
        }
    }

    if file_matches.len() > MAX_FILES {
        result.push(format!("  ... ({} more files)", file_matches.len() - MAX_FILES));
    }

    result.join("\n")
}

fn split_grep_line(line: &str) -> Option<(String, String)> {
    // Detect ripgrep-style: "path:linenum:content" (two colons) or "path:content" (one colon)
    // Avoid splitting Windows paths "C:\..."
    let mut colon_count = 0;
    for (i, c) in line.char_indices() {
        if c == ':' {
            // Skip Windows drive letter prefix like "C:"
            if i == 1 && line.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) {
                continue;
            }
            colon_count += 1;
            if colon_count == 1 {
                let file = line[..i].to_string();
                let rest = line[i + 1..].to_string();
                // Validate that "file" looks like a file path (has extension or /)
                if file.contains('.') || file.contains('/') || file.contains('\\') {
                    return Some((file, rest));
                }
            }
        }
    }
    None
}
