pub mod dotnet;
pub mod javascript;
pub mod python;
pub mod rust_lang;

use crate::core::types::{Language, ValidationResult};

/// Trait for language-specific support.
pub trait LanguageSupport: Send + Sync {
    /// File extensions handled by this language.
    fn extensions(&self) -> &[&str];

    /// Language name.
    fn name(&self) -> &str;

    /// Validate code syntax.
    fn validate(&self, code: &str) -> ValidationResult;
}

/// Get the language support implementation for a file extension.
pub fn for_extension(ext: &str) -> Option<Box<dyn LanguageSupport>> {
    let lang = Language::from_extension(ext);
    match lang {
        Language::Python => Some(Box::new(python::PythonLanguage)),
        Language::JavaScript | Language::TypeScript => {
            Some(Box::new(javascript::JavaScriptLanguage))
        }
        Language::CSharp => Some(Box::new(dotnet::DotnetLanguage)),
        Language::Rust => Some(Box::new(rust_lang::RustLanguage)),
        _ => None,
    }
}
