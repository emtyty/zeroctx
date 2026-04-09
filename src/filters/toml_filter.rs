use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::config::Config;
use crate::core::types::FilterResult;

/// A TOML-defined filter with 8-stage pipeline.
#[derive(Debug, Deserialize)]
pub struct TomlFilterDef {
    pub description: Option<String>,
    pub match_command: String,
    #[serde(default)]
    pub strip_ansi: bool,
    #[serde(default)]
    pub replace: Vec<ReplaceRule>,
    #[serde(default)]
    pub match_output: Vec<MatchOutputRule>,
    #[serde(default)]
    pub strip_lines_matching: Vec<String>,
    #[serde(default)]
    pub keep_lines_matching: Vec<String>,
    #[serde(default)]
    pub truncate_lines_at: Option<usize>,
    #[serde(default)]
    pub head_lines: Option<usize>,
    #[serde(default)]
    pub tail_lines: Option<usize>,
    #[serde(default)]
    pub max_lines: Option<usize>,
    #[serde(default)]
    pub on_empty: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReplaceRule {
    pub pattern: String,
    pub replacement: String,
}

#[derive(Debug, Deserialize)]
pub struct MatchOutputRule {
    pub pattern: String,
    pub message: String,
}

/// Compiled TOML filter ready for execution.
struct CompiledFilter {
    match_command: Regex,
    def: TomlFilterDef,
    strip_regexes: Vec<Regex>,
    keep_regexes: Vec<Regex>,
    replace_regexes: Vec<(Regex, String)>,
    match_output_regexes: Vec<(Regex, String)>,
}

/// Load and apply user-defined TOML filters.
///
/// Search order: .zeroctx/filters.toml (project) → ~/.zeroctx/filters.toml (global)
pub fn apply_toml_filter(
    command: &str,
    output: &str,
    _config: &Config,
) -> Option<FilterResult> {
    let filters = load_filters();
    for filter in &*filters {
        if filter.match_command.is_match(command) {
            let result = apply_pipeline(output, filter);
            return Some(result);
        }
    }
    None
}

fn load_filters() -> &'static Vec<CompiledFilter> {
    static FILTERS: Lazy<Vec<CompiledFilter>> = Lazy::new(|| {
        let mut all = Vec::new();

        // Project-level
        if let Ok(filters) = load_from_file(Path::new(".zeroctx/filters.toml")) {
            all.extend(filters);
        }

        // Global
        if let Some(global) = global_filters_path() {
            if let Ok(filters) = load_from_file(&global) {
                all.extend(filters);
            }
        }

        all
    });
    &FILTERS
}

fn load_from_file(path: &Path) -> Result<Vec<CompiledFilter>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)?;
    let raw: HashMap<String, HashMap<String, TomlFilterDef>> = toml::from_str(&content)?;

    let mut compiled = Vec::new();
    if let Some(filters) = raw.get("filters") {
        for (_name, def) in filters {
            if let Ok(filter) = compile_filter(def) {
                compiled.push(filter);
            }
        }
    }

    Ok(compiled)
}

fn compile_filter(def: &TomlFilterDef) -> Result<CompiledFilter> {
    let match_command = Regex::new(&def.match_command)?;

    let strip_regexes: Vec<Regex> = def
        .strip_lines_matching
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect();

    let keep_regexes: Vec<Regex> = def
        .keep_lines_matching
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect();

    let replace_regexes: Vec<(Regex, String)> = def
        .replace
        .iter()
        .filter_map(|r| Regex::new(&r.pattern).ok().map(|re| (re, r.replacement.clone())))
        .collect();

    let match_output_regexes: Vec<(Regex, String)> = def
        .match_output
        .iter()
        .filter_map(|r| Regex::new(&r.pattern).ok().map(|re| (re, r.message.clone())))
        .collect();

    // We need to move def's fields — deserialize again from the same source isn't ideal
    // but the filter def is consumed. Use a workaround:
    Ok(CompiledFilter {
        match_command,
        def: TomlFilterDef {
            description: None,
            match_command: String::new(),
            strip_ansi: def.strip_ansi,
            replace: Vec::new(),
            match_output: Vec::new(),
            strip_lines_matching: Vec::new(),
            keep_lines_matching: Vec::new(),
            truncate_lines_at: def.truncate_lines_at,
            head_lines: def.head_lines,
            tail_lines: def.tail_lines,
            max_lines: def.max_lines,
            on_empty: def.on_empty.clone(),
        },
        strip_regexes,
        keep_regexes,
        replace_regexes,
        match_output_regexes,
    })
}

/// Apply the 8-stage pipeline.
fn apply_pipeline(output: &str, filter: &CompiledFilter) -> FilterResult {
    let original_lines = output.lines().count();
    let mut text = output.to_string();

    // Stage 1: strip_ansi
    if filter.def.strip_ansi {
        static ANSI_RE: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").expect("valid"));
        text = ANSI_RE.replace_all(&text, "").to_string();
    }

    // Stage 2: replace
    for (re, replacement) in &filter.replace_regexes {
        text = re.replace_all(&text, replacement.as_str()).to_string();
    }

    // Stage 3: match_output (if pattern matches → return short message)
    for (re, message) in &filter.match_output_regexes {
        if re.is_match(&text) {
            return FilterResult {
                output: message.clone(),
                original_lines,
                filtered_lines: 1,
                savings_percent: if original_lines > 0 {
                    (1.0 - 1.0 / original_lines as f64) * 100.0
                } else {
                    0.0
                },
            };
        }
    }

    // Stage 4: strip_lines_matching
    if !filter.strip_regexes.is_empty() {
        let lines: Vec<&str> = text
            .lines()
            .filter(|line| !filter.strip_regexes.iter().any(|re| re.is_match(line)))
            .collect();
        text = lines.join("\n");
    }

    // Stage 5: keep_lines_matching
    if !filter.keep_regexes.is_empty() {
        let lines: Vec<&str> = text
            .lines()
            .filter(|line| filter.keep_regexes.iter().any(|re| re.is_match(line)))
            .collect();
        text = lines.join("\n");
    }

    // Stage 6: truncate_lines_at
    if let Some(max_chars) = filter.def.truncate_lines_at {
        let lines: Vec<String> = text
            .lines()
            .map(|line| {
                if line.len() > max_chars {
                    format!("{}...", &line[..max_chars])
                } else {
                    line.to_string()
                }
            })
            .collect();
        text = lines.join("\n");
    }

    // Stage 7: head_lines / tail_lines
    if let Some(head) = filter.def.head_lines {
        let lines: Vec<&str> = text.lines().take(head).collect();
        text = lines.join("\n");
    }
    if let Some(tail) = filter.def.tail_lines {
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() > tail {
            text = lines[lines.len() - tail..].join("\n");
        }
    }

    // Stage 8: max_lines + on_empty
    if let Some(max) = filter.def.max_lines {
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() > max {
            let truncated: Vec<&str> = lines[..max].to_vec();
            text = format!("{}\n... ({} more lines)", truncated.join("\n"), lines.len() - max);
        }
    }

    // on_empty fallback
    if text.trim().is_empty() {
        if let Some(ref fallback) = filter.def.on_empty {
            text = fallback.clone();
        }
    }

    let filtered_lines = text.lines().count();
    let savings = if original_lines > 0 {
        (1.0 - filtered_lines as f64 / original_lines as f64) * 100.0
    } else {
        0.0
    };

    FilterResult {
        output: text,
        original_lines,
        filtered_lines,
        savings_percent: savings,
    }
}

fn global_filters_path() -> Option<std::path::PathBuf> {
    if cfg!(windows) {
        std::env::var("APPDATA")
            .ok()
            .map(|p| std::path::PathBuf::from(p).join("zeroctx").join("filters.toml"))
    } else {
        dirs_next::config_dir().map(|p| p.join("zeroctx").join("filters.toml"))
    }
}
