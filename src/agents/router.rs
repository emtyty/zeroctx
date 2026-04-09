use once_cell::sync::Lazy;
use regex::Regex;
use std::path::PathBuf;

use crate::core::types::{Intent, ParsedRequest, SubTask};

/// Regex-based intent router — zero token cost.
///
/// Classifies natural language requests into intents using pattern matching.
pub struct IntentRouter;

// Signal patterns for each intent type
static URL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://\S+").expect("valid regex"));
static FILE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:^|\s)([\w./\\-]+\.(?:rs|py|js|ts|cs|go|java|rb|jsx|tsx|json|toml|yaml|yml|md|txt|cfg|ini|sh|bat))\b").expect("valid regex"));
static COMMAND_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:run|execute|exec)\s+[`']?([^`'\n]+)[`']?").expect("valid regex")
});

impl IntentRouter {
    /// Parse a natural language request into a structured ParsedRequest.
    pub fn parse(request: &str) -> ParsedRequest {
        let lower = request.to_lowercase();

        // Extract URLs
        let urls: Vec<String> = URL_RE
            .find_iter(request)
            .map(|m| m.as_str().to_string())
            .collect();

        // Extract file paths
        let files: Vec<PathBuf> = FILE_RE
            .captures_iter(request)
            .filter_map(|c| c.get(1))
            .map(|m| PathBuf::from(m.as_str()))
            .collect();

        // Extract commands
        let commands: Vec<String> = COMMAND_RE
            .captures_iter(request)
            .filter_map(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .collect();

        // Detect intent from signals
        let intent = Self::classify_intent(&lower, &urls, &commands, &files);

        ParsedRequest {
            raw: request.to_string(),
            intent,
            urls,
            commands,
            files,
            search_patterns: Vec::new(),
            task: request.to_string(),
        }
    }

    fn classify_intent(
        lower: &str,
        urls: &[String],
        commands: &[String],
        files: &[PathBuf],
    ) -> Intent {
        // Multi-step detection
        let multi_step_signals = ["then", "and then", "after that", "first", "second", "finally"];
        let step_count = multi_step_signals
            .iter()
            .filter(|s| lower.contains(**s))
            .count();
        if step_count >= 2 {
            return Intent::MultiStep;
        }

        // Fetch + Analyze
        if !urls.is_empty()
            && (lower.contains("fetch")
                || lower.contains("read")
                || lower.contains("summarize")
                || lower.contains("analyze"))
        {
            return Intent::FetchAndAnalyze;
        }

        // Clone + Explore
        if lower.contains("clone") || lower.contains("repo") {
            return Intent::CloneAndExplore;
        }

        // Run + Debug
        let run_signals = [
            "run", "test", "build", "execute", "debug", "fix", "pytest", "cargo",
            "npm", "dotnet", "make",
        ];
        if run_signals.iter().any(|s| lower.contains(s)) || !commands.is_empty() {
            return Intent::RunAndDebug;
        }

        // Read + Refactor
        let read_signals = ["read", "refactor", "rename", "move", "extract", "explain"];
        if (read_signals.iter().any(|s| lower.contains(s))) && !files.is_empty() {
            return Intent::ReadAndRefactor;
        }

        // Default: code only
        Intent::CodeOnly
    }
}

/// Split multi-step requests into atomic sub-tasks.
pub struct TaskDecomposer;

impl TaskDecomposer {
    /// Decompose a multi-step request into ordered sub-tasks.
    pub fn decompose(request: &str) -> Vec<SubTask> {
        let lower = request.to_lowercase();

        // Split on step indicators
        let parts: Vec<&str> = if lower.contains(" then ") {
            request.split(" then ").collect()
        } else if lower.contains(". ") {
            request.split(". ").collect()
        } else {
            return vec![SubTask {
                description: request.to_string(),
                intent: IntentRouter::parse(request).intent,
                command: None,
                files: Vec::new(),
                depends_on: Vec::new(),
            }];
        };

        parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                let parsed = IntentRouter::parse(part.trim());
                SubTask {
                    description: part.trim().to_string(),
                    intent: parsed.intent,
                    command: parsed.commands.first().cloned(),
                    files: parsed.files,
                    depends_on: if i > 0 { vec![i - 1] } else { vec![] },
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_intent() {
        let parsed = IntentRouter::parse("run pytest and fix failures");
        assert_eq!(parsed.intent, Intent::RunAndDebug);
    }

    #[test]
    fn test_fetch_intent() {
        let parsed = IntentRouter::parse("fetch https://example.com and summarize");
        assert_eq!(parsed.intent, Intent::FetchAndAnalyze);
        assert_eq!(parsed.urls.len(), 1);
    }

    #[test]
    fn test_file_extraction() {
        let parsed = IntentRouter::parse("read src/auth.rs and explain the login flow");
        assert!(!parsed.files.is_empty());
    }

    #[test]
    fn test_multi_step() {
        let tasks = TaskDecomposer::decompose("run pytest then fix failures then run pytest again");
        assert!(tasks.len() >= 2);
    }
}
