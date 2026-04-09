use anyhow::Result;
use once_cell::sync::Lazy;
use regex::RegexSet;

use crate::config::Config;

/// Result of command rewriting.
pub enum RewriteResult {
    /// Command was rewritten — auto-allow execution
    Rewritten(String),
    /// No ZeroCTX equivalent — pass through unchanged
    Passthrough,
    /// Command should be denied
    Deny,
    /// Command rewritten but user should confirm (ask mode)
    Ask(String),
}

/// 70+ commands that ZeroCTX can optimize.
static REWRITE_PATTERNS: Lazy<RegexSet> = Lazy::new(|| {
    RegexSet::new([
        // Git (7)
        r"^git\s+(status|diff|log|show|branch|stash|worktree)",
        r"^git\s+(add|commit|push|pull|fetch|merge|rebase|cherry-pick|checkout|switch)",
        r"^git\s+(tag|remote|clean|reset|bisect|blame|shortlog)",
        r"^gh\s+(pr|issue|run|repo|release|gist)",
        // Python (8)
        r"^(?:python|python3)\s+-m\s+pytest",
        r"^pytest\b",
        r"^ruff\s+(check|format)",
        r"^mypy\b",
        r"^pip\s+(list|show|outdated|install|freeze)",
        r"^(?:python|python3)\s+-m\s+(pip|ruff|mypy|black|isort|pylint|flake8)",
        r"^(?:black|isort|pylint|flake8|bandit)\b",
        r"^(?:uv|pipx|poetry|pdm)\s+",
        // JavaScript/TypeScript (15)
        r"^npm\s+(run|test|build|list|outdated|install|ci|start|exec)",
        r"^npx\s+",
        r"^pnpm\s+(run|test|build|list|install|add|exec)",
        r"^yarn\s+(run|test|build|list|install|add)",
        r"^bun\s+(run|test|build|install|add)",
        r"^tsc\b",
        r"^eslint\b",
        r"^biome\s+(check|format|lint)",
        r"^jest\b",
        r"^vitest\b",
        r"^prettier\b",
        r"^next\s+(build|dev|start|lint)",
        r"^playwright\s+(test|show-report)",
        r"^prisma\s+(generate|migrate|db|studio)",
        r"^webpack\b",
        // .NET (6)
        r"^dotnet\s+(build|test|run|format|restore|publish|pack|clean|watch)",
        r"^dotnet\s+ef\s+",
        r"^msbuild\b",
        r"^nuget\s+(list|restore|install)",
        r"^dotnet\s+tool\s+",
        r"^dotnet\s+add\s+",
        // Rust (5)
        r"^cargo\s+(build|test|check|clippy|fmt|run|bench|doc|publish|install|update|tree)",
        r"^rustfmt\b",
        r"^rustc\b",
        r"^cargo-watch\b",
        r"^wasm-pack\b",
        // Go (3)
        r"^go\s+(build|test|vet|run|install|mod|generate|fmt)",
        r"^golangci-lint\b",
        r"^gofmt\b",
        // Ruby (3)
        r"^(?:rake|rails)\s+",
        r"^rspec\b",
        r"^rubocop\b",
        // System (12)
        r"^(?:ls|dir)\b",
        r"^tree\b",
        r"^(?:cat|head|tail|less|more)\b",
        r"^(?:grep|rg|ag|ack)\b",
        r"^(?:find|fd)\b",
        r"^wc\b",
        r"^(?:env|printenv)\b",
        r"^(?:ps|top|htop)\b",
        r"^(?:df|du)\b",
        r"^(?:which|where|type)\b",
        r"^(?:file|stat)\b",
        r"^(?:diff|cmp)\b",
        // Network (3)
        r"^curl\b",
        r"^wget\b",
        r"^(?:ping|traceroute|nslookup|dig)\b",
        // Build tools (5)
        r"^make\b",
        r"^cmake\b",
        r"^(?:gradle|gradlew)\b",
        r"^(?:mvn|mvnw)\b",
        r"^(?:ant)\b",
    ])
    .expect("valid regex set")
});

/// Check for potentially dangerous redirections (not inside quotes).
fn has_redirect(command: &str) -> bool {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut prev = ' ';
    let chars: Vec<char> = command.chars().collect();

    for i in 0..chars.len() {
        let ch = chars[i];
        if ch == '\'' && !in_double_quote && prev != '\\' {
            in_single_quote = !in_single_quote;
        } else if ch == '"' && !in_single_quote && prev != '\\' {
            in_double_quote = !in_double_quote;
        }

        if !in_single_quote && !in_double_quote {
            match ch {
                '>' | '<' => return true,
                '$' if i + 1 < chars.len() && chars[i + 1] == '(' => return true,
                '`' => return true,
                _ => {}
            }
        }
        prev = ch;
    }
    false
}

/// Split a compound command into parts by `&&`, `||`, `;`.
/// Respects quoted strings.
fn split_compound(command: &str) -> Vec<(String, &str)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut prev = ' ';
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if ch == '\'' && !in_double && prev != '\\' {
            in_single = !in_single;
            current.push(ch);
        } else if ch == '"' && !in_single && prev != '\\' {
            in_double = !in_double;
            current.push(ch);
        } else if !in_single && !in_double {
            // Check for &&
            if ch == '&' && i + 1 < chars.len() && chars[i + 1] == '&' {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    parts.push((trimmed, "&&"));
                }
                current.clear();
                i += 2;
                continue;
            }
            // Check for ||
            if ch == '|' && i + 1 < chars.len() && chars[i + 1] == '|' {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    parts.push((trimmed, "||"));
                }
                current.clear();
                i += 2;
                continue;
            }
            // Check for ; (but not inside $(...))
            if ch == ';' {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    parts.push((trimmed, ";"));
                }
                current.clear();
                i += 1;
                continue;
            }
            // Single pipe — only rewrite left side
            if ch == '|' && (i + 1 >= chars.len() || chars[i + 1] != '|') {
                // Don't split pipes — treat as single command
                current.push(ch);
            } else {
                current.push(ch);
            }
        } else {
            current.push(ch);
        }

        prev = ch;
        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        parts.push((trimmed, "")); // Last part has no separator
    }

    parts
}

/// Rewrite a command for ZeroCTX optimization.
/// Supports compound commands (&&, ||, ;) by rewriting each part independently.
pub fn rewrite(command: &str) -> Result<RewriteResult> {
    let config = Config::load().unwrap_or_default();
    rewrite_with_config(command, &config)
}

pub fn rewrite_with_config(command: &str, config: &Config) -> Result<RewriteResult> {
    let trimmed = command.trim();

    // Check deny list first (highest priority)
    for pattern in &config.hooks.deny_commands {
        if glob_match(pattern, trimmed) {
            return Ok(RewriteResult::Deny);
        }
    }

    // Check ask list
    let needs_ask = config.hooks.ask_commands.iter().any(|p| glob_match(p, trimmed));

    // Don't rewrite commands with redirections
    if has_redirect(trimmed) {
        return Ok(RewriteResult::Passthrough);
    }

    // Check for compound commands BEFORE exclude check
    // (so "ssh deploy && cargo test" rewrites cargo test even though ssh is excluded)
    let parts = split_compound(trimmed);
    if parts.len() > 1 {
        return rewrite_compound(&parts, needs_ask, config);
    }

    // Single command: check exclude list
    let first_word = trimmed.split_whitespace().next().unwrap_or("");
    if config.hooks.exclude_commands.iter().any(|e| e == first_word) {
        return Ok(RewriteResult::Passthrough);
    }

    // Single command: check pattern match
    if REWRITE_PATTERNS.is_match(trimmed) {
        let rewritten = format!("zero rewrite-exec -- {}", trimmed);
        if needs_ask {
            Ok(RewriteResult::Ask(rewritten))
        } else {
            Ok(RewriteResult::Rewritten(rewritten))
        }
    } else {
        Ok(RewriteResult::Passthrough)
    }
}

/// Rewrite a compound command (e.g., `git status && cargo test`).
/// Each part is independently checked and rewritten.
fn rewrite_compound(parts: &[(String, &str)], needs_ask: bool, config: &Config) -> Result<RewriteResult> {
    let mut rewritten_parts = Vec::new();
    let mut any_rewritten = false;

    for (cmd, separator) in parts {
        let first_word = cmd.split_whitespace().next().unwrap_or("");
        let excluded = config.hooks.exclude_commands.iter().any(|e| e == first_word);

        let rewritten = if !excluded && REWRITE_PATTERNS.is_match(cmd) {
            any_rewritten = true;
            format!("zero rewrite-exec -- {}", cmd)
        } else {
            cmd.clone()
        };

        if separator.is_empty() {
            rewritten_parts.push(rewritten);
        } else {
            rewritten_parts.push(format!("{} {}", rewritten, separator));
        }
    }

    if any_rewritten {
        let full = rewritten_parts.join(" ");
        if needs_ask {
            Ok(RewriteResult::Ask(full))
        } else {
            Ok(RewriteResult::Rewritten(full))
        }
    } else {
        Ok(RewriteResult::Passthrough)
    }
}

/// Simple glob matching: `*` matches any characters.
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return text.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return text.ends_with(suffix);
    }
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return text.starts_with(parts[0]) && text.ends_with(parts[1]);
        }
    }
    pattern == text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_rewrite() {
        match rewrite("git status").unwrap() {
            RewriteResult::Rewritten(cmd) => assert!(cmd.contains("zero rewrite-exec")),
            _ => panic!("Expected rewrite"),
        }
    }

    #[test]
    fn test_passthrough() {
        match rewrite("ssh user@host").unwrap() {
            RewriteResult::Passthrough => {}
            _ => panic!("Expected passthrough"),
        }
    }

    #[test]
    fn test_redirect_passthrough() {
        match rewrite("git log > output.txt").unwrap() {
            RewriteResult::Passthrough => {}
            _ => panic!("Expected passthrough for redirect"),
        }
    }

    #[test]
    fn test_compound_and() {
        match rewrite("git status && cargo test").unwrap() {
            RewriteResult::Rewritten(cmd) => {
                assert!(cmd.contains("zero rewrite-exec -- git status"));
                assert!(cmd.contains("zero rewrite-exec -- cargo test"));
                assert!(cmd.contains("&&"));
            }
            _ => panic!("Expected compound rewrite"),
        }
    }

    #[test]
    fn test_compound_semicolon() {
        match rewrite("cargo build; cargo test").unwrap() {
            RewriteResult::Rewritten(cmd) => {
                assert!(cmd.contains("zero rewrite-exec -- cargo build"));
                assert!(cmd.contains("zero rewrite-exec -- cargo test"));
            }
            _ => panic!("Expected compound rewrite"),
        }
    }

    #[test]
    fn test_compound_mixed() {
        // ssh doesn't match, cargo does
        match rewrite("ssh deploy && cargo test").unwrap() {
            RewriteResult::Rewritten(cmd) => {
                assert!(cmd.contains("ssh deploy")); // not rewritten
                assert!(cmd.contains("zero rewrite-exec -- cargo test")); // rewritten
            }
            _ => panic!("Expected partial rewrite"),
        }
    }

    #[test]
    fn test_quoted_not_split() {
        match rewrite(r#"echo "hello && world""#).unwrap() {
            RewriteResult::Passthrough => {} // echo not in our patterns
            _ => panic!("Expected passthrough — && is inside quotes"),
        }
    }

    #[test]
    fn test_expanded_patterns() {
        // Go
        match rewrite("go test ./...").unwrap() {
            RewriteResult::Rewritten(_) => {}
            _ => panic!("Expected go test rewrite"),
        }
        // Make
        match rewrite("make build").unwrap() {
            RewriteResult::Rewritten(_) => {}
            _ => panic!("Expected make rewrite"),
        }
        // Bun
        match rewrite("bun test").unwrap() {
            RewriteResult::Rewritten(_) => {}
            _ => panic!("Expected bun rewrite"),
        }
    }
}
