use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

use crate::core::types::{AutoFix, Language};

/// A compiled error pattern with its handler.
pub struct ErrorPattern {
    pub regex: Regex,
    pub category: &'static str,
    pub language: Language,
    pub handler: fn(&regex::Captures, &str, &str) -> AutoFix,
}

/// Return all compiled error patterns across all languages.
pub fn all_patterns() -> &'static [ErrorPattern] {
    static PATTERNS: Lazy<Vec<ErrorPattern>> = Lazy::new(|| {
        let mut patterns = Vec::new();
        patterns.extend(python_patterns());
        patterns.extend(javascript_patterns());
        patterns.extend(dotnet_patterns());
        patterns.extend(rust_patterns());
        patterns
    });
    &PATTERNS
}

// ============================================================
// Python Patterns (15+)
// ============================================================

fn python_patterns() -> Vec<ErrorPattern> {
    // Known import name → pip package mappings
    static PIP_MAPPINGS: Lazy<HashMap<&str, &str>> = Lazy::new(|| {
        let mut m = HashMap::new();
        m.insert("cv2", "opencv-python");
        m.insert("PIL", "Pillow");
        m.insert("sklearn", "scikit-learn");
        m.insert("yaml", "PyYAML");
        m.insert("bs4", "beautifulsoup4");
        m.insert("attr", "attrs");
        m.insert("dateutil", "python-dateutil");
        m.insert("dotenv", "python-dotenv");
        m.insert("gi", "PyGObject");
        m.insert("serial", "pyserial");
        m
    });

    vec![
        // ModuleNotFoundError: No module named 'X'
        ErrorPattern {
            regex: Regex::new(r"ModuleNotFoundError: No module named '([^']+)'")
                .expect("valid regex"),
            category: "python_module_not_found",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let module = captures.get(1).map_or("", |m| m.as_str());
                let top_level = module.split('.').next().unwrap_or(module);
                let package = PIP_MAPPINGS
                    .get(top_level)
                    .copied()
                    .unwrap_or(top_level);
                AutoFix {
                    fixable: true,
                    category: "python_module_not_found".into(),
                    explanation: format!("Module '{}' not installed", module),
                    command: Some(format!("pip install {}", package)),
                    language: Language::Python,
                }
            },
        },
        // SyntaxError
        ErrorPattern {
            regex: Regex::new(r"SyntaxError: (.+)").expect("valid regex"),
            category: "python_syntax_error",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let detail = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_syntax_error".into(),
                    explanation: format!("Python syntax error: {}", detail),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // TypeError: X() takes N positional arguments but M were given
        ErrorPattern {
            regex: Regex::new(r"TypeError: (\w+)\(\) takes (\d+) positional arguments? but (\d+) (?:was|were) given")
                .expect("valid regex"),
            category: "python_type_error_args",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let func = captures.get(1).map_or("", |m| m.as_str());
                let expected = captures.get(2).map_or("", |m| m.as_str());
                let got = captures.get(3).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_type_error_args".into(),
                    explanation: format!(
                        "{}() expects {} argument(s) but got {}. Check the function signature.",
                        func, expected, got
                    ),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // NameError: name 'X' is not defined
        ErrorPattern {
            regex: Regex::new(r"NameError: name '([^']+)' is not defined").expect("valid regex"),
            category: "python_name_error",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let name = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_name_error".into(),
                    explanation: format!(
                        "'{}' is not defined. Check for typos or missing imports.",
                        name
                    ),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // KeyError
        ErrorPattern {
            regex: Regex::new(r"KeyError: (.+)").expect("valid regex"),
            category: "python_key_error",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let key = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_key_error".into(),
                    explanation: format!("Key {} not found in dictionary. Use .get() for safe access.", key),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // FileNotFoundError
        ErrorPattern {
            regex: Regex::new(r"FileNotFoundError: \[Errno 2\] No such file or directory: '([^']+)'")
                .expect("valid regex"),
            category: "python_file_not_found",
            language: Language::Python,
            handler: |captures, _full, cwd| {
                let path = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_file_not_found".into(),
                    explanation: format!(
                        "File '{}' not found. Working directory: {}",
                        path, cwd
                    ),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // PermissionError
        ErrorPattern {
            regex: Regex::new(r"PermissionError: \[Errno 13\] Permission denied: '([^']+)'")
                .expect("valid regex"),
            category: "python_permission_error",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let path = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_permission_error".into(),
                    explanation: format!("Permission denied for '{}'. Check file permissions.", path),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // ImportError: cannot import name 'X' from 'Y'
        ErrorPattern {
            regex: Regex::new(r"ImportError: cannot import name '([^']+)' from '([^']+)'")
                .expect("valid regex"),
            category: "python_import_error",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let name = captures.get(1).map_or("", |m| m.as_str());
                let module = captures.get(2).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_import_error".into(),
                    explanation: format!(
                        "Cannot import '{}' from '{}'. The name may have been renamed or removed. Check the module's API.",
                        name, module
                    ),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // IndexError
        ErrorPattern {
            regex: Regex::new(r"IndexError: (.+)").expect("valid regex"),
            category: "python_index_error",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let detail = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_index_error".into(),
                    explanation: format!("Index out of range: {}. Check collection length before accessing.", detail),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // AttributeError
        ErrorPattern {
            regex: Regex::new(r"AttributeError: '(\w+)' object has no attribute '(\w+)'").expect("valid regex"),
            category: "python_attribute_error",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let obj_type = captures.get(1).map_or("", |m| m.as_str());
                let attr = captures.get(2).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_attribute_error".into(),
                    explanation: format!("'{}' object has no attribute '{}'. Check spelling or use dir() to list available attributes.", obj_type, attr),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // ValueError
        ErrorPattern {
            regex: Regex::new(r"ValueError: (.+)").expect("valid regex"),
            category: "python_value_error",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let detail = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_value_error".into(),
                    explanation: format!("ValueError: {}. Check the input value and expected type.", detail),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // ZeroDivisionError
        ErrorPattern {
            regex: Regex::new(r"ZeroDivisionError").expect("valid regex"),
            category: "python_zero_division",
            language: Language::Python,
            handler: |_captures, _full, _cwd| {
                AutoFix {
                    fixable: false,
                    category: "python_zero_division".into(),
                    explanation: "Division by zero. Add a guard: `if divisor != 0:` before dividing.".into(),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // RecursionError
        ErrorPattern {
            regex: Regex::new(r"RecursionError: maximum recursion depth exceeded").expect("valid regex"),
            category: "python_recursion",
            language: Language::Python,
            handler: |_captures, _full, _cwd| {
                AutoFix {
                    fixable: false,
                    category: "python_recursion".into(),
                    explanation: "Maximum recursion depth exceeded. Check for infinite recursion or convert to iterative approach.".into(),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // ConnectionError / ConnectionRefusedError
        ErrorPattern {
            regex: Regex::new(r"(?:ConnectionError|ConnectionRefusedError|ConnectionResetError): (.+)").expect("valid regex"),
            category: "python_connection_error",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let detail = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_connection_error".into(),
                    explanation: format!("Connection error: {}. Check network connectivity and target service.", detail),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // JSONDecodeError
        ErrorPattern {
            regex: Regex::new(r"json\.decoder\.JSONDecodeError: (.+)").expect("valid regex"),
            category: "python_json_error",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let detail = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_json_error".into(),
                    explanation: format!("Invalid JSON: {}. Check the input string for syntax errors.", detail),
                    command: None,
                    language: Language::Python,
                }
            },
        },
        // UnicodeDecodeError
        ErrorPattern {
            regex: Regex::new(r"UnicodeDecodeError: '(\w+)' codec can't decode").expect("valid regex"),
            category: "python_unicode_error",
            language: Language::Python,
            handler: |captures, _full, _cwd| {
                let codec = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "python_unicode_error".into(),
                    explanation: format!("Cannot decode with '{}' codec. Try: open(file, encoding='utf-8') or encoding='latin-1'.", codec),
                    command: None,
                    language: Language::Python,
                }
            },
        },
    ]
}

// ============================================================
// JavaScript/TypeScript Patterns (15+)
// ============================================================

fn javascript_patterns() -> Vec<ErrorPattern> {
    vec![
        // Cannot find module 'X'
        ErrorPattern {
            regex: Regex::new(r"Cannot find module '([^']+)'").expect("valid regex"),
            category: "js_module_not_found",
            language: Language::JavaScript,
            handler: |captures, _full, _cwd| {
                let module = captures.get(1).map_or("", |m| m.as_str());
                // Skip relative imports
                if module.starts_with('.') || module.starts_with('/') {
                    return AutoFix {
                        fixable: false,
                        category: "js_module_not_found".into(),
                        explanation: format!("Local module '{}' not found. Check the file path.", module),
                        command: None,
                        language: Language::JavaScript,
                    };
                }
                let package = module.split('/').next().unwrap_or(module);
                AutoFix {
                    fixable: true,
                    category: "js_module_not_found".into(),
                    explanation: format!("Module '{}' not installed", module),
                    command: Some(format!("npm install {}", package)),
                    language: Language::JavaScript,
                }
            },
        },
        // TS2307: Cannot find module 'X' or its type declarations
        ErrorPattern {
            regex: Regex::new(r"TS2307: Cannot find module '([^']+)'").expect("valid regex"),
            category: "ts_module_not_found",
            language: Language::TypeScript,
            handler: |captures, _full, _cwd| {
                let module = captures.get(1).map_or("", |m| m.as_str());
                if module.starts_with('.') {
                    return AutoFix {
                        fixable: false,
                        category: "ts_module_not_found".into(),
                        explanation: format!("Local module '{}' not found.", module),
                        command: None,
                        language: Language::TypeScript,
                    };
                }
                let package = module.split('/').next().unwrap_or(module);
                AutoFix {
                    fixable: true,
                    category: "ts_module_not_found".into(),
                    explanation: format!("Module '{}' not found. Installing types.", module),
                    command: Some(format!("npm install {} @types/{}", package, package)),
                    language: Language::TypeScript,
                }
            },
        },
        // ReferenceError: X is not defined
        ErrorPattern {
            regex: Regex::new(r"ReferenceError: (\w+) is not defined").expect("valid regex"),
            category: "js_reference_error",
            language: Language::JavaScript,
            handler: |captures, _full, _cwd| {
                let name = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "js_reference_error".into(),
                    explanation: format!("'{}' is not defined. Check for typos or missing imports.", name),
                    command: None,
                    language: Language::JavaScript,
                }
            },
        },
        // TypeError: X is not a function
        ErrorPattern {
            regex: Regex::new(r"TypeError: (\S+) is not a function").expect("valid regex"),
            category: "js_not_a_function",
            language: Language::JavaScript,
            handler: |captures, _full, _cwd| {
                let name = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "js_not_a_function".into(),
                    explanation: format!("'{}' is not a function. Check the import and export.", name),
                    command: None,
                    language: Language::JavaScript,
                }
            },
        },
        // Cannot read properties of null/undefined
        ErrorPattern {
            regex: Regex::new(r"Cannot read propert(?:y|ies) of (null|undefined)").expect("valid regex"),
            category: "js_null_access",
            language: Language::JavaScript,
            handler: |captures, _full, _cwd| {
                let value = captures.get(1).map_or("null", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "js_null_access".into(),
                    explanation: format!(
                        "Attempted to access property of {}. Add a null check or use optional chaining (?.).",
                        value
                    ),
                    command: None,
                    language: Language::JavaScript,
                }
            },
        },
        // EADDRINUSE
        ErrorPattern {
            regex: Regex::new(r"EADDRINUSE.*:(\d+)").expect("valid regex"),
            category: "js_port_in_use",
            language: Language::JavaScript,
            handler: |captures, _full, _cwd| {
                let port = captures.get(1).map_or("unknown", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "js_port_in_use".into(),
                    explanation: format!(
                        "Port {} is already in use. Kill the process or use a different port.",
                        port
                    ),
                    command: None,
                    language: Language::JavaScript,
                }
            },
        },
        // TS2345: Argument of type 'X' is not assignable
        ErrorPattern {
            regex: Regex::new(r"TS2345: Argument of type '([^']+)' is not assignable to parameter of type '([^']+)'")
                .expect("valid regex"),
            category: "ts_type_mismatch",
            language: Language::TypeScript,
            handler: |captures, _full, _cwd| {
                let got = captures.get(1).map_or("", |m| m.as_str());
                let expected = captures.get(2).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "ts_type_mismatch".into(),
                    explanation: format!("Type '{}' is not assignable to '{}'. Check the function signature.", got, expected),
                    command: None,
                    language: Language::TypeScript,
                }
            },
        },
        // SyntaxError: Unexpected token
        ErrorPattern {
            regex: Regex::new(r"SyntaxError: Unexpected token (.+)").expect("valid regex"),
            category: "js_syntax_error",
            language: Language::JavaScript,
            handler: |captures, _full, _cwd| {
                let token = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "js_syntax_error".into(),
                    explanation: format!("Unexpected token: {}. Check for missing brackets, commas, or semicolons.", token),
                    command: None,
                    language: Language::JavaScript,
                }
            },
        },
        // TypeError: Assignment to constant variable
        ErrorPattern {
            regex: Regex::new(r"TypeError: Assignment to constant variable").expect("valid regex"),
            category: "js_const_assignment",
            language: Language::JavaScript,
            handler: |_captures, _full, _cwd| {
                AutoFix {
                    fixable: false,
                    category: "js_const_assignment".into(),
                    explanation: "Cannot assign to a const variable. Use 'let' instead of 'const' if reassignment is needed.".into(),
                    command: None,
                    language: Language::JavaScript,
                }
            },
        },
        // ERR_MODULE_NOT_FOUND (ESM)
        ErrorPattern {
            regex: Regex::new(r"ERR_MODULE_NOT_FOUND.*?'([^']+)'").expect("valid regex"),
            category: "js_esm_not_found",
            language: Language::JavaScript,
            handler: |captures, _full, _cwd| {
                let module = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "js_esm_not_found".into(),
                    explanation: format!("ESM module '{}' not found. Check file extension (.mjs/.js) and package.json \"type\" field.", module),
                    command: None,
                    language: Language::JavaScript,
                }
            },
        },
        // TS2339: Property 'X' does not exist on type 'Y'
        ErrorPattern {
            regex: Regex::new(r"TS2339: Property '([^']+)' does not exist on type '([^']+)'").expect("valid regex"),
            category: "ts_property_not_exist",
            language: Language::TypeScript,
            handler: |captures, _full, _cwd| {
                let prop = captures.get(1).map_or("", |m| m.as_str());
                let type_name = captures.get(2).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "ts_property_not_exist".into(),
                    explanation: format!("Property '{}' does not exist on type '{}'. Extend the interface or check the property name.", prop, type_name),
                    command: None,
                    language: Language::TypeScript,
                }
            },
        },
        // ENOENT: no such file or directory
        ErrorPattern {
            regex: Regex::new(r"ENOENT:.*?'([^']+)'").expect("valid regex"),
            category: "js_enoent",
            language: Language::JavaScript,
            handler: |captures, _full, _cwd| {
                let path = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "js_enoent".into(),
                    explanation: format!("File or directory not found: '{}'. Check the path exists.", path),
                    command: None,
                    language: Language::JavaScript,
                }
            },
        },
        // webpack/vite Module not found
        ErrorPattern {
            regex: Regex::new(r"Module not found: (?:Error: )?Can't resolve '([^']+)'").expect("valid regex"),
            category: "js_bundler_resolve",
            language: Language::JavaScript,
            handler: |captures, _full, _cwd| {
                let module = captures.get(1).map_or("", |m| m.as_str());
                if module.starts_with('.') {
                    return AutoFix {
                        fixable: false,
                        category: "js_bundler_resolve".into(),
                        explanation: format!("Bundler can't resolve local module '{}'. Check the import path.", module),
                        command: None,
                        language: Language::JavaScript,
                    };
                }
                AutoFix {
                    fixable: true,
                    category: "js_bundler_resolve".into(),
                    explanation: format!("Bundler can't resolve '{}'. Installing.", module),
                    command: Some(format!("npm install {}", module)),
                    language: Language::JavaScript,
                }
            },
        },
        // Jest: Test suite failed to run
        ErrorPattern {
            regex: Regex::new(r"Test suite failed to run").expect("valid regex"),
            category: "js_jest_suite_failed",
            language: Language::JavaScript,
            handler: |_captures, full, _cwd| {
                let cause = full.lines()
                    .find(|l| l.contains("Cannot find") || l.contains("SyntaxError") || l.contains("Error:"))
                    .unwrap_or("Check test setup and imports");
                AutoFix {
                    fixable: false,
                    category: "js_jest_suite_failed".into(),
                    explanation: format!("Jest test suite failed to run. Cause: {}", cause),
                    command: None,
                    language: Language::JavaScript,
                }
            },
        },
        // ESLint parsing error
        ErrorPattern {
            regex: Regex::new(r"Parsing error: (.+)").expect("valid regex"),
            category: "js_eslint_parse",
            language: Language::JavaScript,
            handler: |captures, _full, _cwd| {
                let detail = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "js_eslint_parse".into(),
                    explanation: format!("ESLint parsing error: {}. Check syntax or parser configuration.", detail),
                    command: None,
                    language: Language::JavaScript,
                }
            },
        },
    ]
}

// ============================================================
// C#/.NET Patterns (20+)
// ============================================================

fn dotnet_patterns() -> Vec<ErrorPattern> {
    vec![
        // CS0246: The type or namespace name 'X' could not be found
        ErrorPattern {
            regex: Regex::new(r"CS0246:.*?'([^']+)'.*could not be found").expect("valid regex"),
            category: "dotnet_type_not_found",
            language: Language::CSharp,
            handler: |captures, _full, _cwd| {
                let type_name = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: true,
                    category: "dotnet_type_not_found".into(),
                    explanation: format!("Type '{}' not found. Installing NuGet package.", type_name),
                    command: Some(format!("dotnet add package {}", type_name)),
                    language: Language::CSharp,
                }
            },
        },
        // CS0103: The name 'X' does not exist in the current context
        ErrorPattern {
            regex: Regex::new(r"CS0103:.*?'([^']+)'.*does not exist").expect("valid regex"),
            category: "dotnet_name_not_found",
            language: Language::CSharp,
            handler: |captures, _full, _cwd| {
                let name = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "dotnet_name_not_found".into(),
                    explanation: format!("'{}' does not exist in the current context. Add a using directive or check spelling.", name),
                    command: None,
                    language: Language::CSharp,
                }
            },
        },
        // NU1101: Unable to find package 'X'
        ErrorPattern {
            regex: Regex::new(r"NU1101:.*?'([^']+)'").expect("valid regex"),
            category: "dotnet_package_not_found",
            language: Language::CSharp,
            handler: |captures, _full, _cwd| {
                let package = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "dotnet_package_not_found".into(),
                    explanation: format!("NuGet package '{}' not found. Check the package name on nuget.org.", package),
                    command: None,
                    language: Language::CSharp,
                }
            },
        },
        // NullReferenceException
        ErrorPattern {
            regex: Regex::new(r"System\.NullReferenceException").expect("valid regex"),
            category: "dotnet_null_ref",
            language: Language::CSharp,
            handler: |_captures, _full, _cwd| {
                AutoFix {
                    fixable: false,
                    category: "dotnet_null_ref".into(),
                    explanation: "NullReferenceException: Object reference not set. Use null-conditional (?.) or null checks.".into(),
                    command: None,
                    language: Language::CSharp,
                }
            },
        },
        // Build FAILED
        ErrorPattern {
            regex: Regex::new(r"Build FAILED").expect("valid regex"),
            category: "dotnet_build_failed",
            language: Language::CSharp,
            handler: |_captures, full, _cwd| {
                // Extract first error line
                let first_error = full
                    .lines()
                    .find(|l| l.contains("error CS") || l.contains("error NU") || l.contains("error MSB"))
                    .unwrap_or("See build output for details");
                AutoFix {
                    fixable: false,
                    category: "dotnet_build_failed".into(),
                    explanation: format!("Build failed. First error: {}", first_error),
                    command: None,
                    language: Language::CSharp,
                }
            },
        },
        // NETSDK1045: The current .NET SDK does not support targeting
        ErrorPattern {
            regex: Regex::new(r"NETSDK1045").expect("valid regex"),
            category: "dotnet_sdk_version",
            language: Language::CSharp,
            handler: |_captures, _full, _cwd| {
                AutoFix {
                    fixable: false,
                    category: "dotnet_sdk_version".into(),
                    explanation: "SDK version mismatch. Check global.json or install the required .NET SDK version.".into(),
                    command: None,
                    language: Language::CSharp,
                }
            },
        },
        // CS1002: ; expected
        ErrorPattern {
            regex: Regex::new(r"CS1002:").expect("valid regex"),
            category: "dotnet_semicolon",
            language: Language::CSharp,
            handler: |_captures, _full, _cwd| {
                AutoFix { fixable: false, category: "dotnet_semicolon".into(), explanation: "Missing semicolon. Add ';' at the end of the statement.".into(), command: None, language: Language::CSharp }
            },
        },
        // CS0019: Operator cannot be applied
        ErrorPattern {
            regex: Regex::new(r"CS0019:.*?Operator '([^']+)'.*?'([^']+)'.*?'([^']+)'").expect("valid regex"),
            category: "dotnet_operator",
            language: Language::CSharp,
            handler: |captures, _full, _cwd| {
                let op = captures.get(1).map_or("", |m| m.as_str());
                let t1 = captures.get(2).map_or("", |m| m.as_str());
                let t2 = captures.get(3).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "dotnet_operator".into(), explanation: format!("Operator '{}' cannot be applied to '{}' and '{}'. Cast or convert types.", op, t1, t2), command: None, language: Language::CSharp }
            },
        },
        // CS0029: Cannot implicitly convert
        ErrorPattern {
            regex: Regex::new(r"CS0029:.*?'([^']+)'.*?'([^']+)'").expect("valid regex"),
            category: "dotnet_implicit_convert",
            language: Language::CSharp,
            handler: |captures, _full, _cwd| {
                let from = captures.get(1).map_or("", |m| m.as_str());
                let to = captures.get(2).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "dotnet_implicit_convert".into(), explanation: format!("Cannot implicitly convert '{}' to '{}'. Use explicit cast: ({})", from, to, to), command: None, language: Language::CSharp }
            },
        },
        // CS0117: does not contain a definition for
        ErrorPattern {
            regex: Regex::new(r"CS0117:.*?'([^']+)'.*?'([^']+)'").expect("valid regex"),
            category: "dotnet_no_definition",
            language: Language::CSharp,
            handler: |captures, _full, _cwd| {
                let type_name = captures.get(1).map_or("", |m| m.as_str());
                let member = captures.get(2).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "dotnet_no_definition".into(), explanation: format!("'{}' does not contain a definition for '{}'. Check spelling or add the method.", type_name, member), command: None, language: Language::CSharp }
            },
        },
        // InvalidOperationException
        ErrorPattern {
            regex: Regex::new(r"System\.InvalidOperationException: (.+)").expect("valid regex"),
            category: "dotnet_invalid_op",
            language: Language::CSharp,
            handler: |captures, _full, _cwd| {
                let detail = captures.get(1).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "dotnet_invalid_op".into(), explanation: format!("InvalidOperationException: {}. Check object state before calling this method.", detail), command: None, language: Language::CSharp }
            },
        },
        // ArgumentException / ArgumentNullException
        ErrorPattern {
            regex: Regex::new(r"System\.Argument(?:Null)?Exception: (.+)").expect("valid regex"),
            category: "dotnet_argument",
            language: Language::CSharp,
            handler: |captures, _full, _cwd| {
                let detail = captures.get(1).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "dotnet_argument".into(), explanation: format!("ArgumentException: {}. Validate input parameters.", detail), command: None, language: Language::CSharp }
            },
        },
        // FileNotFoundException
        ErrorPattern {
            regex: Regex::new(r"System\.IO\.FileNotFoundException:.*?'([^']+)'").expect("valid regex"),
            category: "dotnet_file_not_found",
            language: Language::CSharp,
            handler: |captures, _full, _cwd| {
                let path = captures.get(1).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "dotnet_file_not_found".into(), explanation: format!("File not found: '{}'. Check the path and working directory.", path), command: None, language: Language::CSharp }
            },
        },
        // FormatException
        ErrorPattern {
            regex: Regex::new(r"System\.FormatException: (.+)").expect("valid regex"),
            category: "dotnet_format",
            language: Language::CSharp,
            handler: |captures, _full, _cwd| {
                let detail = captures.get(1).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "dotnet_format".into(), explanation: format!("FormatException: {}. Check input string format for parsing.", detail), command: None, language: Language::CSharp }
            },
        },
        // IndexOutOfRangeException
        ErrorPattern {
            regex: Regex::new(r"System\.IndexOutOfRangeException").expect("valid regex"),
            category: "dotnet_index_out_of_range",
            language: Language::CSharp,
            handler: |_captures, _full, _cwd| {
                AutoFix { fixable: false, category: "dotnet_index_out_of_range".into(), explanation: "Index was outside the bounds of the array. Check collection.Length before accessing.".into(), command: None, language: Language::CSharp }
            },
        },
        // InvalidCastException
        ErrorPattern {
            regex: Regex::new(r"System\.InvalidCastException").expect("valid regex"),
            category: "dotnet_invalid_cast",
            language: Language::CSharp,
            handler: |_captures, _full, _cwd| {
                AutoFix { fixable: false, category: "dotnet_invalid_cast".into(), explanation: "Invalid cast. Use 'as' operator or 'is' pattern for safe casting.".into(), command: None, language: Language::CSharp }
            },
        },
        // StackOverflowException
        ErrorPattern {
            regex: Regex::new(r"System\.StackOverflowException").expect("valid regex"),
            category: "dotnet_stack_overflow",
            language: Language::CSharp,
            handler: |_captures, _full, _cwd| {
                AutoFix { fixable: false, category: "dotnet_stack_overflow".into(), explanation: "Stack overflow. Check for infinite recursion or deeply nested calls.".into(), command: None, language: Language::CSharp }
            },
        },
        // HttpRequestException
        ErrorPattern {
            regex: Regex::new(r"System\.Net\.Http\.HttpRequestException: (.+)").expect("valid regex"),
            category: "dotnet_http_error",
            language: Language::CSharp,
            handler: |captures, _full, _cwd| {
                let detail = captures.get(1).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "dotnet_http_error".into(), explanation: format!("HTTP request failed: {}. Check URL and network connectivity.", detail), command: None, language: Language::CSharp }
            },
        },
        // EF Core migration error
        ErrorPattern {
            regex: Regex::new(r"(?:pending model changes|pending migrations|The model.*has changed)").expect("valid regex"),
            category: "dotnet_ef_migration",
            language: Language::CSharp,
            handler: |_captures, _full, _cwd| {
                AutoFix { fixable: true, category: "dotnet_ef_migration".into(), explanation: "Pending EF Core migrations detected.".into(), command: Some("dotnet ef database update".into()), language: Language::CSharp }
            },
        },
        // Razor compilation error
        ErrorPattern {
            regex: Regex::new(r"(?:RZ\d{4}|Razor):.*?error").expect("valid regex"),
            category: "dotnet_razor",
            language: Language::CSharp,
            handler: |_captures, full, _cwd| {
                let first = full.lines().find(|l| l.contains("RZ") || l.contains("Razor")).unwrap_or("See build output");
                AutoFix { fixable: false, category: "dotnet_razor".into(), explanation: format!("Razor compilation error: {}", first), command: None, language: Language::CSharp }
            },
        },
    ]
}

// ============================================================
// Rust Patterns (10+)
// ============================================================

fn rust_patterns() -> Vec<ErrorPattern> {
    vec![
        // E0433: failed to resolve: use of undeclared crate or module
        ErrorPattern {
            regex: Regex::new(r"E0433.*?use of undeclared (?:crate|module) `([^`]+)`")
                .expect("valid regex"),
            category: "rust_unresolved_import",
            language: Language::Rust,
            handler: |captures, _full, _cwd| {
                let crate_name = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: true,
                    category: "rust_unresolved_import".into(),
                    explanation: format!("Crate '{}' not in dependencies", crate_name),
                    command: Some(format!("cargo add {}", crate_name)),
                    language: Language::Rust,
                }
            },
        },
        // E0308: mismatched types
        ErrorPattern {
            regex: Regex::new(r"E0308.*?expected `([^`]+)`, found `([^`]+)`")
                .expect("valid regex"),
            category: "rust_type_mismatch",
            language: Language::Rust,
            handler: |captures, _full, _cwd| {
                let expected = captures.get(1).map_or("", |m| m.as_str());
                let found = captures.get(2).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "rust_type_mismatch".into(),
                    explanation: format!("Type mismatch: expected `{}`, found `{}`", expected, found),
                    command: None,
                    language: Language::Rust,
                }
            },
        },
        // E0382: borrow of moved value
        ErrorPattern {
            regex: Regex::new(r"E0382.*?borrow of moved value: `([^`]+)`")
                .expect("valid regex"),
            category: "rust_moved_value",
            language: Language::Rust,
            handler: |captures, _full, _cwd| {
                let name = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "rust_moved_value".into(),
                    explanation: format!(
                        "Value '{}' was moved. Use .clone() to copy, or restructure to avoid the move.",
                        name
                    ),
                    command: None,
                    language: Language::Rust,
                }
            },
        },
        // E0599: no method named 'X' found
        ErrorPattern {
            regex: Regex::new(r"E0599.*?no method named `([^`]+)` found")
                .expect("valid regex"),
            category: "rust_method_not_found",
            language: Language::Rust,
            handler: |captures, _full, _cwd| {
                let method = captures.get(1).map_or("", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "rust_method_not_found".into(),
                    explanation: format!(
                        "Method '{}' not found. Check if you need to import a trait (use X::Trait).",
                        method
                    ),
                    command: None,
                    language: Language::Rust,
                }
            },
        },
        // thread 'X' panicked at 'Y'
        ErrorPattern {
            regex: Regex::new(r"thread '([^']+)' panicked at '([^']+)'(?:.*?(\S+\.rs:\d+))?")
                .expect("valid regex"),
            category: "rust_panic",
            language: Language::Rust,
            handler: |captures, _full, _cwd| {
                let thread = captures.get(1).map_or("main", |m| m.as_str());
                let message = captures.get(2).map_or("", |m| m.as_str());
                let location = captures.get(3).map_or("unknown", |m| m.as_str());
                AutoFix {
                    fixable: false,
                    category: "rust_panic".into(),
                    explanation: format!(
                        "Thread '{}' panicked: '{}' at {}",
                        thread, message, location
                    ),
                    command: None,
                    language: Language::Rust,
                }
            },
        },
        // E0502: cannot borrow as mutable because it is also borrowed as immutable
        ErrorPattern {
            regex: Regex::new(r"E0502.*?cannot borrow `([^`]+)`").expect("valid regex"),
            category: "rust_borrow_conflict",
            language: Language::Rust,
            handler: |captures, _full, _cwd| {
                let name = captures.get(1).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "rust_borrow_conflict".into(), explanation: format!("Cannot borrow '{}' as mutable while also borrowed as immutable. Restructure to avoid overlapping borrows.", name), command: None, language: Language::Rust }
            },
        },
        // E0425: cannot find value/function
        ErrorPattern {
            regex: Regex::new(r"E0425.*?cannot find (?:value|function) `([^`]+)`").expect("valid regex"),
            category: "rust_not_found",
            language: Language::Rust,
            handler: |captures, _full, _cwd| {
                let name = captures.get(1).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "rust_not_found".into(), explanation: format!("Cannot find '{}'. Check for typos or add a `use` import.", name), command: None, language: Language::Rust }
            },
        },
        // E0277: the trait bound is not satisfied
        ErrorPattern {
            regex: Regex::new(r"E0277.*?the trait bound `([^`]+)` is not satisfied").expect("valid regex"),
            category: "rust_trait_bound",
            language: Language::Rust,
            handler: |captures, _full, _cwd| {
                let bound = captures.get(1).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "rust_trait_bound".into(), explanation: format!("Trait bound not satisfied: `{}`. Implement the trait or add it to where clause.", bound), command: None, language: Language::Rust }
            },
        },
        // E0061: this function takes N arguments but M were supplied
        ErrorPattern {
            regex: Regex::new(r"E0061.*?this function takes (\d+) arguments? but (\d+) (?:was|were) supplied").expect("valid regex"),
            category: "rust_wrong_args",
            language: Language::Rust,
            handler: |captures, _full, _cwd| {
                let expected = captures.get(1).map_or("", |m| m.as_str());
                let got = captures.get(2).map_or("", |m| m.as_str());
                AutoFix { fixable: false, category: "rust_wrong_args".into(), explanation: format!("Function takes {} argument(s) but {} were supplied. Check the function signature.", expected, got), command: None, language: Language::Rust }
            },
        },
        // E0015: cannot call non-const fn in const context
        ErrorPattern {
            regex: Regex::new(r"E0015.*?cannot call non-const fn").expect("valid regex"),
            category: "rust_const_fn",
            language: Language::Rust,
            handler: |_captures, _full, _cwd| {
                AutoFix { fixable: false, category: "rust_const_fn".into(), explanation: "Cannot call non-const function in const context. Use a const fn or move to runtime initialization.".into(), command: None, language: Language::Rust }
            },
        },
    ]
}
