use anyhow::Result;
use md5::{Digest, Md5};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// MD5-based file deduplication cache with optional SQLite persistence.
///
/// In-memory layer handles same-session dedup.
/// Persistent layer handles cross-session: if file mtime hasn't changed,
/// return cached compressed content without re-running tree-sitter.
pub struct ContextCache {
    hashes: HashMap<String, String>,
    summaries: HashMap<String, String>,
    conn: Option<Connection>,
}

impl ContextCache {
    pub fn new() -> Self {
        Self {
            hashes: HashMap::new(),
            summaries: HashMap::new(),
            conn: None,
        }
    }

    /// Open a persistent cache backed by SQLite at the default zeroctx directory.
    pub fn open_default() -> Result<Self> {
        let db_path = cache_db_path()?;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS file_cache (
                path TEXT PRIMARY KEY,
                hash TEXT NOT NULL,
                mtime INTEGER NOT NULL DEFAULT 0,
                compressed TEXT NOT NULL,
                summary TEXT NOT NULL,
                last_seen INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_file_cache_path ON file_cache(path);",
        )?;
        // Remove entries older than 30 days
        conn.execute(
            "DELETE FROM file_cache WHERE last_seen < ?1",
            [unix_now() - 30 * 24 * 3600],
        )?;
        Ok(Self {
            hashes: HashMap::new(),
            summaries: HashMap::new(),
            conn: Some(conn),
        })
    }

    /// Fast mtime-based cache check. If file mtime matches cached entry,
    /// return stored compressed content without reading the file.
    pub fn check_mtime(&self, path: &str) -> Result<Option<String>> {
        let current_mtime = get_mtime(path);
        if current_mtime == 0 {
            return Ok(None);
        }
        if let Some(ref conn) = self.conn {
            let result: Option<(i64, String)> = conn
                .query_row(
                    "SELECT mtime, compressed FROM file_cache WHERE path = ?1",
                    [path],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok();
            if let Some((cached_mtime, compressed)) = result {
                if cached_mtime == current_mtime as i64 {
                    return Ok(Some(compressed));
                }
            }
        }
        Ok(None)
    }

    /// Store compressed content with mtime for future cache hits.
    pub fn store_compressed(&self, path: &str, content: &str, compressed: &str) -> Result<()> {
        let hash = Self::hash(content);
        let summary = Self::generate_summary(path, content);
        let mtime = get_mtime(path) as i64;
        let now = unix_now();
        if let Some(ref conn) = self.conn {
            conn.execute(
                "INSERT OR REPLACE INTO file_cache (path, hash, mtime, compressed, summary, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![path, hash, mtime, compressed, summary, now],
            )?;
        }
        Ok(())
    }

    /// Check if a file has changed since last seen (in-memory hash check).
    /// Returns (is_changed, optional_summary_if_unchanged).
    pub fn check(&self, path: &str, content: &str) -> (bool, Option<&str>) {
        let new_hash = Self::hash(content);
        match self.hashes.get(path) {
            Some(old_hash) if *old_hash == new_hash => {
                let summary = self.summaries.get(path).map(|s| s.as_str());
                (false, summary)
            }
            _ => (true, None),
        }
    }

    /// Update the in-memory cache with a file's current content.
    pub fn update(&mut self, path: &str, content: &str) {
        let hash = Self::hash(content);
        let summary = Self::generate_summary(path, content);
        self.hashes.insert(path.to_string(), hash);
        self.summaries.insert(path.to_string(), summary);
    }

    fn generate_summary(path: &str, content: &str) -> String {
        let line_count = content.lines().count();
        let filename = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);
        let func_count = content
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.starts_with("def ")
                    || t.starts_with("fn ")
                    || t.starts_with("function ")
                    || t.starts_with("public ")
                    || (t.starts_with("class ") && !t.contains("className"))
            })
            .count();
        if func_count > 0 {
            format!("{}: {} lines, {} definitions", filename, line_count, func_count)
        } else {
            format!("{}: {} lines", filename, line_count)
        }
    }

    fn hash(content: &str) -> String {
        let mut hasher = Md5::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

impl Default for ContextCache {
    fn default() -> Self {
        Self::new()
    }
}

fn cache_db_path() -> Result<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        dirs_next::data_local_dir().unwrap_or_else(|| PathBuf::from(".local/share"))
    };
    Ok(base.join("zeroctx").join("cache.db"))
}

fn get_mtime(path: &str) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unchanged_file() {
        let mut cache = ContextCache::new();
        let content = "def hello():\n    print('hello')\n";
        cache.update("test.py", content);
        let (changed, summary) = cache.check("test.py", content);
        assert!(!changed);
        assert!(summary.is_some());
        assert!(summary.unwrap().contains("test.py"));
    }

    #[test]
    fn test_changed_file() {
        let mut cache = ContextCache::new();
        cache.update("test.py", "version 1");
        let (changed, _) = cache.check("test.py", "version 2");
        assert!(changed);
    }

    #[test]
    fn test_new_file() {
        let cache = ContextCache::new();
        let (changed, summary) = cache.check("new.py", "content");
        assert!(changed);
        assert!(summary.is_none());
    }
}
