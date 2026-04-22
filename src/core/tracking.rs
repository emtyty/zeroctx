use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::PathBuf;
use tracing::debug;

use crate::core::types::SavingsMethod;

/// SQLite-based token savings tracker with session support.
///
/// Unlike RTK's tracker which only records per-command stats,
/// this also maintains session state for context caching across rounds.
pub struct Tracker {
    conn: Connection,
}

impl Tracker {
    /// Open or create the tracking database.
    pub fn open(path: Option<&PathBuf>) -> Result<Self> {
        let conn = match path {
            Some(p) => {
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
                }
                Connection::open(p)
                    .with_context(|| format!("Failed to open database: {}", p.display()))?
            }
            None => {
                let default_path = default_db_path()?;
                if let Some(parent) = default_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                Connection::open(&default_path)
                    .with_context(|| format!("Failed to open database: {}", default_path.display()))?
            }
        };

        let tracker = Self { conn };
        tracker.init_tables()?;
        Ok(tracker)
    }

    /// Open an in-memory database (for testing).
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let tracker = Self { conn };
        tracker.init_tables()?;
        Ok(tracker)
    }

    fn init_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tracking (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now')),
                operation TEXT NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                savings_percent REAL NOT NULL,
                method TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS session_cache (
                file_path TEXT PRIMARY KEY,
                content_hash TEXT NOT NULL,
                summary TEXT NOT NULL,
                last_seen TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_tracking_timestamp ON tracking(timestamp);
            CREATE INDEX IF NOT EXISTS idx_tracking_method ON tracking(method);",
        )?;
        Ok(())
    }

    /// Record a token savings event.
    pub fn record(
        &self,
        operation: &str,
        input_tokens: usize,
        output_tokens: usize,
        method: SavingsMethod,
    ) -> Result<()> {
        let savings = if input_tokens > 0 {
            (1.0 - output_tokens as f64 / input_tokens as f64) * 100.0
        } else {
            0.0
        };

        let method_str = match method {
            SavingsMethod::OutputFilter => "output_filter",
            SavingsMethod::ErrorAutoFix => "error_autofix",
            SavingsMethod::AstCompression => "ast_compression",
            SavingsMethod::ContextCache => "context_cache",
            SavingsMethod::DiffOutput => "diff_output",
            SavingsMethod::None => "none",
        };

        debug!(
            operation,
            input_tokens,
            output_tokens,
            savings_percent = format!("{:.1}", savings),
            method = method_str,
            "Recording token savings"
        );

        self.conn.execute(
            "INSERT INTO tracking (operation, input_tokens, output_tokens, savings_percent, method)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![operation, input_tokens, output_tokens, savings, method_str],
        )?;

        Ok(())
    }

    /// Get summary statistics.
    pub fn get_summary(&self) -> Result<TrackingSummary> {
        let mut stmt = self.conn.prepare(
            "SELECT
                COUNT(*) as total_commands,
                COALESCE(SUM(input_tokens), 0) as total_input,
                COALESCE(SUM(output_tokens), 0) as total_output,
                COALESCE(AVG(savings_percent), 0) as avg_savings
             FROM tracking",
        )?;

        let summary = stmt.query_row([], |row| {
            Ok(TrackingSummary {
                total_commands: row.get(0)?,
                total_input_tokens: row.get(1)?,
                total_output_tokens: row.get(2)?,
                avg_savings_percent: row.get(3)?,
            })
        })?;

        Ok(summary)
    }

    /// Get savings breakdown by method.
    pub fn get_by_method(&self) -> Result<Vec<MethodStats>> {
        let mut stmt = self.conn.prepare(
            "SELECT method, COUNT(*), SUM(input_tokens - output_tokens), AVG(savings_percent)
             FROM tracking
             GROUP BY method
             ORDER BY SUM(input_tokens - output_tokens) DESC",
        )?;

        let results = stmt
            .query_map([], |row| {
                Ok(MethodStats {
                    method: row.get(0)?,
                    count: row.get(1)?,
                    tokens_saved: row.get(2)?,
                    avg_savings: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Get daily stats for the last N days.
    pub fn get_daily(&self, days: u32) -> Result<Vec<DailyStats>> {
        let mut stmt = self.conn.prepare(
            "SELECT date(timestamp), COUNT(*), SUM(input_tokens), SUM(output_tokens),
                    AVG(savings_percent)
             FROM tracking
             WHERE timestamp >= datetime('now', ?1)
             GROUP BY date(timestamp)
             ORDER BY date(timestamp) DESC",
        )?;

        let offset = format!("-{} days", days);
        let results = stmt
            .query_map([offset], |row| {
                Ok(DailyStats {
                    date: row.get(0)?,
                    commands: row.get(1)?,
                    input_tokens: row.get(2)?,
                    output_tokens: row.get(3)?,
                    avg_savings: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    // --- Session Cache ---

    /// Update the file hash cache for context deduplication.
    pub fn cache_file(&self, path: &str, hash: &str, summary: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO session_cache (file_path, content_hash, summary, last_seen)
             VALUES (?1, ?2, ?3, datetime('now'))",
            rusqlite::params![path, hash, summary],
        )?;
        Ok(())
    }

    /// Check if a file's content has changed since last cached.
    pub fn get_cached_hash(&self, path: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT content_hash FROM session_cache WHERE file_path = ?1")?;

        let result = stmt
            .query_row([path], |row| row.get(0))
            .ok();

        Ok(result)
    }

    /// Get cached summary for a file.
    pub fn get_cached_summary(&self, path: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT summary FROM session_cache WHERE file_path = ?1")?;

        let result = stmt
            .query_row([path], |row| row.get(0))
            .ok();

        Ok(result)
    }

    /// Clean up old tracking records.
    pub fn cleanup(&self, days: u32) -> Result<usize> {
        let offset = format!("-{} days", days);
        let deleted = self.conn.execute(
            "DELETE FROM tracking WHERE timestamp < datetime('now', ?1)",
            [offset],
        )?;
        Ok(deleted)
    }

    /// Get session summary for the last N hours.
    pub fn get_session_summary(&self, hours: u32) -> Result<SessionSummary> {
        let offset = format!("-{} hours", hours);
        let mut stmt = self.conn.prepare(
            "SELECT
                COUNT(*) as commands,
                COALESCE(SUM(input_tokens), 0) as total_input,
                COALESCE(SUM(output_tokens), 0) as total_output,
                COALESCE(SUM(input_tokens - output_tokens), 0) as tokens_saved,
                MIN(timestamp) as session_start
             FROM tracking
             WHERE timestamp >= datetime('now', ?1)",
        )?;

        let result = stmt.query_row([offset], |row| {
            Ok(SessionSummary {
                commands_run: row.get(0)?,
                total_input_tokens: row.get(1)?,
                total_output_tokens: row.get(2)?,
                tokens_saved: row.get(3).unwrap_or(0),
                session_start: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                hours,
            })
        })?;

        Ok(result)
    }
}

fn default_db_path() -> Result<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        dirs_next::data_local_dir().unwrap_or_else(|| PathBuf::from(".local/share"))
    };
    Ok(base.join("zeroctx").join("tracking.db"))
}

#[derive(Debug, Clone)]
pub struct TrackingSummary {
    pub total_commands: usize,
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub avg_savings_percent: f64,
}

#[derive(Debug, Clone)]
pub struct MethodStats {
    pub method: String,
    pub count: usize,
    pub tokens_saved: i64,
    pub avg_savings: f64,
}

#[derive(Debug, Clone)]
pub struct DailyStats {
    pub date: String,
    pub commands: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub avg_savings: f64,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub commands_run: usize,
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub tokens_saved: i64,
    pub session_start: String,
    pub hours: u32,
}

impl SessionSummary {
    /// Estimated cost in USD (rough Claude Sonnet pricing).
    pub fn estimated_cost_usd(&self) -> f64 {
        // Claude Sonnet: ~$3/M input, $15/M output (rough average)
        let input_cost = self.total_input_tokens as f64 * 3.0 / 1_000_000.0;
        let output_cost = self.total_output_tokens as f64 * 15.0 / 1_000_000.0;
        input_cost + output_cost
    }

    /// Estimated cost without ZeroCTX (using input_tokens as the "would have been" baseline).
    pub fn estimated_cost_without_usd(&self) -> f64 {
        let total = self.total_input_tokens as i64 + self.tokens_saved;
        let input_cost = total.max(0) as f64 * 3.0 / 1_000_000.0;
        let output_cost = self.total_output_tokens as f64 * 15.0 / 1_000_000.0;
        input_cost + output_cost
    }

    pub fn savings_percent(&self) -> f64 {
        let total_would_be = self.total_input_tokens as i64 + self.tokens_saved;
        if total_would_be > 0 {
            self.tokens_saved as f64 / total_would_be as f64 * 100.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_summary() -> Result<()> {
        let tracker = Tracker::in_memory()?;
        tracker.record("git status", 1000, 300, SavingsMethod::OutputFilter)?;
        tracker.record("pytest", 5000, 500, SavingsMethod::OutputFilter)?;
        tracker.record("pip install", 2000, 0, SavingsMethod::ErrorAutoFix)?;

        let summary = tracker.get_summary()?;
        assert_eq!(summary.total_commands, 3);
        assert_eq!(summary.total_input_tokens, 8000);
        assert_eq!(summary.total_output_tokens, 800);
        assert!(summary.avg_savings_percent > 50.0);

        Ok(())
    }

    #[test]
    fn test_cache_file() -> Result<()> {
        let tracker = Tracker::in_memory()?;
        tracker.cache_file("src/auth.rs", "abc123", "auth.rs: 150 lines, 4 functions")?;

        assert_eq!(
            tracker.get_cached_hash("src/auth.rs")?,
            Some("abc123".to_string())
        );
        assert_eq!(
            tracker.get_cached_summary("src/auth.rs")?,
            Some("auth.rs: 150 lines, 4 functions".to_string())
        );
        assert_eq!(tracker.get_cached_hash("nonexistent.rs")?, None);

        Ok(())
    }
}
