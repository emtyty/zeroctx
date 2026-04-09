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
