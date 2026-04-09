use anyhow::{Context, Result};

/// Fetch a URL and extract text content.
pub async fn fetch_url(url: &str) -> Result<String> {
    let response = reqwest::get(url)
        .await
        .with_context(|| format!("Failed to fetch URL: {}", url))?;

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body = response
        .text()
        .await
        .with_context(|| format!("Failed to read response from: {}", url))?;

    if content_type.contains("json") {
        // Return JSON as-is (compact if possible)
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) {
            return Ok(serde_json::to_string_pretty(&value).unwrap_or(body));
        }
    }

    if content_type.contains("html") {
        // Strip HTML tags for text extraction
        return Ok(strip_html(&body));
    }

    Ok(body)
}

/// Smart HTML content extraction.
///
/// Strips noise (scripts, styles, nav, footer, ads, hidden elements)
/// before extracting visible text. Produces clean, readable output.
pub fn strip_html(html: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    // Phase 1: Remove non-visible blocks entirely
    static SCRIPT_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("valid regex"));
    static STYLE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("valid regex"));
    static COMMENT_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?s)<!--.*?-->").expect("valid regex"));
    static SVG_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<svg[^>]*>.*?</svg>").expect("valid regex"));
    static NOSCRIPT_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<noscript[^>]*>.*?</noscript>").expect("valid regex"));

    // Phase 2: Remove noisy structural elements
    static NAV_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<nav[^>]*>.*?</nav>").expect("valid regex"));
    static FOOTER_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<footer[^>]*>.*?</footer>").expect("valid regex"));
    static HEADER_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<header[^>]*>.*?</header>").expect("valid regex"));
    static ASIDE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<aside[^>]*>.*?</aside>").expect("valid regex"));
    static IFRAME_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<iframe[^>]*>.*?</iframe>").expect("valid regex"));

    // Phase 3: Remove ad/social/tracking divs by class/id patterns
    static AD_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?is)<div[^>]*(?:class|id)\s*=\s*"[^"]*(?:ad-|ads-|advert|banner|social|share|sidebar|popup|modal|overlay|cookie|consent|newsletter|subscribe|related|recommend)[^"]*"[^>]*>.*?</div>"#)
            .expect("valid regex")
    });

    // Phase 4: Convert block elements to newlines, strip remaining tags
    static BLOCK_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)</?(p|div|br|h[1-6]|li|tr|blockquote|section|article|pre|hr|dt|dd)[^>]*>")
            .expect("valid regex")
    });
    static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").expect("valid regex"));

    // Phase 5: Decode HTML entities and clean whitespace
    static ENTITY_AMP: Lazy<Regex> = Lazy::new(|| Regex::new(r"&amp;").expect("valid regex"));
    static ENTITY_LT: Lazy<Regex> = Lazy::new(|| Regex::new(r"&lt;").expect("valid regex"));
    static ENTITY_GT: Lazy<Regex> = Lazy::new(|| Regex::new(r"&gt;").expect("valid regex"));
    static ENTITY_QUOT: Lazy<Regex> = Lazy::new(|| Regex::new(r"&quot;").expect("valid regex"));
    static ENTITY_NBSP: Lazy<Regex> = Lazy::new(|| Regex::new(r"&nbsp;").expect("valid regex"));
    static ENTITY_NUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"&#\d+;").expect("valid regex"));
    static MULTI_NEWLINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").expect("valid regex"));
    static TRAILING_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]+\n").expect("valid regex"));
    static MULTI_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]{2,}").expect("valid regex"));

    // Apply phases
    let text = SCRIPT_RE.replace_all(html, "");
    let text = STYLE_RE.replace_all(&text, "");
    let text = COMMENT_RE.replace_all(&text, "");
    let text = SVG_RE.replace_all(&text, "");
    let text = NOSCRIPT_RE.replace_all(&text, "");

    let text = NAV_RE.replace_all(&text, "");
    let text = FOOTER_RE.replace_all(&text, "");
    let text = HEADER_RE.replace_all(&text, "");
    let text = ASIDE_RE.replace_all(&text, "");
    let text = IFRAME_RE.replace_all(&text, "");
    let text = AD_RE.replace_all(&text, "");

    let text = BLOCK_RE.replace_all(&text, "\n");
    let text = TAG_RE.replace_all(&text, "");

    let text = ENTITY_AMP.replace_all(&text, "&");
    let text = ENTITY_LT.replace_all(&text, "<");
    let text = ENTITY_GT.replace_all(&text, ">");
    let text = ENTITY_QUOT.replace_all(&text, "\"");
    let text = ENTITY_NBSP.replace_all(&text, " ");
    let text = ENTITY_NUM.replace_all(&text, "");

    let text = TRAILING_SPACE.replace_all(&text, "\n");
    let text = MULTI_SPACE.replace_all(&text, " ");
    let text = MULTI_NEWLINE.replace_all(&text, "\n\n");

    text.lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}
