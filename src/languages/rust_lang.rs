use crate::core::types::ValidationResult;
use crate::languages::LanguageSupport;

pub struct RustLanguage;

impl LanguageSupport for RustLanguage {
    fn extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn name(&self) -> &str {
        "rust"
    }

    fn validate(&self, code: &str) -> ValidationResult {
        // Use syn::parse_file for Rust syntax validation — no subprocess needed
        match syn::parse_file(code) {
            Ok(_) => ValidationResult {
                valid: true,
                errors: vec![],
                warnings: vec![],
            },
            Err(e) => ValidationResult {
                valid: false,
                errors: vec![format!("Syntax error: {}", e)],
                warnings: vec![],
            },
        }
    }
}
