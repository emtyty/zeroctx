use once_cell::sync::Lazy;
use regex::Regex;
use std::path::PathBuf;

use crate::core::mismatch::{MismatchCategory, MismatchEvent, MismatchSeverity, MismatchTracker};
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

        // Detect intent from signals + check for ambiguity
        let (intent, confidence) = Self::classify_intent_with_confidence(&lower, &urls, &commands, &files);

        // Log mismatch if confidence is low (ambiguous signals)
        if confidence.competing_intents.len() > 1 {
            Self::log_ambiguous_routing(request, &intent, &confidence);
        }

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
        Self::classify_intent_with_confidence(lower, urls, commands, files).0
    }

    fn classify_intent_with_confidence(
        lower: &str,
        urls: &[String],
        commands: &[String],
        files: &[PathBuf],
    ) -> (Intent, IntentConfidence) {
        let mut competing = Vec::new();

        // Multi-step detection
        let multi_step_signals = ["then", "and then", "after that", "first", "second", "finally"];
        let step_count = multi_step_signals
            .iter()
            .filter(|s| lower.contains(**s))
            .count();
        if step_count >= 2 {
            competing.push(("MultiStep", step_count as f64 * 0.4));
        }

        // Fetch + Analyze signals
        let fetch_signals = ["fetch", "read", "summarize", "analyze"];
        let fetch_score: f64 = if !urls.is_empty() {
            fetch_signals.iter().filter(|s| lower.contains(**s)).count() as f64 * 0.3 + 0.3
        } else {
            0.0
        };
        if fetch_score > 0.0 {
            competing.push(("FetchAndAnalyze", fetch_score));
        }

        // Clone + Explore
        if lower.contains("clone") || lower.contains("repo") {
            competing.push(("CloneAndExplore", 0.7));
        }

        // Run + Debug signals
        let run_signals = [
            "run", "test", "build", "execute", "debug", "fix", "pytest", "cargo",
            "npm", "dotnet", "make",
        ];
        let run_score: f64 = run_signals.iter().filter(|s| lower.contains(**s)).count() as f64 * 0.2
            + if !commands.is_empty() { 0.3 } else { 0.0 };
        if run_score > 0.0 {
            competing.push(("RunAndDebug", run_score));
        }

        // Read + Refactor signals
        let read_signals = ["read", "refactor", "rename", "move", "extract", "explain"];
        let read_score: f64 = if !files.is_empty() {
            read_signals.iter().filter(|s| lower.contains(**s)).count() as f64 * 0.25
        } else {
            0.0
        };
        if read_score > 0.0 {
            competing.push(("ReadAndRefactor", read_score));
        }

        // Sort by score descending
        competing.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Pick the winner (same logic as before, but now we have scores)
        let winner = if step_count >= 2 {
            Intent::MultiStep
        } else if !urls.is_empty()
            && (lower.contains("fetch")
                || lower.contains("read")
                || lower.contains("summarize")
                || lower.contains("analyze"))
        {
            Intent::FetchAndAnalyze
        } else if lower.contains("clone") || lower.contains("repo") {
            Intent::CloneAndExplore
        } else if run_signals.iter().any(|s| lower.contains(s)) || !commands.is_empty() {
            Intent::RunAndDebug
        } else if (read_signals.iter().any(|s| lower.contains(s))) && !files.is_empty() {
            Intent::ReadAndRefactor
        } else {
            Intent::CodeOnly
        };

        let confidence = IntentConfidence {
            competing_intents: competing,
        };

        (winner, confidence)
    }

    fn log_ambiguous_routing(request: &str, chosen: &Intent, confidence: &IntentConfidence) {
        let competing_str: Vec<String> = confidence
            .competing_intents
            .iter()
            .map(|(name, score)| format!("{}({:.2})", name, score))
            .collect();

        // Only log if there are genuinely close scores (ambiguity)
        if confidence.competing_intents.len() >= 2 {
            let top = confidence.competing_intents[0].1;
            let second = confidence.competing_intents[1].1;
            // If the gap between top two is small, it's ambiguous
            if (top - second).abs() < 0.2 {
                let severity = if (top - second).abs() < 0.1 {
                    MismatchSeverity::Warn
                } else {
                    MismatchSeverity::Info
                };

                if let Ok(tracker) = MismatchTracker::open(None) {
                    let _ = tracker.record(&MismatchEvent {
                        category: MismatchCategory::IntentRouting,
                        severity,
                        detected: format!("{:?}", chosen),
                        actual: String::new(), // unknown until user feedback
                        input_snippet: request.to_string(),
                        context: format!("competing: [{}]", competing_str.join(", ")),
                        user_feedback: None,
                    });
                }

                tracing::debug!(
                    chosen = ?chosen,
                    competing = %competing_str.join(", "),
                    "Ambiguous intent routing"
                );
            }
        }
    }
}

/// Confidence information for intent classification.
struct IntentConfidence {
    /// (intent_name, score) pairs, sorted by score descending.
    competing_intents: Vec<(&'static str, f64)>,
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
