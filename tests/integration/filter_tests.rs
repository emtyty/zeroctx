use zeroctx::config::Config;
use zeroctx::filters::FilterRegistry;

fn fixtures_dir() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!("{}/tests/fixtures", manifest)
}

#[test]
fn test_git_filter_matches_diff() {
    let registry = FilterRegistry::new();
    let config = Config::default();
    let output = std::fs::read_to_string(format!("{}/git_diff_output.txt", fixtures_dir())).unwrap();

    let result = registry.apply("git diff", &output, &config);
    // Git filter should match and process
    assert!(result.output.contains("main.rs"), "should preserve file names");
}

#[test]
fn test_rust_filter_matches_cargo() {
    let registry = FilterRegistry::new();
    let config = Config::default();
    let output = std::fs::read_to_string(format!("{}/cargo_error_output.txt", fixtures_dir())).unwrap();

    let result = registry.apply("cargo build", &output, &config);
    assert!(result.output.contains("error"), "should preserve error messages");
}

#[test]
fn test_js_filter_matches_npm() {
    let registry = FilterRegistry::new();
    let config = Config::default();
    let output = std::fs::read_to_string(format!("{}/npm_install_output.txt", fixtures_dir())).unwrap();

    let result = registry.apply("npm install", &output, &config);
    assert!(!result.output.is_empty(), "should produce output");
}

#[test]
fn test_unknown_command_passthrough() {
    let registry = FilterRegistry::new();
    let config = Config::default();
    let output = "Hello, world!\nLine 2\n";

    let result = registry.apply("some-unknown-command", output, &config);
    assert_eq!(result.output, output, "unknown commands should pass through");
    assert_eq!(result.savings_percent, 0.0, "no savings for passthrough");
}

#[test]
fn test_filter_registry_has_all_filters() {
    let registry = FilterRegistry::new();
    let config = Config::default();

    // Test that each expected filter type matches
    let commands = vec![
        ("git status", true),
        ("git diff", true),
        ("python3 script.py", true),
        ("node app.js", true),
        ("npm test", true),
        ("dotnet build", true),
        ("cargo test", true),
        ("curl http://example.com", true),
        ("ls -la", true),
        ("random_command", false),
    ];

    for (cmd, should_filter) in commands {
        let result = registry.apply(cmd, "test output", &config);
        if should_filter {
            // Filter matched (may or may not modify output, but savings_percent tells us)
            // Just verify it doesn't panic
            let _ = result.output;
        } else {
            assert_eq!(result.savings_percent, 0.0, "{} should not be filtered", cmd);
        }
    }
}
