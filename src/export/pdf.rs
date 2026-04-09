use anyhow::Result;

pub fn export(output_path: Option<&str>, days: u32) -> Result<()> {
    // Strategy: Generate HTML first, then convert to PDF
    // Option 1: Use headless Chrome/Chromium (if available)
    // Option 2: Use wkhtmltopdf (if available)
    // Option 3: Use printpdf crate (pure Rust, no external deps)

    let html_path = output_path
        .map(|p| format!("{}.html", p.trim_end_matches(".pdf")))
        .unwrap_or_else(|| ".zeroctx/reports/temp_report.html".to_string());

    // Generate HTML first
    super::html::export(Some(&html_path), days)?;

    // Try to convert to PDF
    let pdf_path = output_path.unwrap_or("report.pdf");

    // Try Chrome headless
    let chrome_result = std::process::Command::new("chrome")
        .args([
            "--headless",
            "--disable-gpu",
            &format!("--print-to-pdf={}", pdf_path),
            &html_path,
        ])
        .output();

    if let Ok(output) = chrome_result {
        if output.status.success() {
            println!("Exported PDF report to {}", pdf_path);
            // Clean up temp HTML
            let _ = std::fs::remove_file(&html_path);
            return Ok(());
        }
    }

    // Try wkhtmltopdf
    let wk_result = std::process::Command::new("wkhtmltopdf")
        .args([&html_path, pdf_path])
        .output();

    if let Ok(output) = wk_result {
        if output.status.success() {
            println!("Exported PDF report to {}", pdf_path);
            let _ = std::fs::remove_file(&html_path);
            return Ok(());
        }
    }

    // Fallback: keep HTML and inform user
    println!(
        "PDF export requires Chrome or wkhtmltopdf. HTML report saved to: {}",
        html_path
    );
    println!("Install wkhtmltopdf or use: chrome --headless --print-to-pdf={} {}", pdf_path, html_path);

    Ok(())
}
