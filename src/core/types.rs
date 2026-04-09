use std::collections::HashMap;
use std::path::PathBuf;

/// Intent types detected by the Router agent from natural language requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Fetch a URL and analyze its content
    FetchAndAnalyze,
    /// Clone a git repo and explore its structure
    CloneAndExplore,
    /// Run a command and debug any errors
    RunAndDebug,
    /// Read files and refactor code
    ReadAndRefactor,
    /// Pure code generation (no I/O needed)
    CodeOnly,
    /// Complex request requiring multiple steps
    MultiStep,
}

/// Parsed representation of a user request after intent routing.
#[derive(Debug, Clone)]
pub struct ParsedRequest {
    /// The original raw request text
    pub raw: String,
    /// Detected intent type
    pub intent: Intent,
    /// URLs found in the request
    pub urls: Vec<String>,
    /// Shell commands to execute
    pub commands: Vec<String>,
    /// File paths referenced
    pub files: Vec<PathBuf>,
    /// Search patterns/keywords
    pub search_patterns: Vec<String>,
    /// The core task description (cleaned)
    pub task: String,
}

/// A sub-task produced by the TaskDecomposer for multi-step requests.
#[derive(Debug, Clone)]
pub struct SubTask {
    /// Human-readable description
    pub description: String,
    /// Intent for this sub-task
    pub intent: Intent,
    /// Shell command to execute (if any)
    pub command: Option<String>,
    /// Files relevant to this sub-task
    pub files: Vec<PathBuf>,
    /// Indices of sub-tasks this depends on
    pub depends_on: Vec<usize>,
}

/// Result of an auto-fix attempt by the ErrorClassifier.
#[derive(Debug, Clone)]
pub struct AutoFix {
    /// Whether this error can be automatically fixed
    pub fixable: bool,
    /// Error category (e.g., "module_not_found", "type_error")
    pub category: String,
    /// Human-readable explanation of the error
    pub explanation: String,
    /// Command to run for the fix (if fixable)
    pub command: Option<String>,
    /// The language this error came from
    pub language: Language,
}

/// Result from an output filter.
#[derive(Debug, Clone)]
pub struct FilterResult {
    /// The filtered/compressed output
    pub output: String,
    /// Number of lines in the original output
    pub original_lines: usize,
    /// Number of lines after filtering
    pub filtered_lines: usize,
    /// Percentage of tokens saved (estimated)
    pub savings_percent: f64,
}

impl FilterResult {
    pub fn passthrough(output: String) -> Self {
        let lines = output.lines().count();
        Self {
            output,
            original_lines: lines,
            filtered_lines: lines,
            savings_percent: 0.0,
        }
    }
}

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Python,
    JavaScript,
    TypeScript,
    CSharp,
    Rust,
    Go,
    Java,
    Ruby,
    Unknown,
}

impl Language {
    /// Detect language from file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "py" | "pyw" => Self::Python,
            "js" | "mjs" | "cjs" | "jsx" => Self::JavaScript,
            "ts" | "mts" | "cts" | "tsx" => Self::TypeScript,
            "cs" | "razor" | "cshtml" => Self::CSharp,
            "rs" => Self::Rust,
            "go" => Self::Go,
            "java" => Self::Java,
            "rb" => Self::Ruby,
            _ => Self::Unknown,
        }
    }
}

/// Validation result from a language validator.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the code is valid
    pub valid: bool,
    /// Error messages (if any)
    pub errors: Vec<String>,
    /// Warnings (if any)
    pub warnings: Vec<String>,
}

/// Token tracking record for a single operation.
#[derive(Debug, Clone)]
pub struct TrackingRecord {
    /// Timestamp (UTC)
    pub timestamp: chrono::NaiveDateTime,
    /// Original command or operation
    pub operation: String,
    /// Estimated input tokens (original)
    pub input_tokens: usize,
    /// Estimated output tokens (after filtering)
    pub output_tokens: usize,
    /// Savings percentage
    pub savings_percent: f64,
    /// How the savings were achieved
    pub method: SavingsMethod,
}

/// How token savings were achieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavingsMethod {
    /// Output filter compressed command output
    OutputFilter,
    /// Error was auto-fixed without LLM
    ErrorAutoFix,
    /// AST compression reduced file content
    AstCompression,
    /// Context cache deduplication
    ContextCache,
    /// Diff-only output format
    DiffOutput,
    /// No savings (passthrough)
    None,
}

/// Session state for multi-round context caching.
#[derive(Debug, Clone, Default)]
pub struct SessionState {
    /// File path → MD5 hash of content
    pub file_hashes: HashMap<PathBuf, String>,
    /// File path → one-line summary for cached files
    pub file_summaries: HashMap<PathBuf, String>,
    /// Round number (increments each interaction)
    pub round: usize,
}
