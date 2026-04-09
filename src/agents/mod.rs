pub mod analyzer;
pub mod build;
pub mod fetch;
pub mod reasoning;
pub mod router;
pub mod validator;

use anyhow::Result;

use crate::config::Config;

/// The main pipeline that orchestrates all agents.
pub struct Pipeline {
    config: Config,
}

impl Pipeline {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Execute the full pipeline on a natural language request.
    ///
    /// Flow: Router → Fetch/Build (parallel) → Analyzer → Reasoning → Validator
    pub async fn execute(&self, request: &str) -> Result<String> {
        // Step 1: Route the request
        let parsed = router::IntentRouter::parse(request);
        tracing::info!(intent = ?parsed.intent, "Request routed");

        // Step 2: Execute I/O (fetch + build in parallel)
        let io_results = self.execute_io(&parsed).await?;

        // Step 3: Analyze — error classify, AST compress, context build
        let analysis = analyzer::analyze(&io_results, &parsed, &self.config)?;

        // Step 4: Check if error was auto-fixed (short-circuit)
        if let Some(fix_result) = &analysis.auto_fix_result {
            return Ok(fix_result.clone());
        }

        // Step 5: Call Claude for reasoning (THE token spend)
        let response = reasoning::call_claude(&analysis.context, &parsed, &self.config).await?;

        // Step 6: Validate and apply
        let validated = validator::validate_and_apply(&response, &parsed, &self.config)?;

        Ok(validated)
    }

    async fn execute_io(
        &self,
        parsed: &crate::core::types::ParsedRequest,
    ) -> Result<IoResults> {
        let mut results = IoResults::default();

        // Execute commands
        for cmd in &parsed.commands {
            let output = crate::core::runner::execute_shell(cmd, &self.config)?;

            // Apply output filters
            let registry = crate::filters::FilterRegistry::new();
            let filtered = registry.apply(cmd, &output.stdout, &self.config);

            results.command_outputs.push((cmd.clone(), filtered.output, output.stderr.clone(), output.exit_code));
        }

        // Read files
        for file in &parsed.files {
            if let Ok(content) = std::fs::read_to_string(file) {
                results.file_contents.push((file.display().to_string(), content));
            }
        }

        // Fetch URLs
        for url in &parsed.urls {
            match fetch::fetch_url(url).await {
                Ok(content) => results.web_contents.push((url.clone(), content)),
                Err(e) => tracing::warn!(url, error = %e, "Failed to fetch URL"),
            }
        }

        Ok(results)
    }
}

/// Results from the I/O phase (Fetch + Build agents).
#[derive(Default, Debug)]
pub struct IoResults {
    /// (command, filtered_stdout, stderr, exit_code)
    pub command_outputs: Vec<(String, String, String, i32)>,
    /// (file_path, content)
    pub file_contents: Vec<(String, String)>,
    /// (url, content)
    pub web_contents: Vec<(String, String)>,
}
