pub mod convert;
pub mod csv_export;
pub mod html;
pub mod json;
pub mod pdf;

use anyhow::Result;

use crate::core::tracking::Tracker;

/// Print token savings dashboard to stdout.
pub fn print_stats(daily: bool) -> Result<()> {
    let tracker = Tracker::open(None)?;
    let summary = tracker.get_summary()?;

    println!("=== ZeroCTX Token Savings ===\n");
    println!("Total commands:     {}", summary.total_commands);
    println!("Total input tokens: {}", summary.total_input_tokens);
    println!("Total output tokens:{}", summary.total_output_tokens);
    println!(
        "Tokens saved:       {}",
        summary.total_input_tokens.saturating_sub(summary.total_output_tokens)
    );
    println!("Average savings:    {:.1}%", summary.avg_savings_percent);

    if daily {
        println!("\n--- Daily Breakdown (last 30 days) ---\n");
        let days = tracker.get_daily(30)?;
        for day in days {
            println!(
                "  {} | {} cmds | {} → {} tokens | {:.1}%",
                day.date, day.commands, day.input_tokens, day.output_tokens, day.avg_savings
            );
        }
    }

    let by_method = tracker.get_by_method()?;
    if !by_method.is_empty() {
        println!("\n--- Savings by Method ---\n");
        for m in by_method {
            println!(
                "  {:20} | {:5} uses | {:8} tokens saved | {:.1}%",
                m.method, m.count, m.tokens_saved, m.avg_savings
            );
        }
    }

    Ok(())
}

/// Export tracking data in the specified format.
pub fn export_data(format: &str, output_path: Option<&str>, days: u32) -> Result<()> {
    match format {
        "json" => json::export(output_path, days),
        "csv" => csv_export::export(output_path, days),
        "html" => html::export(output_path, days),
        "pdf" => pdf::export(output_path, days),
        _ => anyhow::bail!("Unsupported export format: {}. Use json, csv, html, or pdf.", format),
    }
}

/// Print a live session cost estimate to stdout.
/// With watch=true, refreshes every 30 seconds.
pub fn print_session(hours: u32, watch: bool) -> Result<()> {
    loop {
        print_session_once(hours)?;
        if !watch {
            break;
        }
        println!("\n  [Refreshing in 30s — Ctrl+C to exit]");
        std::thread::sleep(std::time::Duration::from_secs(30));
        // Clear last output (ANSI erase screen)
        print!("\x1b[2J\x1b[H");
    }
    Ok(())
}

fn print_session_once(hours: u32) -> Result<()> {
    let tracker = crate::core::tracking::Tracker::open(None)?;
    let s = tracker.get_session_summary(hours)?;

    let cost_with = s.estimated_cost_usd();
    let cost_without = s.estimated_cost_without_usd();
    let savings_pct = s.savings_percent();

    println!("=== ZeroCTX Session ({} hours) ===", hours);
    if !s.session_start.is_empty() {
        println!("  Started:  {}", s.session_start);
    }
    println!("  Commands: {}", s.commands_run);
    println!(
        "  Tokens:   ~{}K input / ~{}K output  (${:.3} est.)",
        s.total_input_tokens / 1000,
        s.total_output_tokens / 1000,
        cost_with,
    );
    if s.tokens_saved > 0 {
        println!(
            "  Saved:    ~{}K tokens ({:.0}%) via ZeroCTX  (${:.3} saved)",
            s.tokens_saved / 1000,
            savings_pct,
            cost_without - cost_with,
        );
        println!(
            "  Without:  ~{}K tokens would have been used  (${:.3} est.)",
            (s.total_input_tokens as i64 + s.tokens_saved) / 1000,
            cost_without,
        );
    } else {
        println!("  No savings recorded yet — run commands through `zero rewrite-exec --`");
    }
    println!("==================================");
    Ok(())
}
