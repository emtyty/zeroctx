use crate::config::Config;
use crate::core::types::FilterResult;
use crate::filters::OutputFilter;

pub struct NetworkFilter;

impl OutputFilter for NetworkFilter {
    fn name(&self) -> &str {
        "network"
    }

    fn matches(&self, command: &str) -> bool {
        command.starts_with("curl ") || command.starts_with("wget ")
    }

    fn filter(&self, output: &str, _config: &Config) -> FilterResult {
        let original_lines = output.lines().count();
        let filtered = filter_http_output(output);
        let filtered_lines = filtered.lines().count();
        let savings = if original_lines > 0 {
            (1.0 - filtered_lines as f64 / original_lines as f64) * 100.0
        } else {
            0.0
        };

        FilterResult {
            output: filtered,
            original_lines,
            filtered_lines,
            savings_percent: savings,
        }
    }
}

fn filter_http_output(output: &str) -> String {
    let mut result = Vec::new();
    let mut in_headers = false;
    let mut status_line: Option<String> = None;
    let mut content_type: Option<String> = None;

    for line in output.lines() {
        let trimmed = line.trim();

        // curl/wget progress lines — always strip
        if trimmed.starts_with("% Total")
            || trimmed.starts_with("% Received")
            || trimmed.contains("--:--:--")
            || trimmed.starts_with("Connecting to")
            || trimmed.starts_with("Resolving")
            || trimmed.starts_with("Length:")
            || trimmed.starts_with("Saving to:")
            || trimmed.starts_with("  0  ")
            || trimmed.starts_with("100 ")
        {
            continue;
        }

        // HTTP status line (keep)
        if trimmed.starts_with("HTTP/") {
            status_line = Some(trimmed.to_string());
            in_headers = true;
            continue;
        }

        // HTTP headers — keep only content-type
        if in_headers {
            if trimmed.is_empty() {
                in_headers = false;
                // Emit status + content-type
                if let Some(ref status) = status_line {
                    result.push(status.clone());
                }
                if let Some(ref ct) = content_type {
                    result.push(ct.clone());
                }
                result.push(String::new());
            } else if trimmed.to_lowercase().starts_with("content-type:") {
                content_type = Some(trimmed.to_string());
            }
            // Skip all other headers
            continue;
        }

        // Body content
        // Try to detect and compact JSON
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            // Might be start of JSON body — collect and try to compact
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(output.trim()) {
                // Entire output is JSON — compact it
                let compact = compact_json(&value, 0, 3);
                return if let Some(ref status) = status_line {
                    format!("{}\n\n{}", status, compact)
                } else {
                    compact
                };
            }
        }

        result.push(line.to_string());
    }

    result.join("\n")
}

/// Compact JSON: truncate at max depth
fn compact_json(value: &serde_json::Value, depth: usize, max_depth: usize) -> String {
    if depth >= max_depth {
        return match value {
            serde_json::Value::Object(map) => format!("{{...{} keys}}", map.len()),
            serde_json::Value::Array(arr) => format!("[...{} items]", arr.len()),
            serde_json::Value::String(s) if s.len() > 100 => format!("\"{}...\"", &s[..100]),
            _ => value.to_string(),
        };
    }

    match value {
        serde_json::Value::Object(map) => {
            let entries: Vec<String> = map
                .iter()
                .take(10)
                .map(|(k, v)| format!("\"{}\": {}", k, compact_json(v, depth + 1, max_depth)))
                .collect();
            let suffix = if map.len() > 10 {
                format!(", ...+{}", map.len() - 10)
            } else {
                String::new()
            };
            format!("{{{}{}}}", entries.join(", "), suffix)
        }
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr
                .iter()
                .take(5)
                .map(|v| compact_json(v, depth + 1, max_depth))
                .collect();
            let suffix = if arr.len() > 5 {
                format!(", ...+{}", arr.len() - 5)
            } else {
                String::new()
            };
            format!("[{}{}]", items.join(", "), suffix)
        }
        serde_json::Value::String(s) if s.len() > 200 => {
            format!("\"{}...\"", &s[..200])
        }
        _ => value.to_string(),
    }
}
