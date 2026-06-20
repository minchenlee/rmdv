//! PDF → markdown via liteparse (PDFium-backed, local-only). Feature-gated
//! behind `pdf`. PDFium is loaded at runtime by dlopen; the dylib must sit
//! next to the executable (in the .app: `Contents/MacOS/libpdfium.dylib`).
//!
//! OCR (Tesseract) is intentionally disabled — `liteparse` is pulled with
//! `default-features = false`, so scanned/image-only PDFs yield no text. The
//! common case (PDFs with a native text layer) works without it.

use liteparse::config::ImageMode;
use liteparse::extract::extract_pages_from_input;
use liteparse::types::PdfInput;
use liteparse::{LiteParse, LiteParseConfig, OutputFormat};

/// Extract a PDF file's text as markdown. Blocking: PDFium is synchronous and
/// holds a process-global lock, so callers should run this on a blocking thread.
pub fn pdf_to_markdown(path: &std::path::Path) -> Result<String, String> {
    let input = PdfInput::Path(path.to_string_lossy().into_owned());

    let pages = extract_pages_from_input(&input, None, usize::MAX, None)
        .map_err(|e| format!("pdf parse failed: {e}"))?;

    let config = LiteParseConfig {
        output_format: OutputFormat::Markdown,
        ocr_enabled: false,
        // Strip raster image refs: we have no bytes to back them (no Embed),
        // and dangling `![](image_pN_K.png)` links would render broken.
        image_mode: ImageMode::Off,
        extract_links: true,
        ..LiteParseConfig::default()
    };

    let result = LiteParse::new(config).parse_from_pages(pages, Vec::new());
    Ok(result.text)
}
