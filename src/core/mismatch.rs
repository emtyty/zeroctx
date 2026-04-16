use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::debug;

/// Whether tables have been initialized in this process.
static TABLES_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Log a mismatch event — convenience function that opens connection only when needed.
/// All instrumentation code should call this instead of MismatchTracker::open() directly.
pub fn log_event(event: &MismatchEvent) {
    if let Ok(tracker) = MismatchTracker::open(None) {
        let _ = tracker.record(event);
    }
}

/// Log a mismatch signal — convenience function.
pub fn log_signal(signal_type: &str, operation: &str, metadata: &str) {
    if let Ok(tracker) = MismatchTracker::open(None) {
        let _ = tracker.record_signal(signal_type, operation, metadata);
    }
}

/// Mismatch category — which agent/phase produced the mismatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchCategory {
    /// Router classified intent incorrectly
    IntentRouting,
    /// Filter applied wrong filter or over/under-filtered
    OutputFilter,
    /// Auto-fix pattern matched but fix failed or was wrong
    AutoFix,
    /// AST compression lost important context
    Compression,
    /// Language detection was wrong
    LanguageDetection,
    /// Command rewrite was incorrect
    CommandRewrite,
}

impl MismatchCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::IntentRouting => "intent_routing",
            Self::OutputFilter => "output_filter",
            Self::AutoFix => "autofix",
            Self::Compression => "compression",
            Self::LanguageDetection => "language_detection",
            Self::CommandRewrite => "command_rewrite",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "intent_routing" => Self::IntentRouting,
            "output_filter" => Self::OutputFilter,
            "autofix" => Self::AutoFix,
            "compression" => Self::Compression,
            "language_detection" => Self::LanguageDetection,
            "command_rewrite" => Self::CommandRewrite,
            _ => Self::IntentRouting,
        }
    }
}

/// Severity of a mismatch — how much impact it had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchSeverity {
    /// Informational — detected anomaly, no user impact
    Info,
    /// Warning — possible quality degradation
    Warn,
    /// Error — caused incorrect behavior or required user retry
    Error,
}

impl MismatchSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "warn" => Self::Warn,
            "error" => Self::Error,
            _ => Self::Info,
        }
    }
}

/// A single mismatch event recorded during execution.
#[derive(Debug, Clone)]
pub struct MismatchEvent {
    pub category: MismatchCategory,
    pub severity: MismatchSeverity,
    /// What the system detected/chose
    pub detected: String,
    /// What was expected or what actually happened (if known)
    pub actual: String,
    /// The input that caused the mismatch
    pub input_snippet: String,
    /// Additional context (command, file path, error message, etc.)
    pub context: String,
    /// Whether user provided feedback on this
    pub user_feedback: Option<String>,
}

/// SQLite-based mismatch tracker for quality evaluation.
pub struct MismatchTracker {
    conn: Connection,
}

impl MismatchTracker {
    pub fn open(path: Option<&PathBuf>) -> Result<Self> {
        let conn = match path {
            Some(p) => {
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                Connection::open(p)?
            }
            None => {
                let default_path = default_mismatch_db_path()?;
                if let Some(parent) = default_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                Connection::open(&default_path)?
            }
        };
        let tracker = Self { conn };
        // Only init tables once per process
        if !TABLES_INITIALIZED.swap(true, Ordering::Relaxed) {
            tracker.init_tables()?;
        }
        Ok(tracker)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let tracker = Self { conn };
        tracker.init_tables()?;
        Ok(tracker)
    }

    fn init_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS mismatch_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now')),
                category TEXT NOT NULL,
                severity TEXT NOT NULL,
                detected TEXT NOT NULL,
                actual TEXT NOT NULL DEFAULT '',
                input_snippet TEXT NOT NULL,
                context TEXT NOT NULL DEFAULT '',
                user_feedback TEXT,
                resolved INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS mismatch_signals (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now')),
                signal_type TEXT NOT NULL,
                operation TEXT NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}'
            );

            CREATE INDEX IF NOT EXISTS idx_mismatch_category ON mismatch_events(category);
            CREATE INDEX IF NOT EXISTS idx_mismatch_severity ON mismatch_events(severity);
            CREATE INDEX IF NOT EXISTS idx_mismatch_timestamp ON mismatch_events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_signals_type ON mismatch_signals(signal_type);",
        )?;
        Ok(())
    }

    /// Record a mismatch event.
    pub fn record(&self, event: &MismatchEvent) -> Result<i64> {
        debug!(
            category = event.category.as_str(),
            severity = event.severity.as_str(),
            detected = &event.detected,
            actual = &event.actual,
            "Recording mismatch event"
        );

        self.conn.execute(
            "INSERT INTO mismatch_events (category, severity, detected, actual, input_snippet, context, user_feedback)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                event.category.as_str(),
                event.severity.as_str(),
                event.detected,
                event.actual,
                truncate_snippet(&event.input_snippet, 500),
                truncate_snippet(&event.context, 500),
                event.user_feedback,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Record a signal — lightweight breadcrumb for detecting mismatches indirectly.
    ///
    /// Signal types:
    /// - "retry_same_command" — user ran same command again (filter may have over-stripped)
    /// - "retry_same_file"   — AI re-requested a compressed file (compression too aggressive)
    /// - "autofix_rerun"     — same error appeared after auto-fix
    /// - "fallback_used"     — AST fell back to regex compression
    /// - "no_filter_match"   — no filter matched a command (potential gap)
    /// - "intent_override"   — user overrode routed intent
    pub fn record_signal(&self, signal_type: &str, operation: &str, metadata: &str) -> Result<()> {
        debug!(signal_type, operation, "Recording mismatch signal");
        self.conn.execute(
            "INSERT INTO mismatch_signals (signal_type, operation, metadata) VALUES (?1, ?2, ?3)",
            rusqlite::params![signal_type, operation, metadata],
        )?;
        Ok(())
    }

    /// Add user feedback to a mismatch event by ID.
    pub fn add_feedback(&self, event_id: i64, feedback: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE mismatch_events SET user_feedback = ?1, resolved = 1 WHERE id = ?2",
            rusqlite::params![feedback, event_id],
        )?;
        Ok(())
    }

    /// Mark event as resolved.
    pub fn resolve(&self, event_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE mismatch_events SET resolved = 1 WHERE id = ?1",
            rusqlite::params![event_id],
        )?;
        Ok(())
    }

    /// Get mismatch report — aggregated by category.
    pub fn get_report(&self, days: u32) -> Result<MismatchReport> {
        let offset = format!("-{} days", days);

        // Events by category
        let mut stmt = self.conn.prepare(
            "SELECT category, severity, COUNT(*), 
                    GROUP_CONCAT(DISTINCT detected) as examples
             FROM mismatch_events
             WHERE timestamp >= datetime('now', ?1)
             GROUP BY category, severity
             ORDER BY COUNT(*) DESC",
        )?;
        let category_rows = stmt
            .query_map([&offset], |row| {
                Ok(CategoryBreakdown {
                    category: row.get(0)?,
                    severity: row.get(1)?,
                    count: row.get(2)?,
                    examples: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Signals summary
        let mut stmt = self.conn.prepare(
            "SELECT signal_type, COUNT(*), GROUP_CONCAT(DISTINCT operation)
             FROM mismatch_signals
             WHERE timestamp >= datetime('now', ?1)
             GROUP BY signal_type
             ORDER BY COUNT(*) DESC",
        )?;
        let signal_rows = stmt
            .query_map([&offset], |row| {
                Ok(SignalSummary {
                    signal_type: row.get(0)?,
                    count: row.get(1)?,
                    operations: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Top unresolved mismatches
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, category, severity, detected, actual, input_snippet, context
             FROM mismatch_events
             WHERE resolved = 0 AND timestamp >= datetime('now', ?1)
             ORDER BY 
                CASE severity WHEN 'error' THEN 0 WHEN 'warn' THEN 1 ELSE 2 END,
                timestamp DESC
             LIMIT 20",
        )?;
        let unresolved = stmt
            .query_map([&offset], |row| {
                Ok(UnresolvedEvent {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    category: row.get(2)?,
                    severity: row.get(3)?,
                    detected: row.get(4)?,
                    actual: row.get(5)?,
                    input_snippet: row.get(6)?,
                    context: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Total counts
        let total_events: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM mismatch_events WHERE timestamp >= datetime('now', ?1)",
            [&offset],
            |row| row.get(0),
        )?;
        let total_signals: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM mismatch_signals WHERE timestamp >= datetime('now', ?1)",
            [&offset],
            |row| row.get(0),
        )?;

        Ok(MismatchReport {
            days,
            total_events,
            total_signals,
            categories: category_rows,
            signals: signal_rows,
            unresolved,
        })
    }

    /// Get recent events for a specific category.
    pub fn get_events_by_category(
        &self,
        category: &str,
        limit: usize,
    ) -> Result<Vec<UnresolvedEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, category, severity, detected, actual, input_snippet, context
             FROM mismatch_events
             WHERE category = ?1
             ORDER BY timestamp DESC
             LIMIT ?2",
        )?;
        let events = stmt
            .query_map(rusqlite::params![category, limit], |row| {
                Ok(UnresolvedEvent {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    category: row.get(2)?,
                    severity: row.get(3)?,
                    detected: row.get(4)?,
                    actual: row.get(5)?,
                    input_snippet: row.get(6)?,
                    context: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(events)
    }

    /// Clean old records.
    pub fn cleanup(&self, days: u32) -> Result<usize> {
        let offset = format!("-{} days", days);
        let d1 = self.conn.execute(
            "DELETE FROM mismatch_events WHERE timestamp < datetime('now', ?1)",
            [&offset],
        )?;
        let d2 = self.conn.execute(
            "DELETE FROM mismatch_signals WHERE timestamp < datetime('now', ?1)",
            [&offset],
        )?;
        Ok(d1 + d2)
    }
}

fn default_mismatch_db_path() -> Result<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        dirs_next::data_local_dir().unwrap_or_else(|| PathBuf::from(".local/share"))
    };
    Ok(base.join("zeroctx").join("mismatch.db"))
}

fn truncate_snippet(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...[truncated]", &s[..max_len])
    }
}

// --- Report types ---

#[derive(Debug, Clone)]
pub struct MismatchReport {
    pub days: u32,
    pub total_events: usize,
    pub total_signals: usize,
    pub categories: Vec<CategoryBreakdown>,
    pub signals: Vec<SignalSummary>,
    pub unresolved: Vec<UnresolvedEvent>,
}

#[derive(Debug, Clone)]
pub struct CategoryBreakdown {
    pub category: String,
    pub severity: String,
    pub count: usize,
    pub examples: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SignalSummary {
    pub signal_type: String,
    pub count: usize,
    pub operations: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UnresolvedEvent {
    pub id: i64,
    pub timestamp: String,
    pub category: String,
    pub severity: String,
    pub detected: String,
    pub actual: String,
    pub input_snippet: String,
    pub context: String,
}

/// Format a mismatch report for terminal display.
pub fn format_report(report: &MismatchReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "=== ZeroCTX Mismatch Report (last {} days) ===\n\n",
        report.days
    ));
    out.push_str(&format!(
        "Total events: {}  |  Total signals: {}\n\n",
        report.total_events, report.total_signals
    ));

    // Category breakdown
    if !report.categories.is_empty() {
        out.push_str("--- By Category ---\n\n");
        out.push_str(&format!(
            "  {:20} {:8} {:>6}  {}\n",
            "Category", "Severity", "Count", "Examples"
        ));
        out.push_str(&format!("  {:20} {:8} {:>6}  {}\n", "--------", "--------", "-----", "--------"));
        for c in &report.categories {
            let examples = c
                .examples
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect::<String>();
            out.push_str(&format!(
                "  {:20} {:8} {:>6}  {}\n",
                c.category, c.severity, c.count, examples
            ));
        }
        out.push('\n');
    }

    // Signals
    if !report.signals.is_empty() {
        out.push_str("--- Indirect Signals ---\n\n");
        out.push_str(&format!(
            "  {:25} {:>6}  {}\n",
            "Signal", "Count", "Operations"
        ));
        out.push_str(&format!("  {:25} {:>6}  {}\n", "------", "-----", "----------"));
        for s in &report.signals {
            let ops = s
                .operations
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(50)
                .collect::<String>();
            out.push_str(&format!("  {:25} {:>6}  {}\n", s.signal_type, s.count, ops));
        }
        out.push('\n');
    }

    // Unresolved
    if !report.unresolved.is_empty() {
        out.push_str("--- Top Unresolved (newest first) ---\n\n");
        for e in &report.unresolved {
            let sev_icon = match e.severity.as_str() {
                "error" => "ERR",
                "warn" => "WRN",
                _ => "INF",
            };
            out.push_str(&format!(
                "  [{}] #{} {} [{}]\n",
                sev_icon, e.id, e.timestamp, e.category
            ));
            out.push_str(&format!("       Detected: {}\n", e.detected));
            if !e.actual.is_empty() {
                out.push_str(&format!("       Actual:   {}\n", e.actual));
            }
            if !e.input_snippet.is_empty() {
                let snippet: String = e.input_snippet.chars().take(80).collect();
                out.push_str(&format!("       Input:    {}\n", snippet));
            }
            if !e.context.is_empty() {
                let ctx: String = e.context.chars().take(80).collect();
                out.push_str(&format!("       Context:  {}\n", ctx));
            }
            out.push('\n');
        }
    }

    // Actionable advice
    out.push_str("--- Actionable Insights ---\n\n");
    let mut has_advice = false;

    for c in &report.categories {
        if c.count >= 5 {
            has_advice = true;
            match c.category.as_str() {
                "intent_routing" => {
                    out.push_str(&format!(
                        "  ! {} intent routing mismatches. Review router regex patterns.\n",
                        c.count
                    ));
                    out.push_str("    Run: zero mismatch --category intent_routing --limit 10\n\n");
                }
                "output_filter" => {
                    out.push_str(&format!(
                        "  ! {} filter mismatches. Some filters may be too aggressive.\n",
                        c.count
                    ));
                    out.push_str("    Run: zero mismatch --category output_filter --limit 10\n\n");
                }
                "autofix" => {
                    out.push_str(&format!(
                        "  ! {} auto-fix mismatches. Patterns may need refinement.\n",
                        c.count
                    ));
                    out.push_str("    Run: zero mismatch --category autofix --limit 10\n\n");
                }
                "compression" => {
                    out.push_str(&format!(
                        "  ! {} compression mismatches. AST extraction may be too aggressive.\n",
                        c.count
                    ));
                    out.push_str("    Run: zero mismatch --category compression --limit 10\n\n");
                }
                _ => {}
            }
        }
    }

    for s in &report.signals {
        if s.count >= 3 {
            has_advice = true;
            match s.signal_type.as_str() {
                "retry_same_command" => {
                    out.push_str(&format!(
                        "  ! {} command retries detected. Output filters may strip too much.\n",
                        s.count
                    ));
                }
                "retry_same_file" => {
                    out.push_str(&format!(
                        "  ! {} file re-reads detected. Compression may be too aggressive.\n",
                        s.count
                    ));
                }
                "autofix_rerun" => {
                    out.push_str(&format!(
                        "  ! {} auto-fix reruns. Fixes may not be working correctly.\n",
                        s.count
                    ));
                }
                "fallback_used" => {
                    out.push_str(&format!(
                        "  ! {} AST fallbacks to regex. Consider adding tree-sitter support.\n",
                        s.count
                    ));
                }
                _ => {}
            }
        }
    }

    if !has_advice {
        out.push_str("  No significant patterns detected yet. Keep running to collect data.\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_report() -> Result<()> {
        let tracker = MismatchTracker::in_memory()?;

        tracker.record(&MismatchEvent {
            category: MismatchCategory::IntentRouting,
            severity: MismatchSeverity::Warn,
            detected: "RunAndDebug".into(),
            actual: "ReadAndRefactor".into(),
            input_snippet: "read src/auth.rs and run tests".into(),
            context: "has both 'read' and 'run' signals".into(),
            user_feedback: None,
        })?;

        tracker.record(&MismatchEvent {
            category: MismatchCategory::AutoFix,
            severity: MismatchSeverity::Error,
            detected: "module_not_found → pip install foo".into(),
            actual: "fix command failed, error persists".into(),
            input_snippet: "ModuleNotFoundError: No module named 'foo'".into(),
            context: "pip install foo returned exit 1".into(),
            user_feedback: None,
        })?;

        tracker.record_signal("retry_same_command", "cargo test", "{}")?;

        let report = tracker.get_report(30)?;
        assert_eq!(report.total_events, 2);
        assert_eq!(report.total_signals, 1);
        assert_eq!(report.unresolved.len(), 2);

        Ok(())
    }

    #[test]
    fn test_feedback_and_resolve() -> Result<()> {
        let tracker = MismatchTracker::in_memory()?;

        let id = tracker.record(&MismatchEvent {
            category: MismatchCategory::OutputFilter,
            severity: MismatchSeverity::Warn,
            detected: "GitFilter".into(),
            actual: "stripped important diff context".into(),
            input_snippet: "git diff HEAD~3".into(),
            context: "".into(),
            user_feedback: None,
        })?;

        tracker.add_feedback(id, "need full diff for large refactors")?;

        let report = tracker.get_report(30)?;
        assert_eq!(report.unresolved.len(), 0); // resolved by feedback

        Ok(())
    }
}
