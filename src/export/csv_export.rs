use anyhow::Result;

use crate::core::tracking::Tracker;

pub fn export(output_path: Option<&str>, days: u32) -> Result<()> {
    let tracker = Tracker::open(None)?;
    let daily = tracker.get_daily(days)?;

    let mut output = String::from("date,commands,input_tokens,output_tokens,avg_savings\n");
    for d in &daily {
        output.push_str(&format!(
            "{},{},{},{},{:.1}\n",
            d.date, d.commands, d.input_tokens, d.output_tokens, d.avg_savings
        ));
    }

    match output_path {
        Some(path) => {
            std::fs::write(path, &output)?;
            println!("Exported to {}", path);
        }
        None => {
            print!("{}", output);
        }
    }

    Ok(())
}
