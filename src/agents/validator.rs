use anyhow::Result;
use std::path::Path;

use crate::config::Config;
use crate::core::types::ParsedRequest;
use crate::integration::diff_applier;

/// Validate Claude's output and apply diffs if present.
///
/// Flow:
/// 1. Extract unified diffs from response
/// 2. For each diff: validate target file exists → apply via patch
/// 3. Run language validator on patched file
/// 4. If validation fails: return error for retry
/// 5. If all pass: return success with applied files
pub fn validate_and_apply(
    response: &str,
    _parsed: &ParsedRequest,
    _config: &Config,
) -> Result<String> {
    let diffs = diff_applier::extract_diffs(response);

    if diffs.is_empty() {
        // No diffs — return response as-is (could be an explanation)
        return Ok(response.to_string());
    }

    let mut applied = Vec::new();
    let mut errors = Vec::new();

    for diff in &diffs {
        // Check target file exists
        if !Path::new(&diff.file_path).exists() {
            errors.push(format!("File not found: {}", diff.file_path));
            continue;
        }

        // Apply the diff
        match diff.apply() {
            Ok(_msg) => {
                // Validate the patched file
                let ext = Path::new(&diff.file_path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");

                if let Some(lang) = crate::languages::for_extension(ext) {
                    if let Ok(content) = std::fs::read_to_string(&diff.file_path) {
                        let result = lang.validate(&content);
                        if !result.valid {
                            errors.push(format!(
                                "Validation failed for {}: {}",
                                diff.file_path,
                                result.errors.join("; ")
                            ));
                            continue;
                        }
                    }
                }

                applied.push(diff.file_path.clone());
                tracing::info!(file = &diff.file_path, "Applied diff");
            }
            Err(e) => {
                errors.push(format!("Failed to apply diff to {}: {}", diff.file_path, e));
            }
        }
    }

    let mut result = String::new();

    if !applied.is_empty() {
        result.push_str(&format!(
            "Applied patches to {} file(s):\n",
            applied.len()
        ));
        for f in &applied {
            result.push_str(&format!("  {}\n", f));
        }
    }

    if !errors.is_empty() {
        result.push_str(&format!(
            "\nErrors ({}):\n",
            errors.len()
        ));
        for e in &errors {
            result.push_str(&format!("  {}\n", e));
        }
    }

    if !applied.is_empty() && errors.is_empty() {
        result.push_str("\nAll patches applied and validated successfully.");
    }

    // Include original response for context
    result.push_str("\n\n--- Claude's response ---\n");
    result.push_str(response);

    Ok(result)
}
