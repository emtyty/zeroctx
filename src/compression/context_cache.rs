use anyhow::{anyhow, Result};
use md5::{Digest, Md5};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
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
    /// Runs the 30-day cleanup. Prefer `with_shared` for the process-wide
    /// singleton, which runs cleanup only once.
    pub fn open_default() -> Result<Self> {
        Self::open_at(&cache_db_path()?, /* cleanup = */ true)
    }

    fn open_at(db_path: &Path, cleanup: bool) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
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
        if cleanup {
            // Skip cleanup if the clock is unreadable - otherwise we'd delete every row
            // (last_seen < negative_threshold matches everything).
            if let Some(now) = unix_now() {
                let cutoff = now - 30 * 24 * 3600;
                if cutoff > 0 {
                    conn.execute(
                        "DELETE FROM file_cache WHERE last_seen < ?1",
                        [cutoff],
                    )?;
                }
            } else {
                tracing::warn!(
                    "context_cache: system clock unreadable, skipping 30-day cleanup"
                );
            }
        }
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
    /// Errors if the cache is in-memory-only (no SQLite connection).
    pub fn store_compressed(&self, path: &str, content: &str, compressed: &str) -> Result<()> {
        let conn = self
            .conn
            .as_ref()
            .ok_or_else(|| anyhow!("context_cache: no persistent connection (in-memory only)"))?;
        let hash = Self::hash(content);
        let summary = Self::generate_summary(path, content);
        let mtime = get_mtime(path) as i64;
        let now = unix_now().unwrap_or(0);
        conn.execute(
            "INSERT OR REPLACE INTO file_cache (path, hash, mtime, compressed, summary, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![path, hash, mtime, compressed, summary, now],
        )?;
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

/// Process-wide cache singleton. First caller pays the open + cleanup cost;
/// subsequent callers reuse the same connection. On open failure the singleton
/// stays empty, a warning is logged once, and `with_shared` becomes a no-op so
/// callers don't need to special-case it.
static SHARED: OnceLock<Option<Mutex<ContextCache>>> = OnceLock::new();

fn shared_cell() -> &'static Option<Mutex<ContextCache>> {
    SHARED.get_or_init(|| match ContextCache::open_default() {
        Ok(cache) => Some(Mutex::new(cache)),
        Err(e) => {
            tracing::warn!(
                "context_cache: failed to open persistent cache, running without it: {}",
                e
            );
            None
        }
    })
}

/// Run a closure against the shared cache. Returns `None` if the persistent
/// cache is unavailable (so callers can skip rather than retry-then-fail).
pub fn with_shared<R>(f: impl FnOnce(MutexGuard<'_, ContextCache>) -> R) -> Option<R> {
    let cell = shared_cell().as_ref()?;
    match cell.lock() {
        Ok(guard) => Some(f(guard)),
        Err(poisoned) => {
            tracing::warn!("context_cache: mutex poisoned, recovering");
            Some(f(poisoned.into_inner()))
        }
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

/// Current unix timestamp, or `None` if the system clock is unreadable.
/// Callers should treat `None` as "skip whatever time-based logic this is for"
/// rather than substituting 0 (which can make `last_seen < cutoff` match everything).
fn unix_now() -> Option<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
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
