pub mod dotnet;
pub mod git;
pub mod javascript;
pub mod network;
pub mod python;
pub mod rust_tools;
pub mod system;
pub mod toml_filter;

use crate::config::Config;
use crate::core::mismatch::{MismatchCategory, MismatchEvent, MismatchSeverity, MismatchTracker};
use crate::core::types::FilterResult;

/// Trait for output filters. Each filter handles a specific command category.
///
/// This replaces RTK's ~30 duplicated filter functions with a single trait.
pub trait OutputFilter: Send + Sync {
    /// Name of this filter (for logging/tracking).
    fn name(&self) -> &str;

    /// Whether this filter handles the given command.
    fn matches(&self, command: &str) -> bool;

    /// Filter/compress the command output.
    fn filter(&self, output: &str, config: &Config) -> FilterResult;
}

/// Registry of all output filters.
pub struct FilterRegistry {
    filters: Vec<Box<dyn OutputFilter>>,
}

/// Threshold: if filter removes more than this %, flag as potentially over-aggressive.
const OVER_FILTER_THRESHOLD: f64 = 95.0;
/// Threshold: minimum lines in original output to bother checking over-filtering.
const MIN_LINES_FOR_CHECK: usize = 10;

impl FilterRegistry {
    /// Create a registry with all built-in filters.
    pub fn new() -> Self {
        let filters: Vec<Box<dyn OutputFilter>> = vec![
            Box::new(git::GitFilter),
            Box::new(python::PythonFilter),
            Box::new(javascript::JavaScriptFilter),
            Box::new(dotnet::DotnetFilter),
            Box::new(rust_tools::RustFilter),
            Box::new(system::SystemFilter),
            Box::new(network::NetworkFilter),
        ];
        Self { filters }
    }

    /// Find the best matching filter for a command and apply it.
    /// Also records mismatch signals for quality tracking.
    pub fn apply(&self, command: &str, output: &str, config: &Config) -> FilterResult {
        for filter in &self.filters {
            if filter.matches(command) {
                let result = filter.filter(output, config);

                // Check for over-filtering
                if result.original_lines >= MIN_LINES_FOR_CHECK
                    && result.savings_percent > OVER_FILTER_THRESHOLD
                    && result.filtered_lines <= 2
                {
                    if let Ok(tracker) = MismatchTracker::open(None) {
                        let _ = tracker.record(&MismatchEvent {
                            category: MismatchCategory::OutputFilter,
                            severity: MismatchSeverity::Warn,
                            detected: format!(
                                "filter={}, savings={:.1}%",
                                filter.name(),
                                result.savings_percent
                            ),
                            actual: format!(
                                "{}→{} lines (may be too aggressive)",
                                result.original_lines, result.filtered_lines
                            ),
                            input_snippet: truncate_for_log(command, 200),
                            context: format!(
                                "output_preview: {}",
                                truncate_for_log(&result.output, 200)
                            ),
                            user_feedback: None,
                        });
                    }
                }

                return result;
            }
        }

        // No filter matched — record signal for gap analysis
        let output_lines = output.lines().count();
        if output_lines > 20 {
            if let Ok(tracker) = MismatchTracker::open(None) {
                let _ = tracker.record_signal(
                    "no_filter_match",
                    command,
                    &format!("{{\"output_lines\": {}}}", output_lines),
                );
            }
        }

        FilterResult::passthrough(output.to_string())
    }

    /// Apply filter and also record if this is a retry of the same command
    /// (signals that the previous filter may have been too aggressive).
    pub fn apply_with_retry_check(
        &self,
        command: &str,
        output: &str,
        config: &Config,
        recent_commands: &[String],
    ) -> FilterResult {
        // Check if this command was recently run (retry signal)
        let normalized = command.trim().to_lowercase();
        let retry_count = recent_commands
            .iter()
            .filter(|c| c.trim().to_lowercase() == normalized)
            .count();

        if retry_count > 0 {
            if let Ok(tracker) = MismatchTracker::open(None) {
                let _ = tracker.record_signal(
                    "retry_same_command",
                    command,
                    &format!("{{\"retry_count\": {}}}", retry_count),
                );
            }
        }

        self.apply(command, output, config)
    }
}

impl Default for FilterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn truncate_for_log(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
