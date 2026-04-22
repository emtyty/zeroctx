// Claude Code PreToolUse Hook Protocol
//
// The actual hook execution is handled by shell scripts:
//   - ~/.claude/hooks/zeroctx-rewrite.sh  (Bash tool interceptor)
//   - ~/.claude/hooks/zeroctx-read.sh     (Read tool interceptor)
//
// This module provides Rust-native handlers that the shell scripts
// delegate to via `zero rewrite`, `zero rewrite-exec`, and `zero compress-read`.
//
// Protocol:
//   stdin:  {"tool_name":"Bash|Read","tool_input":{"command":"..."|"file_path":"..."}}
//   stdout: {"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{...}}}
//   exit 0: hook processed, check stdout for response
//   exit 1: no match, pass through
//
// Future: replace shell scripts with pure Rust hook handler by reading stdin JSON directly.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Input from Claude Code to the hook (stdin JSON).
#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
}

/// Output from the hook back to Claude Code (stdout JSON).
#[derive(Debug, Serialize)]
pub struct HookOutput {
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: HookSpecificOutput,
}

#[derive(Debug, Serialize)]
pub struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    #[serde(rename = "permissionDecision", skip_serializing_if = "Option::is_none")]
    pub permission_decision: Option<String>,
    #[serde(
        rename = "permissionDecisionReason",
        skip_serializing_if = "Option::is_none"
    )]
    pub permission_decision_reason: Option<String>,
    #[serde(rename = "updatedInput")]
    pub updated_input: serde_json::Value,
}

impl HookOutput {
    /// Create an auto-allow response with rewritten input.
    pub fn allow(updated_input: serde_json::Value, reason: &str) -> Self {
        Self {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "PreToolUse".to_string(),
                permission_decision: Some("allow".to_string()),
                permission_decision_reason: Some(reason.to_string()),
                updated_input,
            },
        }
    }

    /// Create a passthrough (no modification).
    pub fn passthrough() -> Option<Self> {
        None // Shell script exits 0 with no output
    }
}

/// Handle a Bash tool hook call (pure Rust, future replacement for shell script).
pub fn handle_bash(input: &HookInput) -> Result<Option<HookOutput>> {
    let command = input
        .tool_input
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if command.is_empty() {
        return Ok(None);
    }

    match crate::hooks::rewriter::rewrite(command)? {
        crate::hooks::rewriter::RewriteResult::Rewritten(rewritten) => {
            let mut updated = input.tool_input.clone();
            updated["command"] = serde_json::Value::String(rewritten);
            Ok(Some(HookOutput::allow(updated, "ZeroCTX auto-rewrite")))
        }
        _ => Ok(None),
    }
}

/// Handle a Read tool hook call (pure Rust, future replacement for shell script).
pub fn handle_read(input: &HookInput) -> Result<Option<HookOutput>> {
    let file_path = input
        .tool_input
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if file_path.is_empty() {
        return Ok(None);
    }

    let config = crate::config::Config::load()?;

    // Inject project brief on first file read in this session (flag via temp file)
    let brief_injected = should_inject_brief();

    match crate::compression::compress_to_temp(file_path, &config) {
        Ok(temp_path) => {
            // If brief should be injected, prepend it to the compressed file
            if brief_injected {
                if let Some(brief) = crate::compression::load_project_brief() {
                    if let Ok(compressed) = std::fs::read_to_string(&temp_path) {
                        let combined = format!("{}{}", brief, compressed);
                        std::fs::write(&temp_path, combined).ok();
                    }
                }
            }
            let mut updated = input.tool_input.clone();
            updated["file_path"] = serde_json::Value::String(temp_path);
            Ok(Some(HookOutput::allow(
                updated,
                "ZeroCTX AST compression",
            )))
        }
        Err(_) => Ok(None),
    }
}

/// Returns true (and marks the session) if the brief has not yet been injected today.
/// Uses a temp flag file to avoid injecting on every read.
fn should_inject_brief() -> bool {
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let flag_path = std::env::temp_dir().join(format!("zeroctx_brief_{}.flag", today));
    if flag_path.exists() {
        return false;
    }
    // Create flag file — this session gets the brief injected once
    std::fs::write(&flag_path, "").ok();
    true
}

/// Handle PostToolUse hook for Glob tool — compress file listing results.
pub fn handle_post_glob(tool_output: &str) -> String {
    crate::filters::system::compress_glob_results(tool_output)
}

/// Handle PostToolUse hook for Grep tool — compress search results.
pub fn handle_post_grep(tool_output: &str) -> String {
    crate::filters::system::compress_grep_results(tool_output)
}

/// Build a PostToolUse hook JSON response that replaces the tool output.
/// Claude Code PostToolUse protocol: return JSON with decision+reason to replace output.
pub fn post_tool_response(compressed: &str, original_len: usize, compressed_len: usize) -> String {
    let savings = if original_len > 0 && original_len > compressed_len {
        (original_len - compressed_len) * 100 / original_len
    } else {
        0
    };
    let reason = format!(
        "{}\n\n[ZeroCTX: {} → {} chars, ~{}% saved]",
        compressed, original_len, compressed_len, savings
    );
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "decision": "block",
            "reason": reason
        }
    })
    .to_string()
}
