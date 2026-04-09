use anyhow::{Context, Result};
use std::path::PathBuf;

/// Embedded hook script templates (__ZERO_PATH__ placeholder replaced at install time).
const BASH_HOOK_TEMPLATE: &str = include_str!("../../hooks/zeroctx-rewrite.sh");
const READ_HOOK_TEMPLATE: &str = include_str!("../../hooks/zeroctx-read.sh");
const CLAUDE_MD_CONTENT: &str = include_str!("../../CLAUDE.md");

/// Install the Claude Code PreToolUse hooks.
///
/// - Finds the running zero binary's absolute path
/// - Writes hook scripts with the embedded path to ~/.claude/hooks/
/// - Patches settings.json to register both hooks
///
/// If `project` is true, installs to `.claude/` in the current directory instead.
pub fn install_with_options(project: bool) -> Result<()> {
    // Find our own binary path
    let zero_path = find_self_path()?;
    let zero_path_str = zero_path.to_string_lossy().replace('\\', "/");

    tracing::info!(zero_binary = %zero_path_str, "Detected zero binary path");

    // Determine target directory
    let claude_dir = if project {
        PathBuf::from(".claude")
    } else {
        user_claude_dir()?
    };

    let hooks_dir = claude_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)
        .with_context(|| format!("Failed to create {}", hooks_dir.display()))?;

    // Write hook scripts with embedded zero path
    let bash_hook = BASH_HOOK_TEMPLATE.replace("__ZERO_PATH__", &zero_path_str);
    let read_hook = READ_HOOK_TEMPLATE.replace("__ZERO_PATH__", &zero_path_str);

    let bash_hook_path = hooks_dir.join("zeroctx-rewrite.sh");
    std::fs::write(&bash_hook_path, &bash_hook)
        .with_context(|| format!("Failed to write {}", bash_hook_path.display()))?;

    let read_hook_path = hooks_dir.join("zeroctx-read.sh");
    std::fs::write(&read_hook_path, &read_hook)
        .with_context(|| format!("Failed to write {}", read_hook_path.display()))?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&bash_hook_path, perms)?;
        std::fs::set_permissions(&read_hook_path, perms)?;
    }

    // Patch settings.json
    let settings_path = claude_dir.join("settings.json");
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read {}", settings_path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", settings_path.display()))?
    } else {
        serde_json::json!({})
    };

    let bash_command = normalize_path(&bash_hook_path);
    let read_command = normalize_path(&read_hook_path);

    let bash_entry = serde_json::json!({
        "matcher": "Bash",
        "hooks": [{ "type": "command", "command": bash_command }]
    });
    let read_entry = serde_json::json!({
        "matcher": "Read",
        "hooks": [{ "type": "command", "command": read_command }]
    });

    // Insert hooks, preserving existing non-zeroctx hooks
    let settings_obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Settings is not a JSON object"))?;

    let hooks = settings_obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks is not a JSON object"))?;
    let pre_tool_use = hooks_obj
        .entry("PreToolUse")
        .or_insert_with(|| serde_json::json!([]));
    let arr = pre_tool_use
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("PreToolUse is not an array"))?;

    // Remove old zeroctx entries
    arr.retain(|entry| {
        !entry
            .pointer("/hooks/0/command")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("zeroctx"))
            .unwrap_or(false)
    });

    arr.push(bash_entry);
    arr.push(read_entry);

    let content = serde_json::to_string_pretty(&settings)?;
    std::fs::write(&settings_path, &content)
        .with_context(|| format!("Failed to write {}", settings_path.display()))?;

    // Write CLAUDE.md so Claude Code knows about zero commands
    let claude_md_path = claude_dir.join("CLAUDE.md");
    let should_write = if claude_md_path.exists() {
        // Check if existing CLAUDE.md already has ZeroCTX section
        let existing = std::fs::read_to_string(&claude_md_path).unwrap_or_default();
        !existing.contains("ZeroCTX")
    } else {
        true
    };

    if should_write {
        if claude_md_path.exists() {
            // Append to existing CLAUDE.md
            let existing = std::fs::read_to_string(&claude_md_path)?;
            let combined = format!("{}\n\n{}", existing, CLAUDE_MD_CONTENT);
            std::fs::write(&claude_md_path, combined)?;
        } else {
            std::fs::write(&claude_md_path, CLAUDE_MD_CONTENT)?;
        }
        println!("  CLAUDE.md: {} (teaches Claude to use zero commands)", claude_md_path.display());
    }

    let scope = if project { "project" } else { "user" };
    println!("Installed ZeroCTX hooks ({} scope):", scope);
    println!("  Bash hook: {}", bash_hook_path.display());
    println!("  Read hook: {}", read_hook_path.display());
    println!("  Settings:  {}", settings_path.display());
    println!("  Binary:    {}", zero_path_str);

    Ok(())
}

/// Install to user-level ~/.claude/ (default).
pub fn install() -> Result<()> {
    install_with_options(false)
}

/// Install with full control over what gets installed.
pub fn install_full(project: bool, write_claude_md: bool, include_read_hook: bool) -> Result<()> {
    // Call install_with_options for the core install
    install_with_options(project)?;

    // Remove Read hook if not wanted
    if !include_read_hook {
        let claude_dir = if project {
            std::path::PathBuf::from(".claude")
        } else {
            user_claude_dir()?
        };
        let read_hook = claude_dir.join("hooks").join("zeroctx-read.sh");
        if read_hook.exists() {
            std::fs::remove_file(&read_hook)?;
        }
        // Remove Read matcher from settings.json
        let settings_path = claude_dir.join("settings.json");
        if settings_path.exists() {
            let content = std::fs::read_to_string(&settings_path)?;
            let mut settings: serde_json::Value = serde_json::from_str(&content)?;
            if let Some(arr) = settings.pointer_mut("/hooks/PreToolUse").and_then(|v| v.as_array_mut()) {
                arr.retain(|entry| {
                    entry.get("matcher").and_then(|m| m.as_str()) != Some("Read")
                        || !entry.pointer("/hooks/0/command")
                            .and_then(|v| v.as_str())
                            .map(|s| s.contains("zeroctx"))
                            .unwrap_or(false)
                });
            }
            std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        }
        println!("  (Read hook skipped — use --no-read-hook to keep files uncompressed)");
    }

    // Remove CLAUDE.md if not wanted
    if !write_claude_md {
        let claude_dir = if project {
            std::path::PathBuf::from(".claude")
        } else {
            user_claude_dir()?
        };
        let claude_md = claude_dir.join("CLAUDE.md");
        // Only remove if we just wrote it (contains ZeroCTX)
        if claude_md.exists() {
            let content = std::fs::read_to_string(&claude_md).unwrap_or_default();
            if content.contains("ZeroCTX") && content.lines().count() < 80 {
                std::fs::remove_file(&claude_md)?;
                println!("  (CLAUDE.md skipped)");
            }
        }
    }

    Ok(())
}

/// Remove the Claude Code hooks.
pub fn uninstall_with_options(project: bool) -> Result<()> {
    let claude_dir = if project {
        PathBuf::from(".claude")
    } else {
        user_claude_dir()?
    };

    // Remove hook scripts
    for name in &["zeroctx-rewrite.sh", "zeroctx-read.sh"] {
        let path = claude_dir.join("hooks").join(name);
        if path.exists() {
            std::fs::remove_file(&path)?;
            tracing::info!(path = %path.display(), "Removed hook script");
        }
    }

    // Remove from settings.json
    let settings_path = claude_dir.join("settings.json");
    if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)?;
        let mut settings: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(arr) = settings
            .pointer_mut("/hooks/PreToolUse")
            .and_then(|v| v.as_array_mut())
        {
            arr.retain(|entry| {
                !entry
                    .pointer("/hooks/0/command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains("zeroctx"))
                    .unwrap_or(false)
            });
        }

        let content = serde_json::to_string_pretty(&settings)?;
        std::fs::write(&settings_path, content)?;
    }

    let scope = if project { "project" } else { "user" };
    println!("Removed ZeroCTX hooks ({} scope).", scope);
    Ok(())
}

pub fn uninstall() -> Result<()> {
    uninstall_with_options(false)
}

/// Find the absolute path of the currently running zero binary.
fn find_self_path() -> Result<PathBuf> {
    // Method 1: std::env::current_exe()
    if let Ok(exe) = std::env::current_exe() {
        if exe.exists() {
            return Ok(exe);
        }
    }

    // Method 2: Search PATH for "zero" or "zero.exe"
    if let Ok(path) = which::which("zero") {
        return Ok(path);
    }

    // Method 3: Check common locations
    let candidates = [
        dirs_next::home_dir().map(|h| h.join("bin").join("zero.exe")),
        dirs_next::home_dir().map(|h| h.join(".cargo").join("bin").join("zero.exe")),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    anyhow::bail!(
        "Cannot find zero binary. Make sure it's on your PATH or run from the directory containing zero.exe."
    )
}

/// Normalize a path: resolve to absolute, strip Windows \\?\ prefix, use forward slashes.
fn normalize_path(path: &PathBuf) -> String {
    let abs = path
        .canonicalize()
        .unwrap_or_else(|_| path.clone());
    let s = abs.to_string_lossy().to_string();
    // Strip Windows extended-length path prefix
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.replace('\\', "/")
}

fn user_claude_dir() -> Result<PathBuf> {
    let home = if cfg!(windows) {
        std::env::var("USERPROFILE").map(PathBuf::from)?
    } else {
        std::env::var("HOME").map(PathBuf::from)?
    };
    Ok(home.join(".claude"))
}
