use crate::config::Config;
use crate::core::types::FilterResult;
use crate::filters::OutputFilter;

pub struct DotnetFilter;

impl OutputFilter for DotnetFilter {
    fn name(&self) -> &str {
        "dotnet"
    }

    fn matches(&self, command: &str) -> bool {
        command.starts_with("dotnet ") || command.starts_with("msbuild ") || command.starts_with("nuget ")
    }

    fn filter(&self, output: &str, _config: &Config) -> FilterResult {
        let original_lines = output.lines().count();

        let filtered = if output.contains("test result:") || output.contains("Passed!") || output.contains("Failed!") || output.contains("Total tests:") {
            filter_dotnet_test(output)
        } else {
            filter_dotnet_build(output)
        };

        let filtered_lines = filtered.lines().count();
        let savings = if original_lines > 0 {
            (1.0 - filtered_lines as f64 / original_lines as f64) * 100.0
        } else {
            0.0
        };

        FilterResult {
            output: filtered,
            original_lines,
            filtered_lines,
            savings_percent: savings,
        }
    }
}

/// dotnet build: extract errors + warnings + result
fn filter_dotnet_build(output: &str) -> String {
    let mut result = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Keep error and warning lines
        if trimmed.contains(": error ") || trimmed.contains(": warning ") {
            result.push(line.to_string());
            continue;
        }

        // Keep build result lines
        if trimmed.contains("Build succeeded")
            || trimmed.contains("Build FAILED")
            || trimmed.contains("Error(s)")
            || trimmed.contains("Warning(s)")
            || trimmed.starts_with("Time Elapsed")
        {
            result.push(line.to_string());
            continue;
        }

        // Skip restore noise
        if trimmed.starts_with("Restore")
            || trimmed.starts_with("Determining projects")
            || trimmed.starts_with("All projects")
            || trimmed.starts_with("  Generating MSBuild")
            || trimmed.is_empty()
        {
            continue;
        }
    }

    if result.is_empty() {
        output.to_string()
    } else {
        result.join("\n")
    }
}

/// dotnet test: extract failed tests + summary
fn filter_dotnet_test(output: &str) -> String {
    let mut result = Vec::new();
    let mut in_failure = false;
    let has_failures = output.contains("Failed!") || output.contains("Failed:");

    for line in output.lines() {
        let trimmed = line.trim();

        // Summary (always keep)
        if trimmed.starts_with("Total tests:") || trimmed.starts_with("Passed!") || trimmed.starts_with("Failed!") || trimmed.starts_with("Test Run") {
            result.push(line.to_string());
            continue;
        }

        // Skip passing tests if there are failures
        if has_failures && trimmed.starts_with("Passed ") {
            continue;
        }

        // Failed test
        if trimmed.starts_with("Failed ") || trimmed.contains("[FAIL]") {
            result.push(line.to_string());
            in_failure = true;
            continue;
        }

        // Error detail after failure
        if in_failure {
            result.push(line.to_string());
            if trimmed.is_empty() || trimmed.starts_with("---") {
                in_failure = false;
            }
        }
    }

    if result.is_empty() {
        output.to_string()
    } else {
        result.join("\n")
    }
}
