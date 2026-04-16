use zeroctx::compression::ast::AstCompressor;
use zeroctx::compression::compress_file;
use zeroctx::config::Config;
use zeroctx::core::types::Language;

fn fixtures_dir() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!("{}/tests/fixtures", manifest)
}

#[test]
fn test_compress_python_file() {
    let path = format!("{}/sample.py", fixtures_dir());
    let config = Config::default();
    let result = compress_file(&path, &config).unwrap();

    // Should be compressed (> 80 lines)
    assert!(result.contains("ZeroCTX compressed"), "should have compression header");
    assert!(result.contains("class DataProcessor"), "should contain class signature");
    assert!(result.contains("def process"), "should contain method signature");
    assert!(result.contains("class FileHandler"), "should contain FileHandler");
    // Should NOT contain function bodies
    assert!(!result.contains("self.cache = {}"), "should not contain init body");
}

#[test]
fn test_compress_js_file() {
    let path = format!("{}/sample.js", fixtures_dir());
    let config = Config::default();
    let result = compress_file(&path, &config).unwrap();

    assert!(result.contains("ZeroCTX compressed"), "should have compression header");
    assert!(result.contains("parseConfig"), "should contain parseConfig");
    assert!(result.contains("ApiClient"), "should contain ApiClient");
}

#[test]
fn test_compress_csharp_file() {
    let path = format!("{}/sample.cs", fixtures_dir());
    let config = Config::default();
    let result = compress_file(&path, &config).unwrap();

    assert!(result.contains("ZeroCTX compressed"), "should have compression header");
    assert!(result.contains("UserService"), "should contain UserService");
    assert!(result.contains("GetUserAsync"), "should contain method name");
}

#[test]
fn test_compress_rust_file() {
    let path = format!("{}/sample.rs", fixtures_dir());
    let config = Config::default();
    let result = compress_file(&path, &config).unwrap();

    assert!(result.contains("ZeroCTX compressed"), "should have compression header");
    assert!(result.contains("pub struct Cache"), "should contain struct");
    assert!(result.contains("pub fn new"), "should contain fn");
}

#[test]
fn test_compress_small_file_passthrough() {
    // Create a temp file with < 80 lines
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("small.py");
    std::fs::write(&path, "def hello():\n    return 42\n").unwrap();

    let config = Config::default();
    let result = compress_file(path.to_str().unwrap(), &config).unwrap();

    assert!(!result.contains("ZeroCTX compressed"), "small files should pass through");
    assert!(result.contains("def hello()"), "should contain original content");
}

#[test]
fn test_ast_python_relevant_extraction() {
    let source = std::fs::read_to_string(format!("{}/sample.py", fixtures_dir())).unwrap();
    let result = AstCompressor::compress(&source, Language::Python, &["process".to_string()]).unwrap();

    assert!(result.contains("def process"), "should extract process method");
    assert!(result.contains("results"), "should contain process body");
}

#[test]
fn test_ast_js_relevant_extraction() {
    let source = std::fs::read_to_string(format!("{}/sample.js", fixtures_dir())).unwrap();
    let result = AstCompressor::compress(&source, Language::JavaScript, &["debounce".to_string()]).unwrap();

    assert!(result.contains("debounce"), "should extract debounce function");
    assert!(result.contains("setTimeout"), "should contain debounce body");
}

#[test]
fn test_ast_csharp_relevant_extraction() {
    let source = std::fs::read_to_string(format!("{}/sample.cs", fixtures_dir())).unwrap();
    let result = AstCompressor::compress(&source, Language::CSharp, &["GetUserAsync".to_string()]).unwrap();

    assert!(result.contains("GetUserAsync"), "should extract GetUserAsync");
}

#[test]
fn test_compression_savings_ratio() {
    let source = std::fs::read_to_string(format!("{}/sample.py", fixtures_dir())).unwrap();
    let compressed = AstCompressor::signatures_only(&source, Language::Python).unwrap();

    let orig_len = source.len();
    let comp_len = compressed.len();
    let savings = ((orig_len - comp_len) as f64 / orig_len as f64) * 100.0;

    assert!(savings > 40.0, "should save at least 40%% (got {:.1}%%)", savings);
    assert!(savings < 99.0, "should not lose all content (got {:.1}%%)", savings);
}
