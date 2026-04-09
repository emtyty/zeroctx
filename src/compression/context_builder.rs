use crate::config::Config;
use crate::core::runner::estimate_tokens;

/// Budget-aware context assembly.
///
/// Assembles file contents, command outputs, and error info
/// within a configurable token budget.
pub struct ContextBuilder {
    parts: Vec<ContextPart>,
    budget: usize,
}

#[derive(Debug)]
struct ContextPart {
    label: String,
    content: String,
    priority: Priority,
    tokens: usize,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    /// Error output — always include
    Critical = 0,
    /// Files mentioned in error tracebacks
    ErrorRelated = 1,
    /// Recently changed files
    RecentlyChanged = 2,
    /// Other context
    Normal = 3,
    /// Low priority (summaries, old files)
    Low = 4,
}

impl ContextBuilder {
    pub fn new(config: &Config) -> Self {
        Self {
            parts: Vec::new(),
            budget: config.general.context_budget,
        }
    }

    /// Add a context part with the given priority.
    pub fn add(&mut self, label: &str, content: &str, critical: bool) {
        let priority = if critical {
            Priority::Critical
        } else {
            Priority::Normal
        };
        let tokens = estimate_tokens(content);
        self.parts.push(ContextPart {
            label: label.to_string(),
            content: content.to_string(),
            priority,
            tokens,
        });
    }

    /// Add error-related file content (high priority).
    pub fn add_error_file(&mut self, label: &str, content: &str) {
        let tokens = estimate_tokens(content);
        self.parts.push(ContextPart {
            label: label.to_string(),
            content: content.to_string(),
            priority: Priority::ErrorRelated,
            tokens,
        });
    }

    /// Add a low-priority summary (for cached files).
    pub fn add_summary(&mut self, label: &str, content: &str) {
        let tokens = estimate_tokens(content);
        self.parts.push(ContextPart {
            label: label.to_string(),
            content: content.to_string(),
            priority: Priority::Low,
            tokens,
        });
    }

    /// Build the final context string within the token budget.
    pub fn build(mut self) -> String {
        // Sort by priority (critical first)
        self.parts.sort_by(|a, b| a.priority.cmp(&b.priority));

        let mut result = Vec::new();
        let mut used_tokens = 0;

        for part in &self.parts {
            if used_tokens + part.tokens > self.budget {
                // Budget exceeded — add truncation notice
                result.push(format!(
                    "--- {} (skipped: {} tokens, over budget) ---",
                    part.label, part.tokens
                ));
                continue;
            }
            result.push(format!("--- {} ---\n{}", part.label, part.content));
            used_tokens += part.tokens;
        }

        result.join("\n\n")
    }
}
