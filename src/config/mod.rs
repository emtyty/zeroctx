use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

/// ZeroCTX configuration.
///
/// Loaded with precedence: project (.zeroctx/config.toml) > global (~/.zeroctx/config.toml) > defaults.
/// All limits are configurable — no hard-coded values (unlike RTK).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub limits: LimitsConfig,
    pub filters: FiltersConfig,
    pub session: SessionConfig,
    pub autofix: AutofixConfig,
    pub hooks: HooksConfig,
    pub export: ExportConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub model: String,
    pub max_tokens: usize,
    pub context_budget: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    pub grep_max_results: usize,
    pub grep_max_per_file: usize,
    pub git_status_max_files: usize,
    pub git_diff_max_hunk_lines: usize,
    pub max_output_size_bytes: Option<usize>,
    pub tree_max_depth: usize,
    pub tree_max_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FiltersConfig {
    pub ignore_dirs: Vec<String>,
    pub ignore_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    pub cache_enabled: bool,
    pub database_path: Option<PathBuf>,
    pub history_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutofixConfig {
    pub enabled: bool,
    pub auto_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HooksConfig {
    /// Commands to never intercept (pass through unchanged)
    pub exclude_commands: Vec<String>,
    /// Commands that require user confirmation before rewriting (glob patterns)
    pub ask_commands: Vec<String>,
    /// Commands that should be blocked entirely (glob patterns)
    pub deny_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportConfig {
    pub default_format: String,
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
}

// --- Defaults ---

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            limits: LimitsConfig::default(),
            filters: FiltersConfig::default(),
            session: SessionConfig::default(),
            autofix: AutofixConfig::default(),
            hooks: HooksConfig::default(),
            export: ExportConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
            context_budget: 12000,
        }
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            grep_max_results: 200,
            grep_max_per_file: 25,
            git_status_max_files: 30,
            git_diff_max_hunk_lines: 100,
            max_output_size_bytes: Some(100 * 1024 * 1024), // 100MB
            tree_max_depth: 4,
            tree_max_entries: 80,
        }
    }
}

impl Default for FiltersConfig {
    fn default() -> Self {
        Self {
            ignore_dirs: vec![
                ".git".into(),
                "node_modules".into(),
                "target".into(),
                "bin".into(),
                "obj".into(),
                "__pycache__".into(),
                ".venv".into(),
                "dist".into(),
                "build".into(),
            ],
            ignore_files: vec![
                "*.lock".into(),
                "*.min.js".into(),
                "*.min.css".into(),
            ],
        }
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            cache_enabled: true,
            database_path: None,
            history_days: 90,
        }
    }
}

impl Default for AutofixConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_run: true,
        }
    }
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            exclude_commands: vec![
                "ssh".into(),
                "vim".into(),
                "nano".into(),
                "less".into(),
                "man".into(),
            ],
            ask_commands: Vec::new(),
            deny_commands: vec![
                "rm -rf /*".into(),
                "rm -rf /".into(),
            ],
        }
    }
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            default_format: "json".into(),
            output_dir: ".zeroctx/reports".into(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
        }
    }
}

impl Config {
    /// Load configuration with precedence:
    /// 1. Project-level: .zeroctx/config.toml (highest)
    /// 2. Global: ~/.zeroctx/config.toml
    /// 3. Built-in defaults (lowest)
    pub fn load() -> Result<Self> {
        let mut config = Config::default();

        // Load global config
        if let Some(global_path) = global_config_path() {
            if global_path.exists() {
                debug!(path = %global_path.display(), "Loading global config");
                let content = std::fs::read_to_string(&global_path)
                    .with_context(|| format!("Failed to read {}", global_path.display()))?;
                config = toml::from_str(&content)
                    .with_context(|| format!("Failed to parse {}", global_path.display()))?;
            }
        }

        // Load project-level config (overrides global)
        let project_path = Path::new(".zeroctx/config.toml");
        if project_path.exists() {
            debug!(path = %project_path.display(), "Loading project config");
            let content = std::fs::read_to_string(project_path)
                .with_context(|| format!("Failed to read {}", project_path.display()))?;
            config = toml::from_str(&content)
                .with_context(|| format!("Failed to parse {}", project_path.display()))?;
        }

        // Environment variable overrides
        if let Ok(model) = std::env::var("ZEROCTX_MODEL") {
            config.general.model = model;
        }
        if let Ok(max) = std::env::var("ZEROCTX_MAX_TOKENS") {
            if let Ok(v) = max.parse() {
                config.general.max_tokens = v;
            }
        }
        if let Ok(budget) = std::env::var("ZEROCTX_CONTEXT_BUDGET") {
            if let Ok(v) = budget.parse() {
                config.general.context_budget = v;
            }
        }
        if let Ok(level) = std::env::var("ZEROCTX_LOG_LEVEL") {
            config.logging.level = level;
        }

        Ok(config)
    }

    /// Load from a specific file path.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        Ok(config)
    }
}

fn global_config_path() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var("APPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("zeroctx").join("config.toml"))
    } else {
        dirs_next::config_dir().map(|p| p.join("zeroctx").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.general.max_tokens, 4096);
        assert_eq!(config.limits.grep_max_results, 200);
        assert!(config.autofix.enabled);
        assert!(config.session.cache_enabled);
    }

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
[general]
model = "claude-opus-4-20250514"
max_tokens = 8192

[limits]
grep_max_results = 500
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.model, "claude-opus-4-20250514");
        assert_eq!(config.general.max_tokens, 8192);
        assert_eq!(config.limits.grep_max_results, 500);
        // Defaults still apply for unset fields
        assert_eq!(config.limits.grep_max_per_file, 25);
    }
}
