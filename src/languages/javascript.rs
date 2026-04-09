use std::io::Write;

use crate::core::types::ValidationResult;
use crate::languages::LanguageSupport;

pub struct JavaScriptLanguage;

impl LanguageSupport for JavaScriptLanguage {
    fn extensions(&self) -> &[&str] {
        &["js", "jsx", "ts", "tsx", "mjs", "cjs"]
    }

    fn name(&self) -> &str {
        "javascript"
    }

    fn validate(&self, code: &str) -> ValidationResult {
        // Try node --check for JS
        if let Some(result) = try_node_check(code) {
            return result;
        }
        // Fallback: bracket balance
        bracket_balance(code)
    }
}

fn try_node_check(code: &str) -> Option<ValidationResult> {
    let mut temp = tempfile::NamedTempFile::with_suffix(".js").ok()?;
    temp.write_all(code.as_bytes()).ok()?;
    let temp_path = temp.path().to_string_lossy().to_string();

    let output = std::process::Command::new("node")
        .args(["--check", &temp_path])
        .output()
        .ok()?;

    if output.status.success() {
        Some(ValidationResult {
            valid: true,
            errors: vec![],
            warnings: vec![],
        })
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Some(ValidationResult {
            valid: false,
            errors: vec![stderr],
            warnings: vec![],
        })
    }
}

fn bracket_balance(code: &str) -> ValidationResult {
    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut string_char = ' ';
    let mut prev = ' ';

    for ch in code.chars() {
        // Skip string contents
        if in_string {
            if ch == string_char && prev != '\\' {
                in_string = false;
            }
            prev = ch;
            continue;
        }
        if (ch == '"' || ch == '\'' || ch == '`') && prev != '\\' {
            in_string = true;
            string_char = ch;
            prev = ch;
            continue;
        }

        match ch {
            '(' | '[' | '{' => stack.push(ch),
            ')' if stack.pop() != Some('(') => {
                return ValidationResult { valid: false, errors: vec!["Unmatched ')'".into()], warnings: vec![] };
            }
            ']' if stack.pop() != Some('[') => {
                return ValidationResult { valid: false, errors: vec!["Unmatched ']'".into()], warnings: vec![] };
            }
            '}' if stack.pop() != Some('{') => {
                return ValidationResult { valid: false, errors: vec!["Unmatched '}'".into()], warnings: vec![] };
            }
            _ => {}
        }
        prev = ch;
    }

    if !stack.is_empty() {
        return ValidationResult {
            valid: false,
            errors: vec![format!("Unclosed: {:?}", stack)],
            warnings: vec![],
        };
    }
    ValidationResult { valid: true, errors: vec![], warnings: vec![] }
}
