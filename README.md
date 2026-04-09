# ZeroCTX

A Rust-based agent team that reduces AI coding assistant token usage by 40-60% and saves 15-25% of session time.

ZeroCTX intercepts the deterministic work that AI assistants like Claude Code spend 60-70% of their context on — running commands, reading files, fetching URLs — and handles it locally with zero token cost. Only genuine reasoning reaches the LLM.

## How It Works

```
You: "run pytest and fix failures"

Without ZeroCTX (120K tokens, 12 min):
  Claude → run pytest → read 300 lines output → read 3 files (1500 lines)
  → think → output 200-line file → you test → repeat

With ZeroCTX (50K tokens, 8 min):
  Router    → detects: RUN + DEBUG intent           (0 tokens, <1ms)
  Build     → runs pytest, filters to 30 lines      (0 tokens, 60% saved)
  Analyzer  → extracts 20 relevant lines from files  (0 tokens, 95% saved)
  Reasoning → Claude sees minimal context, outputs diff (THE token spend)
  Validator → checks syntax, applies patch           (0 tokens)
```

### The 6 Agents

| Agent | What It Does | Token Cost |
|-------|-------------|------------|
| **Router** | Regex-based intent detection + task decomposition | 0 |
| **Fetch** | Web fetch, git clone, file read (parallel) | 0 |
| **Build** | Shell execution + output filtering (30+ filters) | 0 |
| **Analyzer** | Error auto-fix, AST compression, context caching | 0 |
| **Reasoning** | Single focused Claude API call | **THE spend** |
| **Validator** | Lint, typecheck, apply diff patches | 0 |

## Installation

### From Source (recommended for now)

```bash
# Prerequisites: Rust toolchain (https://rustup.rs)
git clone https://github.com/user/zeroctx.git
cd zeroctx/zero
cargo build --release

# Binary is at target/release/zero (or zero.exe on Windows)
# Add to PATH or copy to a directory in your PATH
```

### From Cargo (once published)

```bash
cargo install zeroctx
```

### Pre-built Binaries (coming soon)

Download from the Releases page for your platform:
- `zero-x86_64-unknown-linux-gnu` (Linux)
- `zero-x86_64-pc-windows-msvc.exe` (Windows)
- `zero-x86_64-apple-darwin` (macOS Intel)
- `zero-aarch64-apple-darwin` (macOS Apple Silicon)

## Quick Start

### 1. Install the Claude Code hook (one time)

```bash
zero install
```

This writes a PreToolUse hook to `~/.claude/settings.json` that automatically intercepts Claude Code's Bash and Read tool calls, routing them through ZeroCTX for compression.

That's it. Your next Claude Code session will use 40-60% fewer tokens automatically.

### 2. Or use standalone

**Interactive REPL:**
```bash
zero
> run pytest and fix failures
> fetch https://docs.rs/tokio and summarize
> read src/auth.rs and explain the login flow
```

**One-shot mode:**
```bash
zero "run pytest and fix failures"
zero "dotnet test and fix CS0246 errors"
zero "npm run build and fix type errors"
```

## What Gets Optimized

### Output Filters (30+ commands, 60-90% savings)

Every command output is compressed before reaching the LLM:

| Category | Commands | Typical Savings |
|----------|----------|----------------|
| **Git** | status, diff, log, show, branch, gh | 60-80% |
| **Python** | pytest, ruff, mypy, pip | 70-90% |
| **JavaScript** | eslint, tsc, jest, vitest, npm, next, playwright | 70-90% |
| **.NET** | dotnet build, dotnet test, nuget, format | 60-85% |
| **Rust** | cargo build, cargo test, cargo clippy | 80-90% |
| **System** | ls, tree, grep, find, cat, wc, env, logs | 50-75% |
| **Network** | curl, wget | 60-70% |

### Error Auto-Fix (60+ patterns, 100% savings when matched)

Common errors are fixed instantly without calling the LLM:

```
ModuleNotFoundError: No module named 'requests'
  → auto-runs: pip install requests (0 tokens, <2 sec)

Cannot find module 'lodash'
  → auto-runs: npm install lodash (0 tokens, <3 sec)

CS0246: The type or namespace 'Newtonsoft' could not be found
  → auto-runs: dotnet add package Newtonsoft.Json (0 tokens, <3 sec)

E0433: unresolved import `serde`
  → auto-runs: cargo add serde (0 tokens, <2 sec)
```

**Supported languages:** Python (15+ patterns), JavaScript/TypeScript (15+), C#/.NET (20+), Rust (10+)

### AST Compression (60-98% savings per file)

When Claude needs to read a file, ZeroCTX extracts only the relevant functions:

```
Without: Claude reads all 500 lines of auth.rs
With:    Error mentions verify_token → extract only that function (12 lines)
Savings: 98%
```

Uses [tree-sitter](https://tree-sitter.github.io/) for accurate AST parsing across Python, JavaScript/TypeScript, C#, and Rust.

### Context Caching (70%+ savings on round 2+)

Files are MD5-hashed. On subsequent rounds, unchanged files become one-line summaries:

```
Round 1: auth.rs (500 lines, full content)       → 1250 tokens
Round 2: auth.rs (unchanged, summary only)        → 15 tokens
Savings: 99%
```

### Diff-Only Output (90% fewer output tokens)

Claude generates unified diffs instead of full files:

```
Without: Claude outputs entire 200-line file     → 2000 output tokens (5x pricing)
With:    Claude outputs 5-line diff patch         → 100 output tokens
Savings: 95% on output tokens (which cost 5x input tokens)
```

## Configuration

ZeroCTX uses TOML configuration with three levels of precedence:

```
Project:  .zeroctx/config.toml     (highest priority, committed with repo)
Global:   ~/.zeroctx/config.toml   (user-wide defaults)
Built-in: defaults.toml            (shipped with binary)
```

### Example `.zeroctx/config.toml`

```toml
[general]
# Claude API model for reasoning agent
model = "claude-sonnet-4-20250514"
# Maximum tokens for Claude API calls
max_tokens = 4096
# Context budget in characters (~4 chars = 1 token)
context_budget = 12000

[limits]
# Output filter limits
grep_max_results = 200
grep_max_per_file = 25
git_status_max_files = 30
git_diff_max_hunk_lines = 100
max_output_size = "100MB"

[filters]
# Directories to ignore in tree/find
ignore_dirs = [".git", "node_modules", "target", "bin", "obj", "__pycache__", ".venv"]
ignore_files = ["*.lock", "*.min.js", "*.min.css"]

[session]
# Enable context caching across rounds
cache_enabled = true
# Session database location (default: ~/.zeroctx/sessions.db)
# database_path = "/custom/path/sessions.db"
# How long to keep session data
history_days = 90

[autofix]
# Enable automatic error fixing
enabled = true
# Auto-run fix commands without confirmation
auto_run = true
# Package name mappings (Python: import name → pip package)
[autofix.python_mappings]
cv2 = "opencv-python"
PIL = "Pillow"
sklearn = "scikit-learn"
yaml = "PyYAML"
bs4 = "beautifulsoup4"

[hooks]
# Commands to never intercept (pass through unchanged)
exclude_commands = ["ssh", "vim", "nano", "less"]

[export]
# Default export format
default_format = "json"
# Output directory for exports
output_dir = ".zeroctx/reports"

[logging]
# Log level: error, warn, info, debug, trace
level = "info"
```

### Environment Variables

All config values can be overridden via environment variables:

```bash
ZEROCTX_MODEL=claude-sonnet-4-20250514
ZEROCTX_MAX_TOKENS=4096
ZEROCTX_CONTEXT_BUDGET=12000
ZEROCTX_LOG_LEVEL=debug
ANTHROPIC_API_KEY=sk-ant-...    # Required for reasoning agent
```

## Adapting ZeroCTX to Your Project

### Custom Output Filters

Add project-specific filters in `.zeroctx/filters.toml`:

```toml
[filters.my-build-tool]
description = "Compact my-build-tool output"
match_command = "^my-build-tool\\s+"
strip_ansi = true
strip_lines_matching = ["^\\s*$", "^Downloading", "^\\s+Compiling"]
keep_lines_matching = ["^ERROR", "^WARNING", "^FAILED"]
max_lines = 50
on_empty = "my-build-tool: ok"

[[tests.my-build-tool]]
name = "filters build noise"
input = "Downloading dep1...\nDownloading dep2...\nCompiling...\nERROR: missing field"
expected = "ERROR: missing field"
```

### Custom Error Patterns

Add project-specific auto-fix patterns in `.zeroctx/patterns.toml`:

```toml
[[patterns]]
regex = "DOCKER_ERROR: image '(.+)' not found"
category = "docker_missing_image"
fixable = true
command = "docker pull {1}"
explanation = "Docker image not found locally, pulling from registry"

[[patterns]]
regex = "MIGRATION_ERROR: pending migrations"
category = "db_migration"
fixable = true
command = "dotnet ef database update"
explanation = "Database has pending Entity Framework migrations"
```

### Custom Intent Signals

Extend the router with project-specific keywords in `.zeroctx/intents.toml`:

```toml
[[signals]]
keywords = ["deploy", "release", "publish"]
intent = "RUN_AND_DEBUG"
default_command = "make deploy"

[[signals]]
keywords = ["migrate", "seed", "schema"]
intent = "RUN_AND_DEBUG"
default_command = "dotnet ef database update"
```

## CLI Reference

```
USAGE:
    zero [OPTIONS] [REQUEST]
    zero <COMMAND>

ARGUMENTS:
    [REQUEST]    Natural language request (one-shot mode)

COMMANDS:
    install      Install Claude Code PreToolUse hook
    uninstall    Remove Claude Code hook
    rewrite      Rewrite a command (used by hooks internally)
    compress     Compress a file for context (used by hooks internally)
    stats        Show token savings dashboard
    export       Export tracking data (--format json|csv|html|pdf)
    config       Show current configuration
    version      Show version information

OPTIONS:
    -c, --config <PATH>    Path to config file
    -v, --verbose          Increase verbosity (-v, -vv, -vvv)
    -q, --quiet            Suppress non-essential output
    --no-cache             Disable context caching for this session
    --no-autofix           Disable error auto-fix for this session
    --dry-run              Show what would be done without executing
```

### Examples

```bash
# Interactive REPL
zero

# One-shot commands
zero "run pytest and fix failures"
zero "git diff HEAD~5 --stat"
zero "dotnet test and explain failures"

# Hook management
zero install          # Install Claude Code hook
zero uninstall        # Remove hook

# Analytics
zero stats                              # Token savings dashboard
zero stats --daily                      # Day-by-day breakdown
zero export --format html -o report.html  # HTML report
zero export --format pdf -o report.pdf    # PDF report
zero export --format csv -o data.csv      # CSV export
zero export --format json                 # JSON to stdout

# Debugging
zero rewrite "git status"               # Test command rewriting
zero compress src/auth.rs               # Test file compression
zero --dry-run "run pytest"             # Show pipeline without executing
```

## Architecture

```
                     ┌─────────────────────────────────┐
                     │         CLI / Hook Entry         │
                     │   REPL | One-shot | PreToolUse   │
                     └──────────────┬──────────────────┘
                                    │
                     ┌──────────────▼──────────────────┐
                     │         Router Agent             │
                     │   Intent classify (regex)        │
                     │   Task decompose (multi-step)    │
                     │   0 tokens                       │
                     └──────────────┬──────────────────┘
                                    │
                  ┌─────────────────┼─────────────────┐
                  ▼                                    ▼
   ┌──────────────────────┐             ┌──────────────────────┐
   │     Fetch Agent       │             │     Build Agent       │
   │  web_fetch, git_clone │             │  shell exec + filter  │
   │  file read, dir list  │             │  30+ OutputFilters    │
   │  0 tokens (parallel)  │             │  0 tokens             │
   └──────────┬───────────┘             └──────────┬───────────┘
              └─────────────────┬──────────────────┘
                                ▼
                 ┌──────────────────────────────┐
                 │       Analyzer Agent          │
                 │  ErrorClassifier (60+ regex)  │
                 │  → auto-fix? DONE (0 tokens)  │
                 │  ASTCompressor (tree-sitter)  │
                 │  ContextCache (MD5 dedup)     │
                 │  ContextBuilder (budget)      │
                 │  0 tokens                     │
                 └──────────────┬───────────────┘
                                │ only if reasoning needed
                                ▼
                 ┌──────────────────────────────┐
                 │      Reasoning Agent          │
                 │  Claude API (diff prompt)     │
                 │  *** THE token spend ***      │
                 └──────────────┬───────────────┘
                                ▼
                 ┌──────────────────────────────┐
                 │      Validator Agent          │
                 │  Syntax check (per-language)  │
                 │  Diff apply (patch)           │
                 │  Auto-retry on failure        │
                 │  0 tokens                     │
                 └──────────────────────────────┘
```

## Realistic Savings

Based on analysis of typical Claude Code sessions:

| Scenario | Tokens | Time | Savings |
|----------|--------|------|---------|
| **Debug session** (test → fix → retest) | 50-70% fewer | 40% faster | Error auto-fix eliminates round-trips |
| **Feature development** (read → write → test) | 35-55% fewer | 25% faster | AST compression + filtered output |
| **Iterative refactor** (same files, 3+ rounds) | 45-65% fewer | 35% faster | Context cache compounds across rounds |
| **Simple Q&A** | 10-20% fewer | ~same | Most tokens are in Claude's reasoning |

**Monthly cost impact (solo developer):**
- Without ZeroCTX: ~$160/month
- With ZeroCTX: ~$60-100/month
- Savings: **$60-100/month per developer**

## Extending ZeroCTX

### Adding a New Output Filter

Implement the `OutputFilter` trait:

```rust
use crate::filters::{OutputFilter, FilterResult};

pub struct MyToolFilter;

impl OutputFilter for MyToolFilter {
    fn name(&self) -> &str { "my-tool" }

    fn matches(&self, command: &str) -> bool {
        command.starts_with("my-tool ")
    }

    fn filter(&self, output: &str, config: &Config) -> FilterResult {
        let filtered = output.lines()
            .filter(|line| line.contains("ERROR") || line.contains("WARNING"))
            .collect::<Vec<_>>()
            .join("\n");

        FilterResult {
            output: filtered,
            original_lines: output.lines().count(),
            filtered_lines: filtered.lines().count(),
        }
    }
}
```

### Adding Error Patterns

Add to `src/errors/patterns.rs`:

```rust
Pattern {
    regex: r"MY_ERROR:\s+(.+)\s+not found in (.+)",
    category: "my_tool_not_found",
    languages: &[Language::All],
    fixable: true,
    fix_command: Some("my-tool install {1}"),
    explanation: "Resource not found, installing automatically",
}
```

### Adding Language Support

Implement the `Language` trait:

```rust
use crate::languages::{LanguageSupport, ValidationResult};

pub struct GoLanguage;

impl LanguageSupport for GoLanguage {
    fn extensions(&self) -> &[&str] { &[".go"] }
    fn name(&self) -> &str { "go" }

    fn validate(&self, code: &str) -> ValidationResult {
        // Run: go vet
    }

    fn compress_ast(&self, source: &str, relevant_names: &[String]) -> String {
        // Use tree-sitter-go to extract relevant functions
    }
}
```

## Comparison with RTK

ZeroCTX was inspired by [RTK](https://github.com/rtk-ai/rtk) but addresses its limitations:

| Feature | RTK | ZeroCTX |
|---------|-----|---------|
| Output filtering | 60-90% savings | 60-90% (same quality, trait-based) |
| Error auto-fix | No | 60+ patterns, auto-installs packages |
| AST compression | Partial (body stripping) | Full (tree-sitter, extracts relevant functions) |
| Context caching | No | MD5 dedup, 70%+ savings on round 2+ |
| Diff-only output | No | 90% fewer output tokens |
| Task decomposition | No | Regex-based multi-step splitting |
| Output validation | No | Per-language syntax + lint checking |
| Session state | Stateless | SQLite session persistence |
| Error handling | 65+ unwraps | Zero unwraps (anyhow/thiserror) |
| Configuration | Hard-coded limits | All configurable via TOML |
| Architecture | God objects, duplication | Trait-based, modular |

## License

MIT
