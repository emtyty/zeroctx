use anyhow::Result;

use crate::agents::IoResults;
use crate::compression::context_builder::ContextBuilder;
use crate::config::Config;
use crate::core::types::ParsedRequest;
use crate::errors;

/// Result of the analysis phase.
pub struct AnalysisResult {
    /// Assembled context for the reasoning agent.
    pub context: String,
    /// If an error was auto-fixed, the result is here (short-circuits reasoning).
    pub auto_fix_result: Option<String>,
}

/// Analyze I/O results: error classify → AST compress → cache → build context.
pub fn analyze(
    io: &IoResults,
    parsed: &ParsedRequest,
    config: &Config,
) -> Result<AnalysisResult> {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    // Step 1: Check for auto-fixable errors in command outputs
    if config.autofix.enabled {
        for (_cmd, stdout, stderr, exit_code) in &io.command_outputs {
            if *exit_code != 0 {
                if let Some(fix) = errors::classify(stderr, stdout, &cwd) {
                    if fix.fixable && config.autofix.auto_run {
                        match errors::execute_fix(&fix) {
                            Ok(result) => {
                                return Ok(AnalysisResult {
                                    context: String::new(),
                                    auto_fix_result: Some(result),
                                });
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Auto-fix execution failed");
                            }
                        }
                    } else if !fix.fixable {
                        // Non-fixable but classified — include explanation in context
                        tracing::info!(
                            category = &fix.category,
                            "Error classified (not auto-fixable)"
                        );
                    }
                }
            }
        }
    }

    // Step 2: Build context within token budget
    let mut builder = ContextBuilder::new(config);

    // Add command outputs (high priority)
    for (cmd, stdout, stderr, exit_code) in &io.command_outputs {
        let label = format!("$ {} (exit {})", cmd, exit_code);
        let content = if stderr.is_empty() {
            stdout.clone()
        } else {
            format!("{}\n--- stderr ---\n{}", stdout, stderr)
        };
        builder.add(&label, &content, *exit_code != 0);
    }

    // Add file contents
    // TODO: Apply AST compression based on error context
    for (path, content) in &io.file_contents {
        builder.add(path, content, false);
    }

    // Add web contents
    for (url, content) in &io.web_contents {
        builder.add(url, content, false);
    }

    // Add the task description
    builder.add("Task", &parsed.task, true);

    Ok(AnalysisResult {
        context: builder.build(),
        auto_fix_result: None,
    })
}
