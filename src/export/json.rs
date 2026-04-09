use anyhow::Result;

use crate::core::tracking::Tracker;

pub fn export(output_path: Option<&str>, days: u32) -> Result<()> {
    let tracker = Tracker::open(None)?;
    let summary = tracker.get_summary()?;
    let daily = tracker.get_daily(days)?;
    let by_method = tracker.get_by_method()?;

    let data = serde_json::json!({
        "summary": {
            "total_commands": summary.total_commands,
            "total_input_tokens": summary.total_input_tokens,
            "total_output_tokens": summary.total_output_tokens,
            "tokens_saved": summary.total_input_tokens.saturating_sub(summary.total_output_tokens),
            "avg_savings_percent": summary.avg_savings_percent,
        },
        "daily": daily.iter().map(|d| serde_json::json!({
            "date": d.date,
            "commands": d.commands,
            "input_tokens": d.input_tokens,
            "output_tokens": d.output_tokens,
            "avg_savings": d.avg_savings,
        })).collect::<Vec<_>>(),
        "by_method": by_method.iter().map(|m| serde_json::json!({
            "method": m.method,
            "count": m.count,
            "tokens_saved": m.tokens_saved,
            "avg_savings": m.avg_savings,
        })).collect::<Vec<_>>(),
    });

    let json_str = serde_json::to_string_pretty(&data)?;

    match output_path {
        Some(path) => {
            std::fs::write(path, &json_str)?;
            println!("Exported to {}", path);
        }
        None => {
            println!("{}", json_str);
        }
    }

    Ok(())
}
