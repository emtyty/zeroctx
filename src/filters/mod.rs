pub mod dotnet;
pub mod git;
pub mod javascript;
pub mod network;
pub mod python;
pub mod rust_tools;
pub mod system;
pub mod toml_filter;

use crate::config::Config;
use crate::core::types::FilterResult;

/// Trait for output filters. Each filter handles a specific command category.
///
/// This replaces RTK's ~30 duplicated filter functions with a single trait.
pub trait OutputFilter: Send + Sync {
    /// Name of this filter (for logging/tracking).
    fn name(&self) -> &str;

    /// Whether this filter handles the given command.
    fn matches(&self, command: &str) -> bool;

    /// Filter/compress the command output.
    fn filter(&self, output: &str, config: &Config) -> FilterResult;
}

/// Registry of all output filters.
pub struct FilterRegistry {
    filters: Vec<Box<dyn OutputFilter>>,
}

impl FilterRegistry {
    /// Create a registry with all built-in filters.
    pub fn new() -> Self {
        let filters: Vec<Box<dyn OutputFilter>> = vec![
            Box::new(git::GitFilter),
            Box::new(python::PythonFilter),
            Box::new(javascript::JavaScriptFilter),
            Box::new(dotnet::DotnetFilter),
            Box::new(rust_tools::RustFilter),
            Box::new(system::SystemFilter),
            Box::new(network::NetworkFilter),
        ];
        Self { filters }
    }

    /// Find the best matching filter for a command and apply it.
    pub fn apply(&self, command: &str, output: &str, config: &Config) -> FilterResult {
        for filter in &self.filters {
            if filter.matches(command) {
                return filter.filter(output, config);
            }
        }
        // No filter matched — passthrough
        FilterResult::passthrough(output.to_string())
    }
}

impl Default for FilterRegistry {
    fn default() -> Self {
        Self::new()
    }
}
