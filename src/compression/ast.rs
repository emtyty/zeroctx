use anyhow::Result;
use tree_sitter::{Node, Parser};

use crate::core::types::Language;

/// AST-based code compression using tree-sitter.
pub struct AstCompressor;

impl AstCompressor {
    pub fn compress(
        source: &str,
        language: Language,
        relevant_names: &[String],
    ) -> Result<String> {
        if relevant_names.is_empty() {
            return Self::signatures_only(source, language);
        }
        match Self::ts_compress(source, language, relevant_names) {
            Ok(result) if !result.is_empty() => Ok(result),
            _ => Self::regex_compress(source, language, relevant_names),
        }
    }

    pub fn signatures_only(source: &str, language: Language) -> Result<String> {
        match Self::ts_signatures(source, language) {
            Ok(result) if !result.is_empty() => Ok(result),
            _ => Self::regex_signatures(source, language),
        }
    }

    fn make_parser(language: Language) -> Option<Parser> {
        let ts_lang = match language {
            Language::Python => tree_sitter_python::LANGUAGE,
            Language::JavaScript => tree_sitter_javascript::LANGUAGE,
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            Language::CSharp => tree_sitter_c_sharp::LANGUAGE,
            _ => return None,
        };
        let mut parser = Parser::new();
        parser.set_language(&ts_lang.into()).ok()?;
        Some(parser)
    }

    fn ts_signatures(source: &str, language: Language) -> Result<String> {
        let mut parser = match Self::make_parser(language) {
            Some(p) => p,
            None => return Ok(String::new()),
        };
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("tree-sitter parse failed"))?;
        let root = tree.root_node();
        let mut sigs = Vec::new();
        Self::collect_signatures(root, source, language, 0, &mut sigs);
        Ok(sigs.join("\n"))
    }

    fn collect_signatures(
        node: Node,
        source: &str,
        language: Language,
        depth: usize,
        out: &mut Vec<String>,
    ) {
        if Self::is_definition_node(node.kind(), language) {
            let sig = Self::extract_signature(node, source, language);
            if !sig.is_empty() {
                let indent = "  ".repeat(depth);
                out.push(format!("{}{}", indent, sig));
            }
            if Self::is_container_node(node.kind(), language) {
                let body = Self::find_body_node(node, language);
                if let Some(body_node) = body {
                    let mut cursor = body_node.walk();
                    for child in body_node.children(&mut cursor) {
                        Self::collect_signatures(child, source, language, depth + 1, out);
                    }
                }
                return;
            }
        }
        // JS/TS: handle test framework call patterns (describe, it, test, etc.)
        // These are expression_statement nodes with call_expression children,
        // not captured by is_definition_node but critical for test file structure.
        if matches!(language, Language::JavaScript | Language::TypeScript)
            && node.kind() == "expression_statement"
        {
            if let Some((sig, body_node)) = Self::js_test_call_signature(node, source) {
                let indent = "  ".repeat(depth);
                out.push(format!("{}{}", indent, sig));
                if let Some(block) = body_node {
                    let mut cursor = block.walk();
                    for child in block.children(&mut cursor) {
                        Self::collect_signatures(child, source, language, depth + 1, out);
                    }
                }
                return;
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::collect_signatures(child, source, language, depth, out);
        }
    }

    fn is_definition_node(kind: &str, language: Language) -> bool {
        match language {
            Language::Python => matches!(
                kind,
                "function_definition"
                    | "class_definition"
                    | "decorated_definition"
                    | "import_statement"
                    | "import_from_statement"
            ),
            Language::JavaScript | Language::TypeScript => matches!(
                kind,
                "function_declaration"
                    | "class_declaration"
                    | "method_definition"
                    | "export_statement"
                    | "import_statement"
                    | "lexical_declaration"
                    | "interface_declaration"
                    | "type_alias_declaration"
            ),
            Language::CSharp => matches!(
                kind,
                "method_declaration"
                    | "class_declaration"
                    | "interface_declaration"
                    | "namespace_declaration"
                    | "using_directive"
                    | "property_declaration"
                    | "constructor_declaration"
                    | "enum_declaration"
                    | "struct_declaration"
            ),
            _ => false,
        }
    }

    fn is_container_node(kind: &str, language: Language) -> bool {
        match language {
            Language::Python => matches!(kind, "class_definition" | "decorated_definition"),
            Language::JavaScript | Language::TypeScript => {
                matches!(kind, "class_declaration" | "export_statement")
            }
            Language::CSharp => matches!(
                kind,
                "class_declaration"
                    | "interface_declaration"
                    | "namespace_declaration"
                    | "struct_declaration"
            ),
            _ => false,
        }
    }

    fn find_body_node<'a>(node: Node<'a>, language: Language) -> Option<Node<'a>> {
        let body_kind = match language {
            Language::Python => "block",
            Language::JavaScript | Language::TypeScript => "class_body",
            Language::CSharp => "declaration_list",
            _ => return None,
        };
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == body_kind {
                return Some(child);
            }
            if language == Language::Python && child.kind() == "class_definition" {
                return Self::find_body_node(child, language);
            }
        }
        None
    }

    fn extract_signature(node: Node, source: &str, language: Language) -> String {
        match language {
            Language::Python => Self::python_signature(node, source),
            Language::JavaScript | Language::TypeScript => Self::js_signature(node, source),
            Language::CSharp => Self::csharp_signature(node, source),
            _ => String::new(),
        }
    }

    fn python_signature(node: Node, source: &str) -> String {
        match node.kind() {
            "function_definition" => {
                let name = Self::child_text(node, "name", source).unwrap_or_default();
                let params = Self::child_text(node, "parameters", source).unwrap_or("()".into());
                let ret = Self::child_by_field(node, "return_type")
                    .map(|n| format!(" -> {}", n.utf8_text(source.as_bytes()).unwrap_or("")));
                format!("def {}{}{}:", name, params, ret.unwrap_or_default())
            }
            "class_definition" => {
                let name = Self::child_text(node, "name", source).unwrap_or_default();
                let superclasses = Self::child_by_field(node, "superclasses")
                    .map(|n| n.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                format!("class {}{}:", name, superclasses.unwrap_or_default())
            }
            "decorated_definition" => {
                let mut parts = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "decorator" {
                        parts.push(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                    } else if child.kind() == "function_definition"
                        || child.kind() == "class_definition"
                    {
                        parts.push(Self::python_signature(child, source));
                    }
                }
                parts.join("\n")
            }
            "import_statement" | "import_from_statement" => Self::node_first_line(node, source),
            _ => String::new(),
        }
    }

    fn js_signature(node: Node, source: &str) -> String {
        match node.kind() {
            "function_declaration" => {
                let name = Self::child_text(node, "name", source).unwrap_or_default();
                let params = Self::child_text(node, "parameters", source).unwrap_or("()".into());
                format!("function {}{}  {{ ... }}", name, params)
            }
            "class_declaration" => {
                let name = Self::child_text(node, "name", source).unwrap_or_default();
                format!("class {} {{ ... }}", name)
            }
            "method_definition" => {
                let name = Self::child_text(node, "name", source).unwrap_or_default();
                let params = Self::child_text(node, "parameters", source).unwrap_or("()".into());
                format!("{}{}  {{ ... }}", name, params)
            }
            "interface_declaration" | "type_alias_declaration" => {
                Self::node_first_line(node, source)
            }
            "import_statement" | "export_statement" | "lexical_declaration" => {
                Self::node_first_line(node, source)
            }
            _ => String::new(),
        }
    }

    /// Extract signature from JS/TS test framework call expressions.
    /// Handles: describe('name', () => {}), it('name', () => {}), test('name', fn), etc.
    /// Returns (signature_string, optional_body_node_for_recursion).
    fn js_test_call_signature<'a>(
        node: Node<'a>,
        source: &str,
    ) -> Option<(String, Option<Node<'a>>)> {
        let call = node.child(0)?;
        if call.kind() != "call_expression" {
            return None;
        }

        let func = call.child(0)?;
        let name = func.utf8_text(source.as_bytes()).ok()?;

        // Known test framework call names
        let is_container_call = matches!(name, "describe" | "context" | "suite" | "fdescribe" | "xdescribe");
        let is_test_call = matches!(
            name,
            "it" | "test" | "specify" | "fit" | "xit" | "xtest"
                | "beforeEach" | "afterEach" | "beforeAll" | "afterAll"
                | "before" | "after"
        );

        if !is_container_call && !is_test_call {
            return None;
        }

        let args = call.child_by_field_name("arguments")?;

        // Walk arguments to find test name (string) and callback body
        let mut test_name = None;
        let mut callback_body = None;
        let mut cursor = args.walk();
        for child in args.children(&mut cursor) {
            match child.kind() {
                "string" | "template_string" => {
                    if test_name.is_none() {
                        test_name =
                            child.utf8_text(source.as_bytes()).ok().map(|s| s.to_string());
                    }
                }
                "arrow_function" | "function_expression" => {
                    let mut inner_cursor = child.walk();
                    for inner_child in child.children(&mut inner_cursor) {
                        if inner_child.kind() == "statement_block" {
                            callback_body = Some(inner_child);
                            break;
                        }
                    }
                }
                _ => {}
            }
        }

        let sig = match test_name {
            Some(ref tname) => format!("{}({}, () => {{ ... }})", name, tname),
            None => format!("{}(() => {{ ... }})", name),
        };

        // Only recurse into container calls (describe, context, suite)
        let body_for_recursion = if is_container_call {
            callback_body
        } else {
            None
        };

        Some((sig, body_for_recursion))
    }

    fn csharp_signature(node: Node, source: &str) -> String {
        match node.kind() {
            "method_declaration" | "constructor_declaration" => {
                let start = node.start_byte();
                let mut end = node.end_byte();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "block" {
                        end = child.start_byte();
                        break;
                    }
                }
                let sig = &source[start..end];
                format!("{} {{ ... }}", sig.trim())
            }
            "class_declaration" | "struct_declaration" | "interface_declaration" => {
                let name = Self::child_text(node, "name", source).unwrap_or_default();
                format!(
                    "{} {} {{ ... }}",
                    node.kind().replace("_declaration", ""),
                    name
                )
            }
            "namespace_declaration" => {
                let name = Self::child_text(node, "name", source).unwrap_or_default();
                format!("namespace {} {{ ... }}", name)
            }
            "enum_declaration" => {
                let name = Self::child_text(node, "name", source).unwrap_or_default();
                format!("enum {} {{ ... }}", name)
            }
            "using_directive" | "property_declaration" => Self::node_first_line(node, source),
            _ => String::new(),
        }
    }

    fn ts_compress(source: &str, language: Language, relevant_names: &[String]) -> Result<String> {
        let mut parser = match Self::make_parser(language) {
            Some(p) => p,
            None => return Ok(String::new()),
        };
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("tree-sitter parse failed"))?;
        let root = tree.root_node();
        let mut blocks = Vec::new();
        Self::collect_relevant(root, source, language, relevant_names, &mut blocks);
        if blocks.is_empty() {
            return Self::ts_signatures(source, language);
        }
        Ok(blocks.join("\n\n"))
    }

    fn collect_relevant(
        node: Node,
        source: &str,
        language: Language,
        names: &[String],
        out: &mut Vec<String>,
    ) {
        if Self::is_definition_node(node.kind(), language) {
            let def_name = Self::definition_name(node, source, language);
            if names.iter().any(|n| def_name.contains(n)) {
                let text = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                out.push(text);
                return;
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::collect_relevant(child, source, language, names, out);
        }
    }

    fn definition_name(node: Node, source: &str, language: Language) -> String {
        match language {
            Language::Python => {
                if node.kind() == "decorated_definition" {
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind() == "function_definition"
                            || child.kind() == "class_definition"
                        {
                            return Self::child_text(child, "name", source).unwrap_or_default();
                        }
                    }
                    String::new()
                } else {
                    Self::child_text(node, "name", source).unwrap_or_default()
                }
            }
            Language::JavaScript | Language::TypeScript => {
                Self::child_text(node, "name", source).unwrap_or_else(|| {
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if let Some(name) = Self::child_text(child, "name", source) {
                            return name;
                        }
                    }
                    String::new()
                })
            }
            Language::CSharp => Self::child_text(node, "name", source).unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn child_text(node: Node, field: &str, source: &str) -> Option<String> {
        node.child_by_field_name(field)
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(|s| s.to_string())
    }

    fn child_by_field<'a>(node: Node<'a>, field: &str) -> Option<Node<'a>> {
        node.child_by_field_name(field)
    }

    fn node_first_line(node: Node, source: &str) -> String {
        node.utf8_text(source.as_bytes())
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("")
            .to_string()
    }

    fn regex_signatures(source: &str, language: Language) -> Result<String> {
        let lines: Vec<&str> = source
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                match language {
                    Language::Python => {
                        trimmed.starts_with("def ")
                            || trimmed.starts_with("class ")
                            || trimmed.starts_with("import ")
                            || trimmed.starts_with("from ")
                    }
                    Language::JavaScript | Language::TypeScript => {
                        trimmed.starts_with("function ")
                            || trimmed.starts_with("export ")
                            || trimmed.starts_with("import ")
                            || trimmed.starts_with("class ")
                            || trimmed.contains("=> {")
                            || trimmed.starts_with("const ")
                            || trimmed.starts_with("interface ")
                            || trimmed.starts_with("type ")
                    }
                    Language::CSharp => {
                        trimmed.starts_with("public ")
                            || trimmed.starts_with("private ")
                            || trimmed.starts_with("protected ")
                            || trimmed.starts_with("internal ")
                            || trimmed.starts_with("static ")
                            || trimmed.starts_with("namespace ")
                            || trimmed.starts_with("using ")
                            || trimmed.starts_with("class ")
                            || trimmed.starts_with("interface ")
                    }
                    Language::Rust => {
                        trimmed.starts_with("pub ")
                            || trimmed.starts_with("fn ")
                            || trimmed.starts_with("struct ")
                            || trimmed.starts_with("enum ")
                            || trimmed.starts_with("trait ")
                            || trimmed.starts_with("impl ")
                            || trimmed.starts_with("use ")
                            || trimmed.starts_with("mod ")
                    }
                    _ => false,
                }
            })
            .collect();
        Ok(lines.join("\n"))
    }

    fn regex_compress(
        source: &str,
        _language: Language,
        relevant_names: &[String],
    ) -> Result<String> {
        let mut result = Vec::new();
        let mut in_relevant_block = false;
        let mut brace_depth: i32 = 0;
        let mut indent_level: Option<usize> = None;

        for line in source.lines() {
            let trimmed = line.trim();
            let starts_relevant = relevant_names.iter().any(|name| trimmed.contains(name));
            if starts_relevant {
                in_relevant_block = true;
                indent_level = Some(line.len() - line.trim_start().len());
                brace_depth = 0;
            }
            if in_relevant_block {
                result.push(line);
                for ch in trimmed.chars() {
                    match ch {
                        '{' | '(' => brace_depth += 1,
                        '}' | ')' => brace_depth -= 1,
                        _ => {}
                    }
                }
                if let Some(base_indent) = indent_level {
                    let current_indent = line.len() - line.trim_start().len();
                    if !trimmed.is_empty()
                        && current_indent <= base_indent
                        && result.len() > 1
                        && brace_depth <= 0
                    {
                        in_relevant_block = false;
                        indent_level = None;
                    }
                }
            }
        }

        if result.is_empty() {
            Self::regex_signatures(source, _language)
        } else {
            Ok(result.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_signatures() {
        let source = "import os\nfrom pathlib import Path\n\nclass MyClass:\n    def __init__(self, value):\n        self.value = value\n\n    def process(self, data):\n        result = []\n        for item in data:\n            result.append(item * self.value)\n        return result\n\ndef standalone_function(x, y):\n    return x + y\n\ndef another_function():\n    pass\n";
        let result = AstCompressor::signatures_only(source, Language::Python).unwrap();
        assert!(result.contains("class MyClass"), "should contain class");
        assert!(result.contains("def __init__"), "should contain __init__");
        assert!(result.contains("def process"), "should contain process");
        assert!(result.contains("def standalone_function"), "should contain standalone_function");
        assert!(!result.contains("result = []"), "should not contain function body");
    }

    #[test]
    fn test_python_compress_relevant() {
        let source = "def foo():\n    return 1\n\ndef bar():\n    x = 2\n    return x\n\ndef baz():\n    return 3\n";
        let result = AstCompressor::compress(source, Language::Python, &["bar".to_string()]).unwrap();
        assert!(result.contains("def bar"), "should contain bar");
        assert!(result.contains("return x"), "should contain bar body");
    }

    #[test]
    fn test_js_signatures() {
        let source = "function greet(name) {\n    return name;\n}\n\nclass Calculator {\n    constructor(initial) {\n        this.value = initial;\n    }\n\n    add(n) {\n        this.value += n;\n        return this;\n    }\n}\n";
        let result = AstCompressor::signatures_only(source, Language::JavaScript).unwrap();
        assert!(result.contains("greet"), "should contain greet");
        assert!(result.contains("Calculator"), "should contain Calculator");
        assert!(!result.contains("this.value += n"), "should not contain method body");
    }

    #[test]
    fn test_csharp_signatures() {
        let source = "using System;\n\nnamespace MyApp\n{\n    public class UserService\n    {\n        public User GetUser(int id)\n        {\n            return new User();\n        }\n\n        private void ValidateId(int id)\n        {\n            if (id <= 0) throw new ArgumentException();\n        }\n    }\n}\n";
        let result = AstCompressor::signatures_only(source, Language::CSharp).unwrap();
        assert!(result.contains("UserService"), "should contain class name");
        assert!(result.contains("GetUser"), "should contain method");
        assert!(!result.contains("return new User"), "should not contain method body");
    }

    #[test]
    fn test_rust_regex_fallback() {
        let source = "use std::io;\n\npub struct Config {\n    pub name: String,\n}\n\nimpl Config {\n    pub fn new(name: &str) -> Self {\n        Self { name: name.to_string() }\n    }\n}\n\nfn helper() -> bool {\n    true\n}\n";
        let result = AstCompressor::signatures_only(source, Language::Rust).unwrap();
        assert!(result.contains("use std::io"), "should contain use");
        assert!(result.contains("pub struct Config"), "should contain struct");
        assert!(result.contains("impl Config"), "should contain impl");
        assert!(result.contains("pub fn new"), "should contain fn");
        assert!(result.contains("fn helper"), "should contain fn");
    }

    #[test]
    fn test_empty_relevant_names_returns_signatures() {
        let source = "def foo():\n    pass\n\ndef bar():\n    pass\n";
        let result = AstCompressor::compress(source, Language::Python, &[]).unwrap();
        assert!(result.contains("def foo"), "should return signatures");
        assert!(result.contains("def bar"), "should return signatures");
    }

    #[test]
    fn test_unknown_language_empty() {
        let source = "some random text\nmore text\n";
        let result = AstCompressor::signatures_only(source, Language::Unknown).unwrap();
        assert!(result.is_empty(), "unknown language should return empty");
    }
}
