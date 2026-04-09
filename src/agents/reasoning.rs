use anyhow::Result;

use crate::config::Config;
use crate::core::types::ParsedRequest;

/// System prompt that forces diff-only output format.
const DIFF_SYSTEM_PROMPT: &str = r#"You are a precise coding assistant. When modifying code:
1. Output ONLY unified diff format (no full files)
2. Include file paths in the diff header
3. Be minimal — only change what's needed

Example output:
```diff
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -42,3 +42,3 @@
-    if token.expired {
+    if token.expired || token.revoked {
```

If no code change is needed, explain briefly."#;

/// Call the Claude API with minimal context.
///
/// This is THE token spend — the only part of the pipeline that costs money.
pub async fn call_claude(
    context: &str,
    parsed: &ParsedRequest,
    config: &Config,
) -> Result<String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY environment variable not set"))?;

    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "model": config.general.model,
        "max_tokens": config.general.max_tokens,
        "system": DIFF_SYSTEM_PROMPT,
        "messages": [
            {
                "role": "user",
                "content": format!("{}\n\n{}", context, parsed.task)
            }
        ]
    });

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let response_body: serde_json::Value = response.json().await?;

    if !status.is_success() {
        let error_msg = response_body["error"]["message"]
            .as_str()
            .unwrap_or("Unknown API error");
        anyhow::bail!("Claude API error ({}): {}", status, error_msg);
    }

    // Extract text from response
    let text = response_body["content"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|block| block["text"].as_str())
        .unwrap_or("")
        .to_string();

    Ok(text)
}
