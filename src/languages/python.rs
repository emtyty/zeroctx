use std::io::Write;

use crate::core::types::ValidationResult;
use crate::languages::LanguageSupport;

pub struct PythonLanguage;

impl LanguageSupport for PythonLanguage {
    fn extensions(&self) -> &[&str] {
        &["py", "pyw"]
    }

    fn name(&self) -> &str {
        "python"
    }

    fn validate(&self, code: &str) -> ValidationResult {
        // Try Python ast.parse first
        if let Some(result) = try_python_ast(code) {
            return result;
        }
        // Fallback: basic bracket balance
        bracket_balance(code)
    }
}

fn try_python_ast(code: &str) -> Option<ValidationResult> {
    let mut temp = tempfile::NamedTempFile::new().ok()?;
    temp.write_all(code.as_bytes()).ok()?;
    let temp_path = temp.path().to_string_lossy().to_string();

    let output = std::process::Command::new("python")
        .args(["-c", &format!("import ast; ast.parse(open(r'{}').read())", temp_path)])
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
    for ch in code.chars() {
        match ch {
            '(' | '[' | '{' => stack.push(ch),
            ')' => {
                if stack.pop() != Some('(') {
                    return ValidationResult {
                        valid: false,
                        errors: vec!["Unmatched ')'".into()],
                        warnings: vec![],
                    };
                }
            }
            ']' => {
                if stack.pop() != Some('[') {
                    return ValidationResult {
                        valid: false,
                        errors: vec!["Unmatched ']'".into()],
                        warnings: vec![],
                    };
                }
            }
            '}' => {
                if stack.pop() != Some('{') {
                    return ValidationResult {
                        valid: false,
                        errors: vec!["Unmatched '}'".into()],
                        warnings: vec![],
                    };
                }
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return ValidationResult {
            valid: false,
            errors: vec![format!("Unclosed brackets: {:?}", stack)],
            warnings: vec![],
        };
    }
    ValidationResult { valid: true, errors: vec![], warnings: vec![] }
}
