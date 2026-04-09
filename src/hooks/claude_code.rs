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
    match crate::compression::compress_to_temp(file_path, &config) {
        Ok(temp_path) => {
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
