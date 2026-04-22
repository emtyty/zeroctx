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
        let cmd = command.split_whitespace().next().unwrap_or("");
        matches!(cmd, "git" | "gh")
    }

    fn filter(&self, output: &str, config: &Config) -> FilterResult {
        let original_lines = output.lines().count();

        let filtered = if is_diff_output(output) {
            filter_diff(output, config)
        } else if is_log_output(output) {
            filter_log(output, config)
        } else if is_show_stat_output(output) {
            filter_show_stat(output, config)
        } else if is_status_output(output) {
            filter_status(output, config)
        } else {
            truncate_output(output, config.limits.tree_max_entries)
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

fn is_diff_output(output: &str) -> bool {
    output.lines().take(5).any(|l| {
        l.starts_with("diff --git")
            || l.starts_with("---")
            || l.starts_with("+++")
            || l.starts_with("@@")
    })
}

fn is_log_output(output: &str) -> bool {
    output.lines().take(3).any(|l| l.starts_with("commit "))
}

fn is_show_stat_output(output: &str) -> bool {
    output.lines().any(|l| {
        l.contains("|") && (l.contains("+") || l.contains("-"))
            && l.split('|').count() == 2
    })
}

fn is_status_output(output: &str) -> bool {
    let first = output.lines().next().unwrap_or("");
    first.starts_with("On branch")
        || first.starts_with("HEAD detached")
        || output.contains("Changes not staged")
        || output.contains("Changes to be committed")
        || output.contains("Untracked files")
        || output.contains("nothing to commit")
}

/// Represents a single file in a diff with its change stats.
struct DiffFile {
    name: String,
    additions: usize,
    deletions: usize,
    lines: Vec<String>,
}

/// Filter git diff output. If total content lines > 300: stat view + top 3 files.
fn filter_diff(output: &str, config: &Config) -> String {
    static NOISE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^(index [0-9a-f]+\.\.[0-9a-f]+|old mode|new mode|Binary files|diff --git)")
            .expect("valid regex")
    });

    let clean: Vec<&str> = output
        .lines()
        .filter(|l| !NOISE_RE.is_match(l))
        .collect();

    let total_content_lines: usize = clean
        .iter()
        .filter(|l| {
            let s = l.trim_start();
            (s.starts_with('+') || s.starts_with('-'))
                && !s.starts_with("---") && !s.starts_with("+++")
        })
        .count();

    if total_content_lines > 300 {
        let files = collect_diff_files(output, config);
        let summary = format!("{} files changed", files.len());
        return format_large_diff(&files, &summary, total_content_lines);
    }

    let max_per_hunk = config.limits.git_diff_max_hunk_lines;
    let mut result = Vec::new();
    let mut hunk_lines = 0usize;
    let mut hunk_skipped = 0usize;
    let mut in_hunk = false;

    for line in &clean {
        if line.starts_with("@@") {
            if hunk_skipped > 0 {
                result.push(format!("  ... ({} more lines)", hunk_skipped));
                hunk_skipped = 0;
            }
            hunk_lines = 0;
            in_hunk = true;
            result.push(line.to_string());
        } else if line.starts_with("--- ") || line.starts_with("+++ ") {
            in_hunk = false;
            hunk_lines = 0;
            if hunk_skipped > 0 {
                result.push(format!("  ... ({} more lines)", hunk_skipped));
                hunk_skipped = 0;
            }
            result.push(line.to_string());
        } else if in_hunk {
            if hunk_lines < max_per_hunk {
                result.push(line.to_string());
                hunk_lines += 1;
            } else {
                hunk_skipped += 1;
            }
        } else {
            result.push(line.to_string());
        }
    }

    if hunk_skipped > 0 {
        result.push(format!("  ... ({} more lines)", hunk_skipped));
    }

    result.join("\n")
}

fn collect_diff_files(output: &str, _config: &Config) -> Vec<DiffFile> {
    let mut files: Vec<DiffFile> = Vec::new();
    let mut current: Option<DiffFile> = None;

    for line in output.lines() {
        if line.starts_with("diff --git ") {
            if let Some(f) = current.take() {
                files.push(f);
            }
            let name = line
                .split_whitespace()
                .last()
                .unwrap_or("unknown")
                .trim_start_matches("b/")
                .to_string();
            current = Some(DiffFile {
                name,
                additions: 0,
                deletions: 0,
                lines: Vec::new(),
            });
        } else if let Some(ref mut f) = current {
            if line.starts_with('+') && !line.starts_with("+++") {
                f.additions += 1;
                f.lines.push(line.to_string());
            } else if line.starts_with('-') && !line.starts_with("---") {
                f.deletions += 1;
                f.lines.push(line.to_string());
            } else if !line.starts_with("index ")
                && !line.starts_with("old mode")
                && !line.starts_with("new mode")
                && !line.starts_with("Binary")
            {
                f.lines.push(line.to_string());
            }
        }
    }

    if let Some(f) = current {
        files.push(f);
    }

    files
}

fn format_large_diff(files: &[DiffFile], summary: &str, total_lines: usize) -> String {
    let mut result = Vec::new();

    result.push(format!(
        "// [Large diff: {} content lines — showing stat + top 3 files]",
        total_lines
    ));
    result.push(summary.to_string());
    result.push(String::new());

    let mut sorted_indices: Vec<usize> = (0..files.len()).collect();
    sorted_indices.sort_by(|&a, &b| {
        let ta = files[a].additions + files[a].deletions;
        let tb = files[b].additions + files[b].deletions;
        tb.cmp(&ta)
    });

    const BAR_WIDTH: usize = 40;
    let max_changes = sorted_indices
        .first()
        .map(|&i| files[i].additions + files[i].deletions)
        .unwrap_or(1)
        .max(1);

    for &idx in &sorted_indices {
        let f = &files[idx];
        let total = f.additions + f.deletions;
        let bar_len = total * BAR_WIDTH / max_changes;
        let add_len = f.additions * bar_len / total.max(1);
        let del_len = bar_len.saturating_sub(add_len);
        let bar = format!("{}{}", "+".repeat(add_len), "-".repeat(del_len));
        result.push(format!(" {:50} | {:>4} {}", f.name, total, bar));
    }

    result.push(String::new());

    let top3 = sorted_indices.iter().take(3);
    for &idx in top3 {
        let f = &files[idx];
        result.push(format!("// === {} (+{} -{}) ===", f.name, f.additions, f.deletions));
        for line in &f.lines {
            result.push(line.clone());
        }
        result.push(String::new());
    }

    if files.len() > 3 {
        result.push(format!(
            "// [Remaining {} files omitted. Run 'git diff' directly for full output.]",
            files.len() - 3
        ));
    }

    result.join("\n")
}

fn filter_log(output: &str, config: &Config) -> String {
    static HASH_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^commit ([0-9a-f]{40})").expect("valid regex"));

    let max = config.limits.tree_max_entries;
    let mut result = Vec::new();
    let mut entry_count = 0usize;
    let mut current_entry: Vec<&str> = Vec::new();

    for line in output.lines() {
        if HASH_RE.is_match(line) {
            if !current_entry.is_empty() {
                if entry_count >= max {
                    let remaining = output
                        .lines()
                        .filter(|l| HASH_RE.is_match(l))
                        .count()
                        .saturating_sub(entry_count);
                    if remaining > 0 {
                        result.push(format!("... ({} more commits)", remaining));
                    }
                    break;
                }
                flush_log_entry(&mut result, &current_entry);
                entry_count += 1;
                current_entry.clear();
            }
            current_entry.push(line);
        } else {
            current_entry.push(line);
        }
    }

    if entry_count < max && !current_entry.is_empty() {
        flush_log_entry(&mut result, &current_entry);
    }

    result.join("\n")
}

fn flush_log_entry(result: &mut Vec<String>, entry: &[&str]) {
    if let Some(hash_line) = entry.first() {
        let short_hash = &hash_line.get(7..15).unwrap_or("");
        let author = entry
            .iter()
            .find(|l| l.starts_with("Author:"))
            .map(|l| l.trim_start_matches("Author:").trim())
            .unwrap_or("");
        let date = entry
            .iter()
            .find(|l| l.starts_with("Date:"))
            .map(|l| l.trim_start_matches("Date:").trim())
            .unwrap_or("");
        let subject = entry
            .iter()
            .skip_while(|l| !l.starts_with("Date:"))
            .skip(1)
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim())
            .unwrap_or("");
        result.push(format!("{} {} ({}, {})", short_hash, subject, author, date));
    }
}

fn filter_show_stat(output: &str, config: &Config) -> String {
    truncate_output(output, config.limits.tree_max_entries * 3)
}

fn filter_status(output: &str, config: &Config) -> String {
    let max_per_group = config.limits.git_status_max_files;
    let mut result = Vec::new();
    let mut group_count = 0usize;
    let mut in_file_list = false;
    let mut skipped = 0usize;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if skipped > 0 {
                result.push(format!("  ... ({} more files)", skipped));
                skipped = 0;
            }
            group_count = 0;
            in_file_list = false;
            result.push(String::new());
            continue;
        }

        if line.starts_with("On branch")
            || line.starts_with("HEAD")
            || line.contains("Changes to be committed")
            || line.contains("Changes not staged")
            || line.contains("Untracked files")
            || line.contains("nothing to commit")
            || line.contains("Your branch")
        {
            if skipped > 0 {
                result.push(format!("  ... ({} more files)", skipped));
                skipped = 0;
            }
            group_count = 0;
            in_file_list = false;
            result.push(line.to_string());
            continue;
        }

        if trimmed.starts_with('(') {
            in_file_list = true;
            result.push(line.to_string());
            continue;
        }

        if in_file_list {
            if group_count < max_per_group {
                result.push(line.to_string());
                group_count += 1;
            } else {
                skipped += 1;
            }
        } else {
            result.push(line.to_string());
        }
    }

    if skipped > 0 {
        result.push(format!("  ... ({} more files)", skipped));
    }

    result.join("\n")
}

fn truncate_output(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= max_lines {
        return output.to_string();
    }
    let remaining_msg = format!("... ({} more lines)", lines.len() - max_lines);
    let mut r: Vec<String> = lines[..max_lines].iter().map(|l| l.to_string()).collect();
    r.push(remaining_msg);
    r.join("\n")
}
