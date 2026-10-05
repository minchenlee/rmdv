//! Async document, workspace, and image loading with size limits.

use super::*;

pub(super) async fn load_file(p: PathBuf) -> Result<(PathBuf, String), String> {
    let p = canonicalize_existing_path(p);
    #[cfg(feature = "pdf")]
    if p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        let path = p.clone();
        // PDFium is sync + holds a process-global lock; run off the async runtime.
        let md = tokio::task::spawn_blocking(move || crate::pdf::pdf_to_markdown(&path))
            .await
            .map_err(|e| e.to_string())??;
        return Ok((p, md));
    }
    let bytes = read_document_bytes(&p).await?;
    Ok((p, document_text(bytes)))
}

/// Largest document the main viewer will read. Files past this are refused
/// before reading instead of being pulled whole into memory.
pub(super) const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

pub(super) async fn read_document_bytes(p: &Path) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncReadExt;

    let file = tokio::fs::File::open(p).await.map_err(|e| e.to_string())?;
    let len = file.metadata().await.map_err(|e| e.to_string())?.len();
    if len > MAX_DOCUMENT_BYTES {
        return Err(too_large_message(len));
    }
    // The size can change between the check and the read; never read past
    // the cap either way.
    let mut bytes = Vec::with_capacity(len as usize);
    file.take(MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(too_large_message(bytes.len() as u64));
    }
    Ok(bytes)
}

pub(super) fn too_large_message(len: u64) -> String {
    format!(
        "File is too large to open ({} MB; the limit is {} MB)",
        len / (1024 * 1024),
        MAX_DOCUMENT_BYTES / (1024 * 1024)
    )
}

/// Decode without copying valid UTF-8; only invalid input pays for a lossy
/// conversion.
pub(super) fn document_text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

/// Build the tree and file-finder index together on a blocking worker. The
/// tree-side entry/file budgets guarantee a very large project cannot grow
/// this task without bound.
pub(super) async fn load_workspace_snapshot(
    path: PathBuf,
    show_hidden: bool,
) -> Result<(PathBuf, tree::WorkspaceSnapshot), String> {
    let scan_path = path.clone();
    let snapshot =
        tokio::task::spawn_blocking(move || tree::build_workspace(&scan_path, show_hidden))
            .await
            .map_err(|error| error.to_string())??;
    Ok((path, snapshot))
}

pub(super) async fn load_full_mindmap_folder(
    path: PathBuf,
    show_hidden: bool,
) -> Result<(PathBuf, tree::ExpandedFolderSnapshot), String> {
    let scan_path = path.clone();
    let snapshot =
        tokio::task::spawn_blocking(move || tree::load_expanded_folder(&scan_path, show_hidden))
            .await
            .map_err(|error| error.to_string())??;
    Ok((path, snapshot))
}

/// Remote images larger than this are refused instead of buffered whole.
pub(super) const MAX_REMOTE_IMAGE_BYTES: usize = 25 * 1024 * 1024;

/// Concurrent remote image downloads; a document with many images queues the
/// rest instead of opening one connection per image at once.
pub(super) const REMOTE_IMAGE_CONCURRENCY: usize = 4;

/// One HTTP client for every image fetch, so TLS setup and pooled connections
/// are reused. Iced drives all tasks on a single runtime, which the pool needs.
pub(super) static IMAGE_CLIENT: std::sync::LazyLock<Result<reqwest::Client, String>> =
    std::sync::LazyLock::new(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent(concat!("rmdv/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())
    });

pub(super) static IMAGE_FETCH_PERMITS: std::sync::LazyLock<tokio::sync::Semaphore> =
    std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(REMOTE_IMAGE_CONCURRENCY));

pub(super) async fn fetch_image(url: String) -> (String, Result<Vec<u8>, String>) {
    let res = async {
        let client = IMAGE_CLIENT.as_ref().map_err(Clone::clone)?;
        let _permit = IMAGE_FETCH_PERMITS
            .acquire()
            .await
            .map_err(|e| e.to_string())?;
        let mut resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("http {}", resp.status()));
        }
        let too_large = || format!("image larger than {} MB", MAX_REMOTE_IMAGE_BYTES >> 20);
        let declared = resp.content_length().unwrap_or(0) as usize;
        if declared > MAX_REMOTE_IMAGE_BYTES {
            return Err(too_large());
        }
        // Stream so an undeclared or lying length still stops at the cap.
        let mut bytes = Vec::with_capacity(declared);
        while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
            if bytes.len() + chunk.len() > MAX_REMOTE_IMAGE_BYTES {
                return Err(too_large());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok::<Vec<u8>, String>(bytes)
    }
    .await;
    (url, res)
}

/// Local images larger than this are not loaded.
pub(super) const MAX_LOCAL_IMAGE_BYTES: u64 = 64 * 1024 * 1024;

/// Cache key for a document's local image: its resolved path. The zoom modal
/// opens the same key.
pub(super) fn local_image_key(url: &str, current_file: Option<&std::path::Path>) -> Option<String> {
    resolve_image_path(url, current_file).map(|path| path.to_string_lossy().into_owned())
}

/// Read a local image off the UI thread, keyed like `fetch_image` results.
pub(super) async fn read_local_image(key: String) -> (String, Result<Vec<u8>, String>) {
    let res = async {
        let path = std::path::Path::new(&key);
        let len = tokio::fs::metadata(path)
            .await
            .map_err(|e| e.to_string())?
            .len();
        if len > MAX_LOCAL_IMAGE_BYTES {
            return Err(format!(
                "image larger than {} MB",
                MAX_LOCAL_IMAGE_BYTES >> 20
            ));
        }
        tokio::fs::read(path).await.map_err(|e| e.to_string())
    }
    .await;
    (key, res)
}

/// Rasterize SVG bytes to RGBA. Target ~2048px on the longer side.
pub fn rasterize_svg(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    use resvg::tiny_skia;
    use resvg::usvg;
    const TARGET: f32 = 2048.0;
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes, &opt).map_err(|e| e.to_string())?;
    let sz = tree.size();
    let (w, h) = (sz.width(), sz.height());
    if w <= 0.0 || h <= 0.0 {
        return Err("svg has zero size".into());
    }
    let scale = (TARGET / w.max(h)).max(1.0);
    let pw = (w * scale).round() as u32;
    let ph = (h * scale).round() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph).ok_or("pixmap alloc failed")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok((pixmap.take(), pw, ph))
}

pub fn is_svg_bytes(b: &[u8]) -> bool {
    let head = &b[..b.len().min(512)];
    let s = std::str::from_utf8(head).unwrap_or("");
    let s = s.trim_start();
    s.starts_with("<svg") || s.starts_with("<?xml")
}
