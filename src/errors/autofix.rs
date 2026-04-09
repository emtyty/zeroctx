use crate::core::types::AutoFix;

impl AutoFix {
    /// Create a fixable auto-fix.
    pub fn fixable(category: &str, explanation: &str, command: &str, language: crate::core::types::Language) -> Self {
        Self {
            fixable: true,
            category: category.to_string(),
            explanation: explanation.to_string(),
            command: Some(command.to_string()),
            language,
        }
    }

    /// Create a non-fixable explanation.
    pub fn explain(category: &str, explanation: &str, language: crate::core::types::Language) -> Self {
        Self {
            fixable: false,
            category: category.to_string(),
            explanation: explanation.to_string(),
            command: None,
            language,
        }
    }

    /// Format the auto-fix result for display.
    pub fn display(&self) -> String {
        if self.fixable {
            format!(
                "[AUTO-FIX] {}\n  Command: {}\n  Category: {}",
                self.explanation,
                self.command.as_deref().unwrap_or("none"),
                self.category
            )
        } else {
            format!(
                "[EXPLAIN] {}\n  Category: {}",
                self.explanation, self.category
            )
        }
    }
}
