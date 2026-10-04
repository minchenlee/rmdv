//! Full Mindmap side-panel preview: guarded off-thread reads, parsing, and asset indexing.

use super::*;

pub(super) fn collect_preview_assets(
    block: &Block,
    images: &mut Vec<String>,
    diagrams: &mut Vec<(u64, crate::ast::DiagramKind, String)>,
) {
    match block {
        Block::Image { url, .. } => images.push(url.clone()),
        Block::Diagram { hash, kind, source } => {
            diagrams.push((*hash, kind.clone(), source.clone()))
        }
        Block::Blockquote(blocks) => {
            for block in blocks {
                collect_preview_assets(block, images, diagrams);
            }
        }
        Block::List { items, .. } => {
            for item in items {
                for block in &item.blocks {
                    collect_preview_assets(block, images, diagrams);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn build_full_mindmap_preview_asset_index(
    blocks: &[(BlockId, Block)],
) -> FullMindmapPreviewAssetIndex {
    let mut by_block = HashMap::new();
    for (id, block) in blocks {
        let mut images = Vec::new();
        let mut diagrams = Vec::new();
        collect_preview_assets(block, &mut images, &mut diagrams);
        let mut assets = Vec::with_capacity(images.len() + diagrams.len());
        assets.extend(images.into_iter().map(FullMindmapPreviewAsset::Image));
        assets.extend(
            diagrams
                .into_iter()
                .map(|(hash, kind, source)| FullMindmapPreviewAsset::Diagram {
                    hash,
                    kind,
                    source,
                }),
        );
        if !assets.is_empty() {
            by_block.insert(*id, assets);
        }
    }
    FullMindmapPreviewAssetIndex { by_block }
}

pub(super) fn prettify_data(lang: &str, src: &str) -> String {
    if lang == "json" {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(src) {
            if let Ok(s) = serde_json::to_string_pretty(&v) {
                return s;
            }
        }
    }
    src.to_string()
}

pub(super) fn truncate_preview_source(mut source: String) -> (String, bool) {
    if source.len() <= MIND_PANEL_MAX_TEXT_BYTES {
        return (source, false);
    }
    let mut end = MIND_PANEL_MAX_TEXT_BYTES;
    while !source.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    source.truncate(end);
    (source, true)
}

/// Parse and highlight a complete preview away from the Iced update thread.
/// The static highlighter cache is mutex-protected, so worker results remain
/// independent from the current document's mutable `HlCache`.
pub(super) fn parse_full_mindmap_preview_blocking(
    path: PathBuf,
    source: String,
) -> FullMindmapPreview {
    if let Some(lang) = data_lang_for(Some(&path)) {
        let (source, source_truncated) = truncate_preview_source(source);
        let (pretty, pretty_truncated) = truncate_preview_source(prettify_data(lang, &source));
        return FullMindmapPreview::Data {
            path,
            source: pretty,
            truncated: source_truncated || pretty_truncated,
        };
    }

    let (mut blocks, _) = if is_tex_path(Some(&path)) {
        crate::tex::parse(&source)
    } else {
        parser::parse(&source)
    };
    for (_, block) in &mut blocks {
        if let Block::CodeBlock {
            lang: Some(lang),
            code,
            spans,
        } = block
        {
            if spans.is_empty() {
                *spans = crate::highlight::highlight(lang, code);
            }
        }
    }
    let shape = Arc::new(crate::virt::VirtWindow::shape(
        &blocks,
        &HashSet::new(),
        &crate::virt::HeightCache::default(),
    ));
    let assets = Arc::new(build_full_mindmap_preview_asset_index(&blocks));
    FullMindmapPreview::Document {
        path,
        blocks,
        truncated: false,
        shape: Some(shape),
        assets: Some(assets),
    }
}

pub(super) fn full_mindmap_preview_settle_stream(
    updates: tokio::sync::watch::Receiver<Option<PendingFullMindmapPreviewSettle>>,
) -> impl futures::Stream<Item = PendingFullMindmapPreviewSettle> {
    futures::stream::unfold((updates, None), |(mut updates, last_emitted)| async move {
        loop {
            let request = loop {
                if let Some(request) = updates.borrow().clone() {
                    if last_emitted.as_ref() != Some(&request) {
                        break request;
                    }
                }
                if updates.changed().await.is_err() {
                    return None;
                }
            };
            let timer = tokio::time::sleep(std::time::Duration::from_millis(
                FULL_MINDMAP_PREVIEW_SETTLE_MS,
            ));
            tokio::pin!(timer);
            loop {
                tokio::select! {
                    changed = updates.changed() => {
                        if changed.is_err() {
                            return None;
                        }
                        // The watch value is the only settle owner. Restart
                        // the quiet window around whatever request is current.
                        break;
                    }
                    _ = &mut timer => {
                        if updates.borrow().as_ref() == Some(&request) {
                            return Some((request.clone(), (updates, Some(request))));
                        }
                        break;
                    }
                }
            }
        }
    })
}

pub(super) fn full_mindmap_preview_work_gate() -> &'static tokio::sync::Semaphore {
    static GATE: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Semaphore::const_new(1))
}

pub(super) fn preview_work_is_current(cancel: &AtomicU64, epoch: u64) -> bool {
    cancel.load(Ordering::Acquire) == epoch
}

pub(super) async fn acquire_full_mindmap_preview_work(
    cancel: &Arc<AtomicU64>,
    epoch: u64,
) -> Result<tokio::sync::SemaphorePermit<'static>, String> {
    loop {
        if !preview_work_is_current(cancel, epoch) {
            return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
        }
        match full_mindmap_preview_work_gate().try_acquire() {
            Ok(permit) => return Ok(permit),
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                tokio::time::sleep(std::time::Duration::from_millis(8)).await;
            }
            Err(tokio::sync::TryAcquireError::Closed) => {
                return Err("preview worker gate closed".to_string());
            }
        }
    }
}

pub(super) fn read_full_mindmap_preview_source_blocking(
    path: PathBuf,
    max_bytes: Option<usize>,
    cancel: Option<Arc<AtomicU64>>,
    epoch: u64,
) -> Result<String, String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    let mut chunk = vec![0u8; FULL_MINDMAP_PREVIEW_READ_CHUNK_BYTES];
    loop {
        if cancel
            .as_ref()
            .is_some_and(|token| !preview_work_is_current(token, epoch))
        {
            return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
        }
        let read = file.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(limit) = max_bytes {
            if bytes.len() >= limit {
                bytes.truncate(limit);
                break;
            }
        }
    }
    if cancel
        .as_ref()
        .is_some_and(|token| !preview_work_is_current(token, epoch))
    {
        return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Complete read with a single cooperative worker. A newer selection bumps
/// the token; stale queued work exits before taking the gate and an active
/// read checks between bounded chunks, so rapid navigation cannot retain a
/// backlog of large sources.
pub(super) async fn load_full_mindmap_preview_guarded(
    path: PathBuf,
    cancel: Arc<AtomicU64>,
    epoch: u64,
) -> Result<(PathBuf, String), String> {
    #[cfg(feature = "pdf")]
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        return Err("PDF preview unavailable — press Enter to open".to_string());
    }
    let _permit = acquire_full_mindmap_preview_work(&cancel, epoch).await?;
    let max_bytes = data_lang_for(Some(&path)).map(|_| MIND_PANEL_MAX_TEXT_BYTES + 1);
    let read_path = path.clone();
    let read_cancel = Arc::clone(&cancel);
    let source = tokio::task::spawn_blocking(move || {
        read_full_mindmap_preview_source_blocking(read_path, max_bytes, Some(read_cancel), epoch)
    })
    .await
    .map_err(|error| error.to_string())??;
    if !preview_work_is_current(&cancel, epoch) {
        return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
    }
    Ok((path, source))
}

/// Parse and highlight off the update thread. Parsing uses a small
/// per-navigator capacity (separate from the read gate): an obsolete large
/// parse may finish in parallel with the current selection without
/// monopolizing a process-global slot, while bounded permits prevent a rapid
/// sequence from filling the blocking pool. Epoch checks before and after the
/// operation reject stale results.
pub(super) async fn parse_full_mindmap_preview_guarded(
    path: PathBuf,
    source: Arc<str>,
    cancel: Arc<AtomicU64>,
    epoch: u64,
    parse_gate: Arc<tokio::sync::Semaphore>,
) -> Result<FullMindmapPreview, String> {
    if !preview_work_is_current(&cancel, epoch) {
        return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
    }
    let parse_permit = loop {
        if !preview_work_is_current(&cancel, epoch) {
            return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
        }
        match Arc::clone(&parse_gate).try_acquire_owned() {
            Ok(permit) => break permit,
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                tokio::time::sleep(std::time::Duration::from_millis(8)).await;
            }
            Err(tokio::sync::TryAcquireError::Closed) => {
                return Err("preview parse gate closed".to_string());
            }
        }
    };
    let parse_cancel = Arc::clone(&cancel);
    tokio::task::spawn_blocking(move || {
        let _parse_permit = parse_permit;
        if !preview_work_is_current(&parse_cancel, epoch) {
            return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
        }
        let preview = parse_full_mindmap_preview_blocking(path, source.to_string());
        if !preview_work_is_current(&parse_cancel, epoch) {
            return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
        }
        Ok(preview)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Read the complete Full Mindmap side-panel source on a worker. PDFs remain
/// openable with Enter but are intentionally not converted just for a preview.
#[cfg(test)]
pub(super) async fn load_full_mindmap_preview(p: PathBuf) -> Result<(PathBuf, String), String> {
    #[cfg(feature = "pdf")]
    if p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        return Err("PDF preview unavailable — press Enter to open".to_string());
    }
    let source = if data_lang_for(Some(&p)).is_some() {
        let path = p.clone();
        tokio::task::spawn_blocking(move || {
            use std::io::Read;

            let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
            let mut limited = file.take((MIND_PANEL_MAX_TEXT_BYTES + 1) as u64);
            let mut bytes = Vec::with_capacity(MIND_PANEL_MAX_TEXT_BYTES + 1);
            limited
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            Ok::<String, String>(String::from_utf8_lossy(&bytes).into_owned())
        })
        .await
        .map_err(|error| error.to_string())??
    } else {
        tokio::fs::read(&p)
            .await
            .map_err(|error| error.to_string())
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())?
    };
    Ok((p, source))
}
