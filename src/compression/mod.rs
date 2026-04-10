pub mod ast;
pub mod context_builder;
pub mod context_cache;

use anyhow::Result;
use std::path::Path;

use crate::config::Config;
use crate::core::mismatch::{MismatchCategory, MismatchEvent, MismatchSeverity};
use crate::core::types::Language;

/// Minimum lines before compression kicks in.
const COMPRESS_THRESHOLD: usize = 80;

/// Compress a file's content for LLM context.
///
/// Strategy:
/// - Files < 80 lines: return as-is (not worth compressing)
/// - Files >= 80 lines: strip blank lines, comments, and collapse whitespace
/// - Code files >= 80 lines: extract signatures + key definitions
pub fn compress_file(path: &str, _config: &Config) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    let line_count = content.lines().count();

    // Small files: return as-is
    if line_count < COMPRESS_THRESHOLD {
        return Ok(content);
    }

    // Detect language from extension
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let lang = Language::from_extension(ext);

    // Non-code files: basic compression (strip blank lines, trim)
    if matches!(lang, Language::Unknown) {
        return Ok(basic_compress(&content));
    }

    // Code files: extract signatures only
    match ast::AstCompressor::signatures_only(&content, lang) {
        Ok(compressed) if !compressed.is_empty() => {
            let orig_tokens = content.len() / 4;
            let comp_tokens = compressed.len() / 4;
            let savings_pct = if orig_tokens > 0 {
                (orig_tokens - comp_tokens) * 100 / orig_tokens
            } else {
                0
            };

            // Track extreme compression as potential mismatch
            if savings_pct > 95 && line_count > 200 {
                crate::core::mismatch::log_event(&MismatchEvent {
                    category: MismatchCategory::Compression,
                    severity: MismatchSeverity::Info,
                    detected: format!(
                        "tree_sitter, lang={:?}, savings={}%",
                        lang, savings_pct
                    ),
                    actual: format!(
                        "{}→{} lines (may lose important context)",
                        line_count,
                        compressed.lines().count()
                    ),
                    input_snippet: path.to_string(),
                    context: format!("method=tree_sitter, original_lines={}", line_count),
                    user_feedback: None,
                });
            }

            let header = format!(
                "// [ZeroCTX compressed: {} → {} lines, ~{}% saved]\n// Full file: {}\n\n",
                line_count,
                compressed.lines().count(),
                savings_pct,
                path,
            );
            Ok(format!("{}{}", header, compressed))
        }
        _ => {
            // Fallback to basic compression — log as signal
            crate::core::mismatch::log_signal(
                "fallback_used",
                &format!("compress {}", path),
                &format!(
                    "{{\"language\": \"{:?}\", \"lines\": {}, \"reason\": \"ast_failed\"}}",
                    lang, line_count
                ),
            );

            Ok(basic_compress(&content))
        }
    }
}

/// Compress a file and write the result to a temp file. Returns the temp path.
pub fn compress_to_temp(path: &str, config: &Config) -> Result<String> {
    let compressed = compress_file(path, config)?;

    let temp_dir = std::env::temp_dir().join("zeroctx");
    std::fs::create_dir_all(&temp_dir)?;

    // Use a hash of the path for the temp filename to avoid collisions
    let filename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let temp_path = temp_dir.join(format!("compressed_{}", filename));

    std::fs::write(&temp_path, &compressed)?;
    Ok(temp_path.to_string_lossy().to_string())
}

/// Basic compression: strip excessive blank lines, trim trailing whitespace.
fn basic_compress(content: &str) -> String {
    let mut result = Vec::new();
    let mut blank_count = 0;

    for line in content.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                result.push("");
            }
        } else {
            blank_count = 0;
            result.push(trimmed);
        }
    }

    result.join("\n")
}
