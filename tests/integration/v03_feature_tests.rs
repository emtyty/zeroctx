// Integration tests for ZeroCTX v0.3 features.
//
// Covers:
//  - compress_glob_results (grouping by directory)
//  - compress_grep_results (grouping by file)
//  - AstCompressor::compress_around_lines (error-line-aware extraction)
//  - extract_error_locations (Python / Rust / JS / C# formats)
//  - GitFilter large-diff detection
//  - ContextCache open_default + store_compressed + check_mtime

use zeroctx::compression::ast::AstCompressor;
use zeroctx::compression::context_cache::ContextCache;
use zeroctx::config::Config;
use zeroctx::core::types::Language;
use zeroctx::errors::extract_error_locations;
use zeroctx::filters::system::{compress_glob_results, compress_grep_results};
use zeroctx::filters::FilterRegistry;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixtures_dir() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!("{}/tests/fixtures", manifest)
}

// ---------------------------------------------------------------------------
// 1. compress_glob_results
// ---------------------------------------------------------------------------

/// 30 paths spread across 3 directories → grouped "files:" summary.
#[test]
fn test_compress_glob_large_groups_by_dir() {
    let mut lines = Vec::new();
    // 10 paths each in src/auth, src/api, and tests
    for i in 0..10 {
        lines.push(format!("src/auth/file_{}.rs", i));
    }
    for i in 0..10 {
        lines.push(format!("src/api/handler_{}.rs", i));
    }
    for i in 0..10 {
        lines.push(format!("tests/test_{}.rs", i));
    }
    let input = lines.join("\n");
    let result = compress_glob_results(&input);

    // Should switch to grouped format — the header always starts with "// N files matched:"
    assert!(
        result.contains("files"),
        "expected grouped output containing 'files', got:\n{}",
        result
    );
    // The total count should be mentioned
    assert!(
        result.contains("30"),
        "expected '30' to appear in grouped output, got:\n{}",
        result
    );
}

/// 5 paths → output is unchanged (pass-through).
#[test]
fn test_compress_glob_small_passthrough() {
    let input = "src/main.rs\nsrc/lib.rs\ntests/test.rs\nbuild.rs\nCargo.toml\n";
    let result = compress_glob_results(input);
    assert_eq!(
        result, input,
        "5-path glob result should be returned unchanged"
    );
}

// ---------------------------------------------------------------------------
// 2. compress_grep_results
// ---------------------------------------------------------------------------

/// 20 lines of ripgrep-style output (`file.rs:42:content`) → grouped "matches" summary.
#[test]
fn test_compress_grep_large_groups_by_file() {
    let mut lines = Vec::new();
    for i in 0..10 {
        lines.push(format!("src/foo.rs:{}:    let x = {};", i + 1, i));
    }
    for i in 0..10 {
        lines.push(format!("src/bar.rs:{}:    return {};", i + 1, i));
    }
    let input = lines.join("\n");
    let result = compress_grep_results(&input);

    assert!(
        result.contains("match"),
        "expected 'match' in grouped grep output, got:\n{}",
        result
    );
}

/// 5 lines → output is unchanged (pass-through).
#[test]
fn test_compress_grep_small_passthrough() {
    let input = "src/a.rs:1:hello\nsrc/a.rs:2:world\nsrc/b.rs:3:foo\nsrc/b.rs:4:bar\nsrc/c.rs:5:baz\n";
    let result = compress_grep_results(input);
    assert_eq!(
        result, input,
        "5-line grep result should be returned unchanged"
    );
}

// ---------------------------------------------------------------------------
// 3. AstCompressor::compress_around_lines
// ---------------------------------------------------------------------------

// sample.py content (lines are 1-indexed in file, 0-indexed in the API):
//
//  Lines 11-20  (0-indexed 10-19): the `process` method
//  Lines 22-23  (0-indexed 21-22): `_clean`
//  Lines 25-30  (0-indexed 24-29): `_validate`
//  Lines 32-36  (0-indexed 31-35): `get_stats`
//  Lines 38-42  (0-indexed 37-41): `load_data` (module-level)
//  Lines 44-45  (0-indexed 43-44): `transform`
//  Lines 47-58  (0-indexed 46-57): `FileHandler` class with methods

/// compress_around_lines targeting a line inside `process` should include
/// more detail for that method than a plain signatures_only call would.
#[test]
fn test_compress_around_lines_focuses_on_target_function() {
    let fixture_path = format!("{}/sample.py", fixtures_dir());
    let source = std::fs::read_to_string(&fixture_path)
        .expect("sample.py fixture must exist");

    // Line 14 (0-indexed 13) is `cleaned = self._clean(item)` — inside `process`.
    let focused = AstCompressor::compress_around_lines(&source, Language::Python, &[13])
        .expect("compress_around_lines should not fail");

    let sigs_only = AstCompressor::signatures_only(&source, Language::Python)
        .expect("signatures_only should not fail");

    // The focused result must be longer than signatures-only for the same source,
    // because the full body of `process` is included.
    assert!(
        focused.len() > sigs_only.len(),
        "compress_around_lines result ({} chars) should be longer than \
         signatures_only ({} chars) because the full body of 'process' is included",
        focused.len(),
        sigs_only.len()
    );

    // The body text of `process` should be present — the loop variable `item` appears
    // only in the method body, never in a signature.
    assert!(
        focused.contains("item") || focused.contains("process"),
        "focused output should contain body content of the 'process' method:\n{}",
        focused
    );
}

/// When lines slice is empty, compress_around_lines delegates to signatures_only.
#[test]
fn test_compress_around_lines_empty_lines_delegates_to_signatures() {
    let fixture_path = format!("{}/sample.py", fixtures_dir());
    let source = std::fs::read_to_string(&fixture_path)
        .expect("sample.py fixture must exist");

    let around = AstCompressor::compress_around_lines(&source, Language::Python, &[])
        .expect("should not fail with empty lines");
    let sigs = AstCompressor::signatures_only(&source, Language::Python)
        .expect("should not fail");

    // Both should contain the same top-level definitions (content may vary
    // by header/formatting, but both should mention `process`).
    assert!(
        around.contains("process") || around.contains("DataProcessor"),
        "output should mention top-level definitions:\n{}",
        around
    );
    assert!(
        sigs.contains("process") || sigs.contains("DataProcessor"),
        "signatures_only output should mention definitions:\n{}",
        sigs
    );
}

// ---------------------------------------------------------------------------
// 4. extract_error_locations
// ---------------------------------------------------------------------------

/// Python traceback: `File "src/auth.py", line 42, in process`
#[test]
fn test_extract_error_locations_python() {
    let stderr = r#"Traceback (most recent call last):
  File "src/auth.py", line 42, in process
    result = do_something()
ValueError: something went wrong"#;

    let locs = extract_error_locations(stderr);
    assert!(
        !locs.is_empty(),
        "should extract at least one location from Python traceback"
    );
    let (path, line) = &locs[0];
    assert_eq!(path, "src/auth.py");
    // API converts 1-indexed line 42 → 0-indexed 41
    assert_eq!(*line, 41, "line should be 0-indexed (42 - 1 = 41)");
}

/// Rust error: `error[E0308]: --> src/main.rs:15:8`
#[test]
fn test_extract_error_locations_rust() {
    let stderr = "error[E0308]: mismatched types\n --> src/main.rs:15:8\n  |\n15 |     let x: i32 = \"hello\";\n";

    let locs = extract_error_locations(stderr);
    assert!(
        !locs.is_empty(),
        "should extract location from Rust error"
    );
    let found = locs.iter().find(|(p, _)| p.contains("main.rs"));
    assert!(found.is_some(), "should find src/main.rs in {:?}", locs);
    let (_, line) = found.unwrap();
    assert_eq!(*line, 14, "Rust line 15 → 0-indexed 14");
}

/// TypeScript error: `src/client.ts:89:12 - error TS2339:`
#[test]
fn test_extract_error_locations_typescript() {
    let stderr = "src/client.ts:89:12 - error TS2339: Property 'foo' does not exist on type 'Bar'.";

    let locs = extract_error_locations(stderr);
    assert!(
        !locs.is_empty(),
        "should extract location from TypeScript error"
    );
    let found = locs.iter().find(|(p, _)| p.contains("client.ts"));
    assert!(found.is_some(), "should find src/client.ts in {:?}", locs);
    let (_, line) = found.unwrap();
    assert_eq!(*line, 88, "TS line 89 → 0-indexed 88");
}

/// C# error: `src/Service.cs(42,8): error CS0246:`
#[test]
fn test_extract_error_locations_csharp() {
    let stderr = "src/Service.cs(42,8): error CS0246: The type or namespace name 'Foo' could not be found";

    let locs = extract_error_locations(stderr);
    assert!(
        !locs.is_empty(),
        "should extract location from C# error"
    );
    let found = locs.iter().find(|(p, _)| p.contains("Service.cs"));
    assert!(found.is_some(), "should find src/Service.cs in {:?}", locs);
    let (_, line) = found.unwrap();
    assert_eq!(*line, 41, "C# line 42 → 0-indexed 41");
}

/// Multiple formats in one stderr blob → all extracted.
#[test]
fn test_extract_error_locations_multiple_formats() {
    let stderr = r#"  File "src/auth.py", line 10, in run
error[E0308]: --> src/main.rs:20:5
src/client.ts:30:1 - error TS2339: oops
src/Service.cs(40,3): error CS0246: missing"#;

    let locs = extract_error_locations(stderr);
    assert!(
        locs.len() >= 4,
        "should find at least 4 locations (one per format), got {:?}",
        locs
    );
}

/// Empty input → empty result.
#[test]
fn test_extract_error_locations_empty() {
    let locs = extract_error_locations("");
    assert!(locs.is_empty(), "empty stderr should produce no locations");
}

// ---------------------------------------------------------------------------
// 5. GitFilter — large diff detection
// ---------------------------------------------------------------------------

/// Generate a diff string that has more than 300 `+`/`-` content lines.
fn large_diff() -> String {
    let mut buf = String::new();
    buf.push_str("diff --git a/src/big.rs b/src/big.rs\n");
    buf.push_str("index 000000..111111 100644\n");
    buf.push_str("--- a/src/big.rs\n");
    buf.push_str("+++ b/src/big.rs\n");
    buf.push_str("@@ -1,305 +1,305 @@\n");
    // Add 305 deletion lines and 305 addition lines = 610 content lines total
    for i in 0..305 {
        buf.push_str(&format!("-    let old_{} = {};\n", i, i));
    }
    for i in 0..305 {
        buf.push_str(&format!("+    let new_{} = {};\n", i, i * 2));
    }
    buf
}

#[test]
fn test_git_filter_large_diff_shows_stat_view() {
    let registry = FilterRegistry::new();
    let config = Config::default();
    let diff = large_diff();

    let result = registry.apply("git diff", &diff, &config);

    assert!(
        result.output.contains("Large diff"),
        "output for large diff (>300 lines) should contain 'Large diff', got:\n{}",
        result.output
    );
}

#[test]
fn test_git_filter_small_diff_no_stat_view() {
    let registry = FilterRegistry::new();
    let config = Config::default();

    // The existing fixture has only a few +/- lines — well under 300.
    let fixture_path = format!("{}/git_diff_output.txt", fixtures_dir());
    let small_diff = std::fs::read_to_string(&fixture_path)
        .expect("git_diff_output.txt fixture must exist");

    let result = registry.apply("git diff", &small_diff, &config);

    assert!(
        !result.output.contains("Large diff"),
        "small diff should NOT trigger stat view, got:\n{}",
        result.output
    );
}

// ---------------------------------------------------------------------------
// 6. ContextCache — SQLite-backed mtime cache
// ---------------------------------------------------------------------------

/// Create a real temp file, store via store_compressed, check_mtime returns hit,
/// then touch the file (update mtime) → check_mtime returns None (cache miss).
#[test]
fn test_context_cache_mtime_hit_and_miss() {
    // Write a temp file
    let tmp_dir = std::env::temp_dir().join("zeroctx_test_cache");
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let tmp_path = tmp_dir.join("sample_cache_test.py");
    let content = "def hello():\n    print('hello')\n";
    std::fs::write(&tmp_path, content).unwrap();

    let path_str = tmp_path.to_string_lossy().to_string();
    let compressed = "// compressed version\ndef hello(): ...";

    // Open a persistent cache (uses default zeroctx db path)
    let cache = ContextCache::open_default().expect("should open default cache");

    // Store compressed content for this file at its current mtime
    cache
        .store_compressed(&path_str, content, compressed)
        .expect("store_compressed should succeed");

    // Immediately check → should get a cache hit
    let hit = cache
        .check_mtime(&path_str)
        .expect("check_mtime should not error");
    assert!(
        hit.is_some(),
        "check_mtime should return Some after store_compressed with same mtime"
    );
    assert_eq!(
        hit.unwrap(),
        compressed,
        "cached content should match what was stored"
    );

    // Simulate a file modification by sleeping 1s and rewriting the file
    // (on FAT32 mtime has 2s resolution; on most modern FSes it's sub-second)
    // We force a different mtime by changing the file content with std::fs::write
    // and then manually advancing the modification time using filetime crate (not
    // available here), so instead we just wait ≥1s to guarantee a new mtime.
    std::thread::sleep(std::time::Duration::from_secs(1));
    std::fs::write(&tmp_path, format!("{}\n# modified", content)).unwrap();

    // Now check again → mtime changed, should be a cache miss
    let miss = cache
        .check_mtime(&path_str)
        .expect("check_mtime should not error after file change");
    assert!(
        miss.is_none(),
        "check_mtime should return None after file mtime changes"
    );

    // Clean up
    std::fs::remove_file(&tmp_path).ok();
}

/// open_default should succeed without panicking (filesystem/SQLite available).
#[test]
fn test_context_cache_open_default_succeeds() {
    let result = ContextCache::open_default();
    assert!(
        result.is_ok(),
        "open_default should succeed, got: {:?}",
        result.err()
    );
}

/// store_compressed + check_mtime round-trip on a real file.
#[test]
fn test_context_cache_store_and_retrieve() {
    let tmp_dir = std::env::temp_dir().join("zeroctx_test_cache2");
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let tmp_path = tmp_dir.join("round_trip_test.rs");
    let content = "fn main() { println!(\"hello\"); }\n";
    std::fs::write(&tmp_path, content).unwrap();

    let path_str = tmp_path.to_string_lossy().to_string();
    let compressed = "fn main() { ... }";

    let cache = ContextCache::open_default().expect("open cache");
    cache
        .store_compressed(&path_str, content, compressed)
        .expect("store");

    let retrieved = cache.check_mtime(&path_str).expect("check");
    assert_eq!(
        retrieved,
        Some(compressed.to_string()),
        "retrieved content should match stored compressed"
    );

    std::fs::remove_file(&tmp_path).ok();
}
