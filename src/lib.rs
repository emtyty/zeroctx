pub mod agents;
pub mod cli;
pub mod compression;
pub mod config;
pub mod core;
pub mod errors;
pub mod export;
pub mod filters;
pub mod hooks;
pub mod integration;
pub mod languages;

/// Run the full ZeroCTX pipeline on a natural language request.
///
/// This is the main entry point for library usage.
/// Routes through: Router → Fetch/Build → Analyzer → Reasoning → Validator
pub async fn run(request: &str) -> anyhow::Result<String> {
    let config = config::Config::load()?;
    let pipeline = agents::Pipeline::new(config);
    pipeline.execute(request).await
}
