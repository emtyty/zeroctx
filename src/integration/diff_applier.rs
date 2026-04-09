use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;

/// Extract unified diff blocks from Claude's response.
pub fn extract_diffs(response: &str) -> Vec<DiffBlock> {
    static DIFF_HEADER: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^---\s+a/(.+)$").expect("valid regex"));
    static HUNK_HEADER: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^@@\s+-(\d+),?(\d*)\s+\+(\d+),?(\d*)\s+@@").expect("valid regex"));

    let mut diffs = Vec::new();
    let mut current_file: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut in_diff = false;

    for line in response.lines() {
        if let Some(caps) = DIFF_HEADER.captures(line) {
            // Save previous diff
            if let Some(file) = current_file.take() {
                if !current_lines.is_empty() {
                    diffs.push(DiffBlock {
                        file_path: file,
                        content: current_lines.join("\n"),
                    });
                    current_lines.clear();
                }
            }
            current_file = Some(caps.get(1).map_or("", |m| m.as_str()).to_string());
            current_lines.push(line.to_string());
            in_diff = true;
        } else if in_diff
            && (line.starts_with('+')
                || line.starts_with('-')
                || line.starts_with(' ')
                || line.starts_with("@@")
                || line.starts_with("+++"))
        {
            current_lines.push(line.to_string());
        } else if in_diff && !line.starts_with("```") {
            // End of diff block
            if let Some(file) = current_file.take() {
                if !current_lines.is_empty() {
                    diffs.push(DiffBlock {
                        file_path: file,
                        content: current_lines.join("\n"),
                    });
                    current_lines.clear();
                }
            }
            in_diff = false;
        }
    }

    // Handle last diff
    if let Some(file) = current_file {
        if !current_lines.is_empty() {
            diffs.push(DiffBlock {
                file_path: file,
                content: current_lines.join("\n"),
            });
        }
    }

    diffs
}

/// A single diff block targeting one file.
#[derive(Debug, Clone)]
pub struct DiffBlock {
    pub file_path: String,
    pub content: String,
}

impl DiffBlock {
    /// Apply this diff using the `patch` command.
    pub fn apply(&self) -> Result<String> {
        use std::io::Write;
        use std::process::Command;

        let mut child = Command::new("patch")
            .args(["-p1", "--no-backup-if-mismatch"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(self.content.as_bytes())?;
        }

        let output = child.wait_with_output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!("Applied diff to {}: {}", self.file_path, stdout))
        } else {
            anyhow::bail!(
                "Failed to apply diff to {}: {}",
                self.file_path,
                stderr
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_diffs() {
        let response = r#"Here's the fix:

```diff
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -42,3 +42,3 @@
-    if token.expired {
+    if token.expired || token.revoked {
```

This adds a check for revoked tokens."#;

        let diffs = extract_diffs(response);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].file_path, "src/auth.rs");
        assert!(diffs[0].content.contains("token.revoked"));
    }
}
