pub mod autofix;
pub mod patterns;

use anyhow::Result;

use crate::core::mismatch::{MismatchCategory, MismatchEvent, MismatchSeverity};
use crate::core::types::AutoFix;

/// Classify an error from command output and return an auto-fix if possible.
///
/// This is the highest-impact feature: saves 2-4 min per auto-fixable error
/// by eliminating entire LLM round-trips.
pub fn classify(stderr: &str, stdout: &str, cwd: &str) -> Option<AutoFix> {
    let combined = format!("{}\n{}", stderr, stdout);

    // Try all pattern sets
    for pattern in patterns::all_patterns() {
        if let Some(captures) = pattern.regex.captures(&combined) {
            let fix = (pattern.handler)(&captures, &combined, cwd);
            return Some(fix);
        }
    }

    None
}

/// Execute an auto-fix command and track its success/failure.
pub fn execute_fix(fix: &AutoFix) -> Result<String> {
    if let Some(ref cmd) = fix.command {
        tracing::info!(command = cmd, category = &fix.category, "Auto-fixing error");
        let config = crate::config::Config::load()?;
        let output = crate::core::runner::execute_shell(cmd, &config)?;

        if output.exit_code == 0 {
            Ok(format!(
                "Auto-fixed: {} (ran: {})\n{}",
                fix.explanation, cmd, output.stdout
            ))
        } else {
            // Auto-fix failed — record as mismatch
            crate::core::mismatch::log_event(&MismatchEvent {
                category: MismatchCategory::AutoFix,
                severity: MismatchSeverity::Error,
                detected: format!(
                    "pattern={}, fix_cmd={}",
                    fix.category,
                    cmd
                ),
                actual: format!(
                    "fix failed (exit {}): {}",
                    output.exit_code,
                    truncate(&output.stderr, 300)
                ),
                input_snippet: fix.explanation.clone(),
                context: format!(
                    "language={:?}, stdout_preview={}",
                    fix.language,
                    truncate(&output.stdout, 200)
                ),
                user_feedback: None,
            });

            Ok(format!(
                "Auto-fix attempted but failed (exit {}):\n{}\n{}",
                output.exit_code, output.stdout, output.stderr
            ))
        }
    } else {
        Ok(fix.explanation.clone())
    }
}

/// Execute an auto-fix and then verify the original error is gone.
pub fn execute_fix_and_verify(
    fix: &AutoFix,
    original_command: &str,
    original_stderr: &str,
) -> Result<String> {
    let result = execute_fix(fix)?;

    // If fix seemed successful, re-run the original command to verify
    if fix.command.is_some() && !result.contains("failed") {
        let config = crate::config::Config::load()?;
        let verify = crate::core::runner::execute_shell(original_command, &config)?;

        // Check if original error is still present
        if verify.exit_code != 0 {
            // Error still present — check if it's the SAME error
            let _combined_verify = format!("{}\n{}", verify.stderr, verify.stdout);
            if let Some(new_fix) = classify(&verify.stderr, &verify.stdout, ".") {
                if new_fix.category == fix.category {
                    // Same error pattern still matches — fix didn't work
                    crate::core::mismatch::log_event(&MismatchEvent {
                        category: MismatchCategory::AutoFix,
                        severity: MismatchSeverity::Error,
                        detected: format!("pattern={}, fix_cmd={}", fix.category, fix.command.as_deref().unwrap_or("none")),
                        actual: "same error persists after fix".into(),
                        input_snippet: truncate(original_stderr, 300),
                        context: format!("verify_stderr: {}", truncate(&verify.stderr, 200)),
                        user_feedback: None,
                    });
                    crate::core::mismatch::log_signal(
                        "autofix_rerun",
                        original_command,
                        &format!("{{\"category\": \"{}\"}}", fix.category),
                    );
                }
            }
        }
    }

    Ok(result)
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// Extract file:line pairs from error output for error-line-aware AST compression.
/// Returns Vec<(file_path, line_number_0indexed)>.
pub fn extract_error_locations(stderr: &str) -> Vec<(String, usize)> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    // Python: File "path/to/file.py", line 42
    static PY_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"File ["']([^"']+\.py)["'],\s*line\s+(\d+)"#).expect("valid regex")
    });
    // Rust: src/foo.rs:42:5 or --> src/foo.rs:42:5
    static RS_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?:-->)?\s*([a-zA-Z0-9_./\-]+\.rs):(\d+):\d+").expect("valid regex")
    });
    // JS/TS: file.js:42:5 or file.ts:42:5
    static JS_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"([a-zA-Z0-9_./\-]+\.[jt]sx?):(\d+):\d+").expect("valid regex")
    });
    // C#: file.cs(42,5) or  File.cs:line 42
    static CS_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"([a-zA-Z0-9_./\-]+\.cs)\((\d+),\d+\)").expect("valid regex")
    });

    let mut results: Vec<(String, usize)> = Vec::new();
    let regexes: &[(&Lazy<Regex>, usize, usize)] = &[
        (&PY_RE, 1, 2),
        (&RS_RE, 1, 2),
        (&JS_RE, 1, 2),
        (&CS_RE, 1, 2),
    ];

    for (re, path_group, line_group) in regexes {
        for caps in re.captures_iter(stderr) {
            let path = caps.get(*path_group).map_or("", |m| m.as_str()).to_string();
            let line: usize = caps
                .get(*line_group)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(1)
                .saturating_sub(1); // convert to 0-indexed
            if !path.is_empty() {
                results.push((path, line));
            }
        }
    }

    // Deduplicate by (path, line)
    results.sort();
    results.dedup();
    results
}
