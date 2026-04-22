pub mod ast;
pub mod context_builder;
pub mod context_cache;

use anyhow::Result;
use std::path::Path;

use crate::config::Config;
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
    // Fast path: check mtime-based persistent cache before reading the file
    if let Ok(cache) = context_cache::ContextCache::open_default() {
        if let Ok(Some(cached)) = cache.check_mtime(path) {
            return Ok(cached);
        }
    }

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

            // Fallback to basic_compress when tree-sitter is too aggressive:
            // (1) >90% savings on large files, or (2) <15 output lines from 200+ line file
            let compressed_lines = compressed.lines().count();
            if (savings_pct > 90 && line_count > 200)
                || (line_count > 200 && compressed_lines < 15)
            {
                // Fallback is expected behavior, not a mismatch — log as signal.
                crate::core::mismatch::log_signal(
                    "aggressive_compression_fallback",
                    &format!("compress {}", path),
                    &format!(
                        "{{\"language\": \"{:?}\", \"original_lines\": {}, \"compressed_lines\": {}, \"savings_pct\": {}}}",
                        lang, line_count, compressed_lines, savings_pct
                    ),
                );
                let basic = basic_compress(&content);
                let basic_lines = basic.lines().count();
                let header = format!(
                    "// [ZeroCTX compressed: {} → {} lines, ~{}% saved]\n// Full file: {}\n\n",
                    line_count,
                    basic_lines,
                    if line_count > 0 { (line_count - basic_lines) * 100 / line_count } else { 0 },
                    path,
                );
                let result = format!("{}{}", header, basic);
                cache_store(path, &content, &result);
                return Ok(result);
            }

            let header = format!(
                "// [ZeroCTX compressed: {} → {} lines, ~{}% saved]\n// Full file: {}\n\n",
                line_count,
                compressed.lines().count(),
                savings_pct,
                path,
            );
            let result = format!("{}{}", header, compressed);
            cache_store(path, &content, &result);
            Ok(result)
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

            let result = basic_compress(&content);
            cache_store(path, &content, &result);
            Ok(result)
        }
    }
}

/// Write result to persistent cache (best-effort, never fails the caller).
fn cache_store(path: &str, content: &str, compressed: &str) {
    if let Ok(cache) = context_cache::ContextCache::open_default() {
        cache.store_compressed(path, content, compressed).ok();
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

/// Load a project brief from `.zeroctx/brief.md` in the current directory.
/// Returns compressed content if the file exists, None otherwise.
pub fn load_project_brief() -> Option<String> {
    // Search from cwd upward for .zeroctx/brief.md
    let cwd = std::env::current_dir().ok()?;
    let brief_path = cwd.join(".zeroctx").join("brief.md");
    if !brief_path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&brief_path).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    // Compress and cap at ~500 tokens (2000 chars)
    let compressed = basic_compress(&content);
    const MAX_BRIEF_CHARS: usize = 2000;
    let capped = if compressed.len() > MAX_BRIEF_CHARS {
        format!("{}...\n[brief truncated — edit .zeroctx/brief.md to shorten]", &compressed[..MAX_BRIEF_CHARS])
    } else {
        compressed
    };
    Some(format!("// [Project brief from .zeroctx/brief.md]\n{}\n// [End of brief]\n\n", capped))
}

/// Show the current project brief if it exists.
pub fn show_project_brief() {
    match load_project_brief() {
        Some(brief) => print!("{}", brief),
        None => println!("No .zeroctx/brief.md found in current directory.\nRun `zero brief --generate` to create one."),
    }
}

/// Auto-generate a project brief from README.md, Cargo.toml, or package.json.
pub fn generate_project_brief() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let zeroctx_dir = cwd.join(".zeroctx");
    std::fs::create_dir_all(&zeroctx_dir)?;
    let brief_path = zeroctx_dir.join("brief.md");

    let mut sections: Vec<String> = Vec::new();

    // Extract from README.md (first 50 lines)
    if let Ok(readme) = std::fs::read_to_string(cwd.join("README.md")) {
        let first50: Vec<&str> = readme.lines().take(50).collect();
        sections.push(format!("## From README.md\n{}", first50.join("\n")));
    }

    // Extract key fields from Cargo.toml
    if let Ok(cargo) = std::fs::read_to_string(cwd.join("Cargo.toml")) {
        let mut cargo_info = Vec::new();
        for line in cargo.lines() {
            let t = line.trim();
            if t.starts_with("name =") || t.starts_with("version =") || t.starts_with("description =") {
                cargo_info.push(t.to_string());
            }
        }
        if !cargo_info.is_empty() {
            sections.push(format!("## From Cargo.toml\n{}", cargo_info.join("\n")));
        }
    }

    // Extract key fields from package.json
    if let Ok(pkg_json) = std::fs::read_to_string(cwd.join("package.json")) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&pkg_json) {
            let mut info = Vec::new();
            for field in &["name", "version", "description", "main"] {
                if let Some(val) = json.get(field) {
                    info.push(format!("{}: {}", field, val));
                }
            }
            if !info.is_empty() {
                sections.push(format!("## From package.json\n{}", info.join("\n")));
            }
        }
    }

    if sections.is_empty() {
        println!("No README.md, Cargo.toml, or package.json found. Creating empty brief.");
        sections.push("# Project Brief\n\nDescribe your project here. This file is injected into Claude Code sessions automatically.\n\nKey info to include:\n- What this project does\n- Main entry points\n- Key data structures or APIs\n- Common development tasks".to_string());
    }

    let content = format!("# Project Brief (auto-generated)\n\nEdit this file to customize what ZeroCTX injects at the start of Claude Code sessions.\n\n{}", sections.join("\n\n"));
    std::fs::write(&brief_path, &content)?;
    println!("Generated .zeroctx/brief.md ({} chars)", content.len());
    println!("Edit it to refine what gets injected, then run `zero brief` to preview.");
    Ok(())
}
