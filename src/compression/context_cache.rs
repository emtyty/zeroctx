use md5::{Digest, Md5};
use std::collections::HashMap;
use std::path::Path;

/// MD5-based file deduplication cache.
///
/// On round 2+, unchanged files become one-line summaries.
/// Savings: 70%+ on iterative sessions.
pub struct ContextCache {
    hashes: HashMap<String, String>,
    summaries: HashMap<String, String>,
}

impl ContextCache {
    pub fn new() -> Self {
        Self {
            hashes: HashMap::new(),
            summaries: HashMap::new(),
        }
    }

    /// Check if a file has changed since last seen.
    /// Returns (is_changed, optional_summary_if_unchanged).
    pub fn check(&self, path: &str, content: &str) -> (bool, Option<&str>) {
        let new_hash = Self::hash(content);
        match self.hashes.get(path) {
            Some(old_hash) if *old_hash == new_hash => {
                // Unchanged — return cached summary
                let summary = self.summaries.get(path).map(|s| s.as_str());
                (false, summary)
            }
            _ => (true, None),
        }
    }

    /// Update the cache with a file's current content.
    pub fn update(&mut self, path: &str, content: &str) {
        let hash = Self::hash(content);
        let summary = Self::generate_summary(path, content);
        self.hashes.insert(path.to_string(), hash);
        self.summaries.insert(path.to_string(), summary);
    }

    /// Generate a one-line summary of a file.
    fn generate_summary(path: &str, content: &str) -> String {
        let line_count = content.lines().count();
        let filename = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);

        // Count function/class definitions (rough heuristic)
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
