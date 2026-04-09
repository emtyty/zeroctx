use anyhow::Result;

use super::Cli;

/// Run a single request and exit.
pub fn run(request: &str, _cli: &Cli) -> Result<()> {
    let config = crate::config::Config::load()?;
    let rt = tokio::runtime::Runtime::new()?;

    let result = rt.block_on(crate::agents::Pipeline::new(config).execute(request))?;
    println!("{}", result);

    Ok(())
}
