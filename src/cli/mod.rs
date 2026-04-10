pub mod oneshot;
pub mod repl;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// ZeroCTX — Agent team that reduces AI coding assistant token usage by 40-60%.
#[derive(Parser, Debug)]
#[command(name = "zero", version, about, long_about = None, args_conflicts_with_subcommands = true)]
pub struct Cli {
    /// Natural language request (one-shot mode).
    /// If omitted, starts interactive REPL.
    #[arg(value_name = "REQUEST")]
    pub request: Option<String>,

    /// Path to config file
    #[arg(short, long)]
    pub config: Option<String>,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress non-essential output
    #[arg(short, long)]
    pub quiet: bool,

    /// Disable context caching for this session
    #[arg(long)]
    pub no_cache: bool,

    /// Disable error auto-fix for this session
    #[arg(long)]
    pub no_autofix: bool,

    /// Show what would be done without executing
    #[arg(long)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Install Claude Code PreToolUse hook
    Install {
        /// Install to .claude/ in current directory instead of user-level ~/.claude/
        #[arg(long)]
        project: bool,
        /// Skip writing CLAUDE.md
        #[arg(long)]
        no_claude_md: bool,
        /// Skip Read hook (only install Bash hook)
        #[arg(long)]
        no_read_hook: bool,
        /// Only install hooks, skip CLAUDE.md and settings patching
        #[arg(long)]
        hook_only: bool,
    },

    /// Remove Claude Code hook
    Uninstall {
        /// Remove from .claude/ in current directory instead of user-level ~/.claude/
        #[arg(long)]
        project: bool,
    },

    /// Rewrite a command (used by hooks internally)
    Rewrite {
        /// The command to rewrite
        command: String,
    },

    /// Execute a command through ZeroCTX filters (used by hooks internally)
    #[command(name = "rewrite-exec")]
    RewriteExec {
        /// The command and arguments to execute
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 1..)]
        args: Vec<String>,
    },

    /// Compress a file for context (used by hooks internally)
    Compress {
        /// The file path to compress
        path: String,
    },

    /// Compress a file and return temp file path (used by Read hook)
    #[command(name = "compress-read")]
    CompressRead {
        /// The file path to compress
        path: String,
    },

    /// Show token savings dashboard
    Stats {
        /// Show daily breakdown
        #[arg(long)]
        daily: bool,
    },

    /// Export tracking data
    Export {
        /// Output format: json, csv, html, pdf
        #[arg(long, short, default_value = "json")]
        format: String,

        /// Output file path (stdout if omitted)
        #[arg(long, short)]
        output: Option<String>,

        /// Number of days to include
        #[arg(long, default_value = "30")]
        days: u32,
    },

    /// Convert a markdown file to HTML or PDF
    Convert {
        /// Path to the markdown file
        path: String,

        /// Output format: html, pdf
        #[arg(long, short, default_value = "html")]
        format: String,

        /// Output file path (auto-generated if omitted)
        #[arg(long, short)]
        output: Option<String>,

        /// Page title (uses filename if omitted)
        #[arg(long)]
        title: Option<String>,
    },

    /// Show current configuration
    Config,

    /// Show version information
    Version,

    /// Show mismatch/quality report
    Mismatch {
        /// Number of days to include (default 30)
        #[arg(long, default_value = "30")]
        days: u32,

        /// Filter by category: intent_routing, output_filter, autofix, compression, language_detection, command_rewrite
        #[arg(long)]
        category: Option<String>,

        /// Max events to show (when filtering by category)
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Export as JSON instead of human-readable
        #[arg(long)]
        json: bool,
    },

    /// Submit feedback on a mismatch event
    Feedback {
        /// Mismatch event ID (shown in mismatch report as #N)
        #[arg(value_name = "EVENT_ID")]
        event_id: i64,

        /// Your feedback message
        #[arg(value_name = "MESSAGE")]
        message: String,
    },

    /// Compress web page HTML to clean text (used by WebFetch hook)
    #[command(name = "compress-web")]
    CompressWeb {
        /// URL being fetched (for tracking)
        url: String,
    },
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Some(Commands::Install { project, no_claude_md, no_read_hook, hook_only }) => {
                crate::hooks::installer::install_full(
                    project,
                    !no_claude_md && !hook_only,
                    !no_read_hook,
                )?;
                Ok(())
            }
            Some(Commands::Uninstall { project }) => {
                crate::hooks::installer::uninstall_with_options(project)?;
                Ok(())
            }
            Some(Commands::Rewrite { command }) => {
                let result = crate::hooks::rewriter::rewrite(&command)?;
                match result {
                    crate::hooks::rewriter::RewriteResult::Rewritten(cmd) => {
                        print!("{}", cmd);
                        std::process::exit(0); // 0 = allow
                    }
                    crate::hooks::rewriter::RewriteResult::Passthrough => {
                        std::process::exit(1); // 1 = no match
                    }
                    crate::hooks::rewriter::RewriteResult::Deny => {
                        std::process::exit(2); // 2 = denied
                    }
                    crate::hooks::rewriter::RewriteResult::Ask(cmd) => {
                        print!("{}", cmd);
                        std::process::exit(3); // 3 = ask user
                    }
                }
            }
            Some(Commands::RewriteExec { args }) => {
                let command = args.join(" ");
                let config = crate::config::Config::load()?;

                // Execute the command
                let output = crate::core::runner::execute_shell(&command, &config)?;

                // Apply output filters
                let registry = crate::filters::FilterRegistry::new();
                let filtered = registry.apply(&command, &output.stdout, &config);

                // Check for auto-fixable errors
                if output.exit_code != 0 && config.autofix.enabled {
                    let cwd = std::env::current_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| ".".into());
                    if let Some(fix) = crate::errors::classify(&output.stderr, &output.stdout, &cwd) {
                        if fix.fixable && config.autofix.auto_run {
                            // Use verify variant to track auto-fix quality
                            if let Ok(result) = crate::errors::execute_fix_and_verify(
                                &fix, &command, &output.stderr,
                            ) {
                                eprintln!("{}", result);
                            }
                        } else {
                            eprintln!("[ZeroCTX] {}", fix.explanation);
                        }
                    }
                }

                // Track savings
                let input_tokens = crate::core::runner::estimate_tokens(&output.stdout);
                let output_tokens = crate::core::runner::estimate_tokens(&filtered.output);
                let savings_pct = if input_tokens > 0 && input_tokens > output_tokens {
                    ((input_tokens - output_tokens) as f64 / input_tokens as f64 * 100.0) as i32
                } else {
                    0
                };
                if let Ok(tracker) = crate::core::tracking::Tracker::open(None) {
                    let _ = tracker.record(
                        &command,
                        input_tokens,
                        output_tokens,
                        crate::core::types::SavingsMethod::OutputFilter,
                    );
                }

                // Print filtered output
                print!("{}", filtered.output);
                if !output.stderr.is_empty() {
                    eprint!("{}", output.stderr);
                }

                // UX feedback: show savings one-liner
                if savings_pct > 5 {
                    eprintln!(
                        "\n[ZeroCTX: {}→{} lines, {}% saved]",
                        filtered.original_lines, filtered.filtered_lines, savings_pct
                    );
                }

                // Preserve exit code
                std::process::exit(output.exit_code);
            }
            Some(Commands::Compress { path }) => {
                let config = crate::config::Config::load()?;
                let compressed = crate::compression::compress_file(&path, &config)?;
                print!("{}", compressed);
                Ok(())
            }
            Some(Commands::CompressRead { path }) => {
                let config = crate::config::Config::load()?;
                match crate::compression::compress_to_temp(&path, &config) {
                    Ok(temp_path) => {
                        // Track savings
                        if let Ok(original) = std::fs::read_to_string(&path) {
                            if let Ok(compressed) = std::fs::read_to_string(&temp_path) {
                                let input_tokens = crate::core::runner::estimate_tokens(&original);
                                let output_tokens = crate::core::runner::estimate_tokens(&compressed);
                                if let Ok(tracker) = crate::core::tracking::Tracker::open(None) {
                                    let _ = tracker.record(
                                        &format!("read {}", path),
                                        input_tokens,
                                        output_tokens,
                                        crate::core::types::SavingsMethod::AstCompression,
                                    );
                                }
                            }
                        }

                        // Track repeated reads of the same file (potential compression mismatch)
                        crate::core::mismatch::log_signal(
                            "file_read",
                            &format!("read {}", path),
                            &format!("{{\"path\": \"{}\"}}", path),
                        );

                        print!("{}", temp_path);
                        std::process::exit(0);
                    }
                    Err(_) => {
                        // Compression failed — pass through original
                        std::process::exit(1);
                    }
                }
            }
            Some(Commands::Stats { daily }) => {
                crate::export::print_stats(daily)?;
                Ok(())
            }
            Some(Commands::Export {
                format,
                output,
                days,
            }) => {
                crate::export::export_data(&format, output.as_deref(), days)?;
                Ok(())
            }
            Some(Commands::Convert {
                path,
                format,
                output,
                title,
            }) => {
                crate::export::convert::convert_file(&path, &format, output.as_deref(), title.as_deref())?;
                Ok(())
            }
            Some(Commands::Config) => {
                let config = crate::config::Config::load()?;
                println!("{}", toml::to_string_pretty(&config).unwrap_or_default());
                Ok(())
            }
            Some(Commands::CompressWeb { url }) => {
                use std::io::Read as _;
                let mut html = String::new();
                std::io::stdin().read_to_string(&mut html).unwrap_or(0);
                let clean = crate::agents::fetch::strip_html(&html);
                let input_tokens = crate::core::runner::estimate_tokens(&html);
                let output_tokens = crate::core::runner::estimate_tokens(&clean);
                if let Ok(tracker) = crate::core::tracking::Tracker::open(None) {
                    let _ = tracker.record(
                        &format!("webfetch {}", url),
                        input_tokens,
                        output_tokens,
                        crate::core::types::SavingsMethod::OutputFilter,
                    );
                }
                let temp_dir = std::env::temp_dir().join("zeroctx");
                std::fs::create_dir_all(&temp_dir).ok();
                let temp_path = temp_dir.join("webfetch_clean.txt");
                std::fs::write(&temp_path, &clean).ok();
                print!("{}", temp_path.display());
                std::process::exit(0);
            }
            Some(Commands::Version) => {
                println!("zeroctx {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
            Some(Commands::Mismatch { days, category, limit, json }) => {
                let tracker = crate::core::mismatch::MismatchTracker::open(None)?;

                if let Some(cat) = category {
                    // Show events for a specific category
                    let events = tracker.get_events_by_category(&cat, limit)?;
                    if json {
                        let items: Vec<serde_json::Value> = events.iter().map(|e| {
                            serde_json::json!({
                                "id": e.id,
                                "timestamp": e.timestamp,
                                "category": e.category,
                                "severity": e.severity,
                                "detected": e.detected,
                                "actual": e.actual,
                                "input_snippet": e.input_snippet,
                                "context": e.context,
                            })
                        }).collect();
                        println!("{}", serde_json::to_string_pretty(&items)?);
                    } else {
                        println!("=== Mismatch Events: {} (last {} days) ===\n", cat, days);
                        if events.is_empty() {
                            println!("  No events found for category '{}'.", cat);
                        }
                        for e in &events {
                            let sev = match e.severity.as_str() {
                                "error" => "ERR",
                                "warn" => "WRN",
                                _ => "INF",
                            };
                            println!("  [{}] #{} {}", sev, e.id, e.timestamp);
                            println!("       Detected: {}", e.detected);
                            if !e.actual.is_empty() {
                                println!("       Actual:   {}", e.actual);
                            }
                            if !e.input_snippet.is_empty() {
                                println!("       Input:    {}", &e.input_snippet.chars().take(100).collect::<String>());
                            }
                            if !e.context.is_empty() {
                                println!("       Context:  {}", &e.context.chars().take(100).collect::<String>());
                            }
                            println!();
                        }
                        println!("Tip: Use `zero feedback <ID> \"your message\"` to annotate events.");
                    }
                } else {
                    // Show full report
                    let report = tracker.get_report(days)?;
                    if json {
                        let j = serde_json::json!({
                            "days": report.days,
                            "total_events": report.total_events,
                            "total_signals": report.total_signals,
                            "categories": report.categories.iter().map(|c| serde_json::json!({
                                "category": c.category,
                                "severity": c.severity,
                                "count": c.count,
                                "examples": c.examples,
                            })).collect::<Vec<_>>(),
                            "signals": report.signals.iter().map(|s| serde_json::json!({
                                "signal_type": s.signal_type,
                                "count": s.count,
                                "operations": s.operations,
                            })).collect::<Vec<_>>(),
                            "unresolved_count": report.unresolved.len(),
                        });
                        println!("{}", serde_json::to_string_pretty(&j)?);
                    } else {
                        print!("{}", crate::core::mismatch::format_report(&report));
                    }
                }
                Ok(())
            }
            Some(Commands::Feedback { event_id, message }) => {
                let tracker = crate::core::mismatch::MismatchTracker::open(None)?;
                tracker.add_feedback(event_id, &message)?;
                println!("Feedback recorded for event #{}. Marked as resolved.", event_id);
                Ok(())
            }
            None => {
                // No subcommand: check for request or start REPL
                if let Some(ref request) = self.request {
                    oneshot::run(request, &self)
                } else {
                    repl::run(&self)
                }
            }
        }
    }
}
