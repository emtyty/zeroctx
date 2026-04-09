use anyhow::Result;
use std::io::{self, BufRead, Write};

use super::Cli;

/// Run the interactive REPL mode.
pub fn run(_cli: &Cli) -> Result<()> {
    println!("ZeroCTX v{} — Interactive Mode", env!("CARGO_PKG_VERSION"));
    println!("Type a request, or 'quit' to exit.\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("> ");
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            // EOF
            break;
        }

        let request = line.trim();
        if request.is_empty() {
            continue;
        }
        if request == "quit" || request == "exit" {
            break;
        }
        if request == "stats" {
            crate::export::print_stats(false)?;
            continue;
        }

        // Run the pipeline
        let config = crate::config::Config::load()?;
        let rt = tokio::runtime::Runtime::new()?;
        match rt.block_on(crate::agents::Pipeline::new(config).execute(request)) {
            Ok(result) => println!("{}\n", result),
            Err(e) => eprintln!("Error: {:#}\n", e),
        }
    }

    println!("Goodbye.");
    Ok(())
}
