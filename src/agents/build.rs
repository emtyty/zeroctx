use anyhow::Result;

use crate::config::Config;
use crate::core::runner;
use crate::core::types::FilterResult;
use crate::filters::FilterRegistry;

/// Build agent: execute shell commands and filter output.
pub fn execute_and_filter(command: &str, config: &Config) -> Result<(FilterResult, String, i32)> {
    let output = runner::execute_shell(command, config)?;

    let registry = FilterRegistry::new();
    let filtered = registry.apply(command, &output.stdout, config);

    Ok((filtered, output.stderr, output.exit_code))
}
