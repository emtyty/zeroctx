pub mod autofix;
pub mod patterns;

use anyhow::Result;

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

/// Execute an auto-fix command.
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
            Ok(format!(
                "Auto-fix attempted but failed (exit {}):\n{}\n{}",
                output.exit_code, output.stdout, output.stderr
            ))
        }
    } else {
        Ok(fix.explanation.clone())
    }
}
