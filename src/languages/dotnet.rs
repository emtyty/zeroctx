use crate::core::types::ValidationResult;
use crate::languages::LanguageSupport;

pub struct DotnetLanguage;

impl LanguageSupport for DotnetLanguage {
    fn extensions(&self) -> &[&str] {
        &["cs", "razor", "cshtml"]
    }

    fn name(&self) -> &str {
        "csharp"
    }

    fn validate(&self, code: &str) -> ValidationResult {
        // C# bracket/brace balance check (fast, no subprocess)
        bracket_balance(code)
    }
}

fn bracket_balance(code: &str) -> ValidationResult {
    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut in_verbatim = false;
    let mut prev = ' ';

    for ch in code.chars() {
        if in_string {
            if in_verbatim {
                if ch == '"' && prev == '"' {
                    prev = ' '; // escaped quote in verbatim string
                    continue;
                } else if ch == '"' {
                    in_string = false;
                    in_verbatim = false;
                }
            } else if ch == '"' && prev != '\\' {
                in_string = false;
            }
            prev = ch;
            continue;
        }

        if ch == '"' {
            in_string = true;
            in_verbatim = prev == '@';
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
            errors: vec![format!("Unclosed brackets: {:?}", stack)],
            warnings: vec![],
        };
    }
    ValidationResult { valid: true, errors: vec![], warnings: vec![] }
}
