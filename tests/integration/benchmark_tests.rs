use zeroctx::compression::ast::AstCompressor;
use zeroctx::compression::compress_file;
use zeroctx::config::Config;
use zeroctx::core::types::Language;
use std::time::Instant;

fn fixtures_dir() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!("{}/tests/fixtures", manifest)
}

#[test]
fn bench_python_signatures() {
    let source = std::fs::read_to_string(format!("{}/sample.py", fixtures_dir())).unwrap();
    // Repeat source to simulate a large file
    let large = source.repeat(20);

    let start = Instant::now();
    for _ in 0..100 {
        let _ = AstCompressor::signatures_only(&large, Language::Python).unwrap();
    }
    let elapsed = start.elapsed();

    println!("Python signatures (100 iterations on ~1700 lines): {:?}", elapsed);
    assert!(elapsed.as_millis() < 15000, "should complete 100 iterations in < 5s (took {:?})", elapsed);
}

#[test]
fn bench_js_signatures() {
    let source = std::fs::read_to_string(format!("{}/sample.js", fixtures_dir())).unwrap();
    let large = source.repeat(20);

    let start = Instant::now();
    for _ in 0..100 {
        let _ = AstCompressor::signatures_only(&large, Language::JavaScript).unwrap();
    }
    let elapsed = start.elapsed();

    println!("JS signatures (100 iterations on ~1700 lines): {:?}", elapsed);
    assert!(elapsed.as_millis() < 15000, "should complete 100 iterations in < 5s (took {:?})", elapsed);
}

#[test]
fn bench_csharp_signatures() {
    let source = std::fs::read_to_string(format!("{}/sample.cs", fixtures_dir())).unwrap();
    let large = source.repeat(20);

    let start = Instant::now();
    for _ in 0..100 {
        let _ = AstCompressor::signatures_only(&large, Language::CSharp).unwrap();
    }
    let elapsed = start.elapsed();

    println!("C# signatures (100 iterations on ~1700 lines): {:?}", elapsed);
    assert!(elapsed.as_millis() < 15000, "should complete 100 iterations in < 5s (took {:?})", elapsed);
}

#[test]
fn bench_compress_file_python() {
    let path = format!("{}/sample.py", fixtures_dir());
    let config = Config::default();

    let start = Instant::now();
    for _ in 0..50 {
        let _ = compress_file(&path, &config).unwrap();
    }
    let elapsed = start.elapsed();

    println!("compress_file Python (50 iterations): {:?}", elapsed);
    assert!(elapsed.as_millis() < 10000, "should complete 50 iterations in < 3s (took {:?})", elapsed);
}

#[test]
fn bench_relevant_extraction() {
    let source = std::fs::read_to_string(format!("{}/sample.py", fixtures_dir())).unwrap();
    let large = source.repeat(20);
    let names = vec!["process".to_string(), "DataProcessor".to_string()];

    let start = Instant::now();
    for _ in 0..100 {
        let _ = AstCompressor::compress(&large, Language::Python, &names).unwrap();
    }
    let elapsed = start.elapsed();

    println!("Relevant extraction (100 iterations): {:?}", elapsed);
    assert!(elapsed.as_millis() < 15000, "should complete 100 iterations in < 5s (took {:?})", elapsed);
}
