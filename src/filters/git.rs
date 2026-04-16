use once_cell::sync::Lazy;
use regex::Regex;

use crate::config::Config;
use crate::core::types::FilterResult;
use crate::filters::OutputFilter;

pub struct GitFilter;

impl OutputFilter for GitFilter {
    fn name(&self) -> &str {
        "git"
    }

    fn matches(&self, command: &str) -> bool {
        command.starts_with("git ") || command.starts_with("gh ")
    }

    fn filter(&self, output: &str, config: &Config) -> FilterResult {
        let original_lines = output.lines().count();

        let filtered = if output.is_empty() {
            return FilterResult::passthrough(output.to_string());
        } else {
            // Detect subcommand from output patterns
            if is_diff_output(output) {
                filter_diff(output, config)
            } else if is_show_stat_output(output) {
                filter_show_stat(output, config)
            } else if is_log_output(output) {
                filter_log(output, config)
            } else if is_status_output(output) {
                filter_status(output, config)
            } else {
                // Generic: truncate
                truncate_output(output, config.limits.git_status_max_files * 3)
            }
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

// --- Detection ---

fn is_diff_output(output: &str) -> bool {
    output.contains("diff --git") || output.contains("@@") || output.starts_with("---")
}

fn is_log_output(output: &str) -> bool {
    output.starts_with("commit ") || output.contains("\ncommit ")
}

fn is_show_stat_output(output: &str) -> bool {
    // git show --stat: has commit header AND file stat lines (file | N +/-)
    static STAT_LINE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?m)^\s+\S+.*\|\s+\d+").expect("valid regex"));
    is_log_output(output) && STAT_LINE_RE.is_match(output)
}

fn filter_show_stat(output: &str, config: &Config) -> String {
    // git show --stat: preserve commit info + file stat summary
    // Short outputs: pass through entirely; long outputs: truncate
    let max_lines = config.limits.git_status_max_files * 3;
    truncate_output(output, max_lines.max(50))
}

fn is_status_output(output: &str) -> bool {
    static STATUS_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?m)^[MADRCU\?\! ]{1,2} ").expect("valid regex"));
    STATUS_RE.is_match(output)
        || output.contains("On branch")
        || output.contains("Changes to be committed")
        || output.contains("Changes not staged")
        || output.contains("Untracked files")
}

// --- Git Diff Filter ---
// Strategy: show stat summary first, then compact hunks with line limits

fn filter_diff(output: &str, config: &Config) -> String {
    let max_hunk_lines = config.limits.git_diff_max_hunk_lines;
    let mut result = Vec::new();
    let mut current_file: Option<String> = None;
    let mut hunk_lines = 0;
    let mut total_additions = 0;
    let mut total_deletions = 0;
    let mut files_changed = 0;
    let mut in_hunk = false;
    let mut hunk_truncated = false;

    for line in output.lines() {
        if line.starts_with("diff --git") {
            // New file diff
            if hunk_truncated {
                result.push(format!("  ... ({} more lines in hunk)", hunk_lines - max_hunk_lines));
            }
            files_changed += 1;
            current_file = line.split(" b/").nth(1).map(|s| s.to_string());
            hunk_lines = 0;
            in_hunk = false;
            hunk_truncated = false;
            result.push(line.to_string());
        } else if line.starts_with("@@") {
            // Hunk header
            if hunk_truncated {
                result.push(format!("  ... ({} more lines)", hunk_lines - max_hunk_lines));
            }
            hunk_lines = 0;
            in_hunk = true;
            hunk_truncated = false;
            result.push(line.to_string());
        } else if line.starts_with("index ") || line.starts_with("--- ") || line.starts_with("+++ ") {
            // File headers — keep
            result.push(line.to_string());
        } else if in_hunk {
            hunk_lines += 1;
            if line.starts_with('+') && !line.starts_with("+++") {
                total_additions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                total_deletions += 1;
            }
            if hunk_lines <= max_hunk_lines {
                result.push(line.to_string());
            } else {
                hunk_truncated = true;
            }
        } else {
            result.push(line.to_string());
        }
    }

    if hunk_truncated {
        result.push(format!("  ... ({} more lines)", hunk_lines - max_hunk_lines));
    }

    // Prepend summary
    let summary = format!(
        "{} file(s) changed, {} insertions(+), {} deletions(-)",
        files_changed, total_additions, total_deletions
    );

    format!("{}\n\n{}", summary, result.join("\n"))
}

// --- Git Log Filter ---
// Strategy: one-line format per commit, limit total entries

fn filter_log(output: &str, _config: &Config) -> String {
    static COMMIT_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^commit ([a-f0-9]{7,40})").expect("valid regex"));
    static AUTHOR_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^Author:\s+(.+)").expect("valid regex"));
    static DATE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^Date:\s+(.+)").expect("valid regex"));

    let max_entries = 20;
    let mut entries = Vec::new();
    let mut current_hash = String::new();
    let mut current_author = String::new();
    let mut current_date = String::new();
    let mut current_message = String::new();

    for line in output.lines() {
        if let Some(caps) = COMMIT_RE.captures(line) {
            // Save previous entry
            if !current_hash.is_empty() {
                entries.push(format!(
                    "{} {} ({}, {})",
                    &current_hash[..7.min(current_hash.len())],
                    current_message.trim(),
                    current_author.split('<').next().unwrap_or("").trim(),
                    current_date.trim(),
                ));
            }
            current_hash = caps.get(1).map_or("", |m| m.as_str()).to_string();
            current_author.clear();
            current_date.clear();
            current_message.clear();
        } else if let Some(caps) = AUTHOR_RE.captures(line) {
            current_author = caps.get(1).map_or("", |m| m.as_str()).to_string();
        } else if let Some(caps) = DATE_RE.captures(line) {
            current_date = caps.get(1).map_or("", |m| m.as_str()).to_string();
        } else if !line.trim().is_empty() && !current_hash.is_empty() && current_message.is_empty() {
            current_message = line.trim().to_string();
        }
    }

    // Last entry
    if !current_hash.is_empty() {
        entries.push(format!(
            "{} {} ({}, {})",
            &current_hash[..7.min(current_hash.len())],
            current_message.trim(),
            current_author.split('<').next().unwrap_or("").trim(),
            current_date.trim(),
        ));
    }

    let total = entries.len();
    if total > max_entries {
        let mut result: Vec<String> = entries[..max_entries].to_vec();
        result.push(format!("... ({} more commits)", total - max_entries));
        result.join("\n")
    } else {
        entries.join("\n")
    }
}

// --- Git Status Filter ---
// Strategy: group by state, limit files per group

fn filter_status(output: &str, config: &Config) -> String {
    let max_files = config.limits.git_status_max_files;
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Porcelain format: XY filename
        if trimmed.len() > 3 {
            let (xy, _rest) = trimmed.split_at(2.min(trimmed.len()));
            let first = xy.chars().next().unwrap_or(' ');
            let second = xy.chars().nth(1).unwrap_or(' ');

            if first == '?' && second == '?' {
                untracked.push(trimmed.to_string());
            } else if second != ' ' {
                unstaged.push(trimmed.to_string());
            } else if first != ' ' {
                staged.push(trimmed.to_string());
            } else {
                // Header lines like "On branch main"
                staged.push(trimmed.to_string()); // Keep informational lines
            }
        } else {
            staged.push(trimmed.to_string());
        }
    }

    let mut result = Vec::new();

    if !staged.is_empty() {
        result.push(format!("Staged ({}):", staged.len()));
        for f in staged.iter().take(max_files) {
            result.push(format!("  {}", f));
        }
        if staged.len() > max_files {
            result.push(format!("  ... ({} more)", staged.len() - max_files));
        }
    }

    if !unstaged.is_empty() {
        result.push(format!("Unstaged ({}):", unstaged.len()));
        for f in unstaged.iter().take(max_files) {
            result.push(format!("  {}", f));
        }
        if unstaged.len() > max_files {
            result.push(format!("  ... ({} more)", unstaged.len() - max_files));
        }
    }

    if !untracked.is_empty() {
        result.push(format!("Untracked ({}):", untracked.len()));
        for f in untracked.iter().take(max_files) {
            result.push(format!("  {}", f));
        }
        if untracked.len() > max_files {
            result.push(format!("  ... ({} more)", untracked.len() - max_files));
        }
    }

    if result.is_empty() {
        "Nothing to commit, working tree clean".to_string()
    } else {
        result.join("\n")
    }
}

fn truncate_output(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= max_lines {
        return output.to_string();
    }
    let mut result: Vec<String> = lines[..max_lines].iter().map(|s| s.to_string()).collect();
    result.push(format!("... ({} more lines)", lines.len() - max_lines));
    result.join("\n")
}
