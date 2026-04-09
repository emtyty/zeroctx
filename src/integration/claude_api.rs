/// Claude API integration.
///
/// The actual API call logic is in agents/reasoning.rs.
/// This module provides additional utilities like token counting
/// and response parsing.

use crate::core::runner::estimate_tokens;

/// Estimate the cost of a Claude API call.
pub fn estimate_cost(input_text: &str, output_tokens: usize, model: &str) -> f64 {
    let input_tokens = estimate_tokens(input_text);

    // Pricing per 1M tokens (approximate, Sonnet 4)
    let (input_price, output_price) = match model {
        m if m.contains("opus") => (15.0, 75.0),
        m if m.contains("sonnet") => (3.0, 15.0),
        m if m.contains("haiku") => (0.25, 1.25),
        _ => (3.0, 15.0), // Default to Sonnet pricing
    };

    let input_cost = (input_tokens as f64 / 1_000_000.0) * input_price;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * output_price;

    input_cost + output_cost
}
