//! Full Mindmap Mode state: pending-request records, preview state, and limits.

use super::*;

pub(super) fn full_mindmap_space_message() -> Message {
    Message::FullMindmapToggleSelected
}

pub(super) fn full_mindmap_preview_scroll_tag(
    full: &FullMindmapState,
) -> (u64, Option<PathBuf>, u64) {
    if let Some(request) = full.preview_identity.as_ref() {
        return (
            full.preview_namespace,
            Some(request.path.clone()),
            request.id,
        );
    }
    if let Some(request) = full.pending_preview.as_ref() {
        return (
            full.preview_namespace,
            Some(request.path.clone()),
            request.id,
        );
    }
    if let Some(request) = full.pending_preview_settle.as_ref() {
        return (
            full.preview_namespace,
            Some(request.path.clone()),
            request.id,
        );
    }
    (full.preview_namespace, None, 0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapOpen {
    pub id: u64,
    pub path: PathBuf,
}

/// Read-only side-panel preview identity. It is separate from a pending open:
/// selecting nodes must never replace the current document or consume its
/// dirty-file guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapPreview {
    pub id: u64,
    pub path: PathBuf,
}

/// Preview-owned asset identity. The namespace survives Full Mindmap
/// re-entry, while the request id changes for every accepted file preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullMindmapPreviewIdentity {
    pub namespace: u64,
    pub request: PendingFullMindmapPreview,
}

/// Ownership of one image/diagram operation in one materialized virtual
/// range. The preview request alone is insufficient: a late completion from
/// an older range must not mutate the current wave's Loading/Pending sentinel
/// or terminal-failure state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullMindmapPreviewAssetIdentity {
    pub preview: FullMindmapPreviewIdentity,
    pub range: (usize, usize),
    pub wave: u64,
}

/// Identity for the short settle window before a selected Full Mindmap file
/// starts its complete read-only preview. This is separate from
/// `PendingFullMindmapPreview`: no file read owns the navigator until this
/// debounce message is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapPreviewSettle {
    pub id: u64,
    pub path: PathBuf,
}

/// Identity for a bounded workspace index requested by Full Mindmap. The
/// result is applied only while this exact request still owns the navigator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapWorkspaceLoad {
    pub id: u64,
    pub path: PathBuf,
    pub select_root: bool,
    /// Folder-picker fallback can request that a file open begin only after its
    /// parent workspace index is ready, avoiding a synchronous scan race.
    pub open_after: Option<PathBuf>,
    /// Hidden-entry refreshes keep the user's current expansion and selection
    /// after the background snapshot replaces the tree.
    pub preserve_navigation: bool,
    /// Returning to Files with a changed hidden-file filter waits for this
    /// snapshot, then reveals the refreshed sidebar directly.
    pub return_to_files_after: bool,
    /// Normal Esc/toggle exits also wait for a stale hidden sidebar snapshot,
    /// but restore the prior underlying surface instead of forcing Files.
    pub exit_after_refresh: bool,
}

/// Identity for one expanded folder's bounded branch-local discovery. The
/// workspace/filter tuple prevents a late completion from another explorer
/// generation from repopulating a collapsed or replaced branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapFolderLoad {
    pub id: u64,
    pub workspace_root: PathBuf,
    pub folder: PathBuf,
    pub show_hidden: bool,
}

/// Identity for one candidate in a fixed delayed-reveal verification wave.
/// Unlike ordinary expanded-folder materialization, these requests are owned
/// by both the wave and the parent expansion generation. A result from a
/// collapsed/re-rooted/refiltered navigator therefore cannot reveal a shell
/// in a newer graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapVerification {
    pub id: u64,
    pub wave_id: u64,
    pub workspace_root: PathBuf,
    pub parent: PathBuf,
    pub parent_expansion_generation: u64,
    pub folder: PathBuf,
    pub show_hidden: bool,
}

/// Fixed, bounded verification ownership. `candidates` is snapshotted at wave
/// start, so `total` never grows while completions arrive. At most
/// `FULL_MINDMAP_VERIFICATION_CONCURRENCY` requests are in flight and the
/// candidate cap leaves excess LowerBound(0) shells visible with their truthful
/// scan-limit label instead of silently dropping them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullMindmapVerificationWave {
    pub id: u64,
    pub workspace_root: PathBuf,
    pub show_hidden: bool,
    pub parent_expansion_generation: u64,
    pub candidates: Vec<PathBuf>,
    pub parent_by_candidate: HashMap<PathBuf, PathBuf>,
    pub next_index: usize,
    pub in_flight: HashSet<PathBuf>,
    pub request_ids: HashMap<PathBuf, u64>,
    pub checked: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullMindmapProgress {
    pub wave_id: u64,
    pub checked: usize,
    pub total: usize,
}

#[derive(Debug, Clone)]
pub(super) enum FullMindmapPreviewAsset {
    Image(String),
    Diagram {
        hash: u64,
        kind: crate::ast::DiagramKind,
        source: String,
    },
}

/// Compact worker-built asset index keyed by top-level preview block. The UI
/// only clones descriptors for the currently materialized virtual range;
/// nested assets in off-screen blocks never trigger an O(all-blocks) scan.
#[derive(Debug, Clone, Default)]
pub struct FullMindmapPreviewAssetIndex {
    pub(super) by_block: HashMap<BlockId, Vec<FullMindmapPreviewAsset>>,
}

#[derive(Debug, Clone)]
pub enum FullMindmapPreview {
    None,
    Loading(PathBuf),
    Document {
        path: PathBuf,
        blocks: Vec<(BlockId, Block)>,
        truncated: bool,
        /// Estimate display/prefix shape built by the preview worker. `None`
        /// is retained for small test fixtures and legacy callers; runtime
        /// complete previews always carry the worker-built shape.
        shape: Option<Arc<crate::virt::VirtShape>>,
        /// Worker-built compact asset descriptors. `None` is retained for
        /// hand-authored fixtures; the runtime parser always provides it.
        assets: Option<Arc<FullMindmapPreviewAssetIndex>>,
    },
    Data {
        path: PathBuf,
        source: String,
        truncated: bool,
    },
    Error {
        path: PathBuf,
        error: String,
    },
}

/// App-level workspace navigator state. This is intentionally distinct from
/// document `ViewMode::Mindmap` and every `mindmap_*` document field.
#[derive(Clone)]
pub struct FullMindmapState {
    pub selected: Option<WorkspaceNodeId>,
    pub expanded: HashSet<PathBuf>,
    /// Visibility is an explicit user choice and must not change when focus
    /// moves between workspace nodes.
    pub panel_open: bool,
    pub panel_width: f32,
    /// Current step in the Full Mindmap ⌘⌥W width cycle. Kept separate from
    /// document Mindmap state so the two modes cannot affect one another.
    pub panel_step: usize,
    pub panel_drag: Option<(f32, Option<f32>)>,
    pub pending_open: Option<PendingFullMindmapOpen>,
    pub pending_preview_settle: Option<PendingFullMindmapPreviewSettle>,
    pub pending_preview: Option<PendingFullMindmapPreview>,
    pub pending_workspace_load: Option<PendingFullMindmapWorkspaceLoad>,
    pub pending_folder_loads: HashMap<PathBuf, PendingFullMindmapFolderLoad>,
    /// One fixed delayed-reveal wave owns unresolved LowerBound(0) shells at
    /// the currently visible expanded frontier. The wave is canceled whenever
    /// expansion/root/filter/mode ownership changes.
    pub verification_wave: Option<FullMindmapVerificationWave>,
    /// Newly materialized child shells cannot be appended to an active wave's
    /// fixed candidate denominator. They remain hidden here until that wave
    /// drains, then a fresh fixed snapshot verifies them.
    pub verification_followup_pending: bool,
    pub verification_hidden: HashSet<PathBuf>,
    /// Increments whenever the expanded frontier changes. Verification
    /// requests carry the parent generation to reject stale completions.
    pub expansion_generation: u64,
    pub materialized_folders: HashMap<PathBuf, workspace_mindmap::MaterializedFolder>,
    /// A current or previously selected file is selected only after its parent
    /// listing proves that the file is still visible under the accepted filter.
    pub deferred_file_selection: Option<PathBuf>,
    /// Exact canvas focus request for the workspace navigator. It is kept
    /// separate from `selected` so async materialization can be observed and
    /// guarded without letting a stale completion refocus the root/sibling.
    pub focus_request: Option<WorkspaceNodeId>,
    pub preview: FullMindmapPreview,
    /// Accepted preview request identity. It is separate from the local
    /// measurement generation so background workspace requests cannot rebuild
    /// an unchanged preview child tree.
    pub preview_identity: Option<PendingFullMindmapPreview>,
    pub preview_namespace: u64,
    pub preview_shape_ready: bool,
    /// Full Mindmap preview virtualization is completely separate from the
    /// document reader's window, viewport, and measured-height cache. A
    /// selected file therefore cannot move or resize the document underneath
    /// the navigator, and a later document load cannot invalidate this panel.
    pub preview_window: crate::virt::VirtWindow,
    pub preview_viewport: Option<iced::widget::scrollable::Viewport>,
    pub preview_height_cache: crate::virt::HeightCache,
    /// At most one preview measurement operation may be in flight. Scroll
    /// events can arrive faster than widget-tree operations complete; the
    /// guard coalesces those events instead of queueing an unbounded stream.
    pub preview_measurement_pending: bool,
    /// Materialized display range captured when the pending measurement was
    /// dispatched. If scrolling moves to a different range before completion,
    /// the accepted result schedules one follow-up for that current range.
    pub preview_measurement_range: Option<(usize, usize)>,
    pub preview_measurement_generation: Option<u64>,
    /// Monotonic identity for preview selection/parse/layout work. Height
    /// measurements carry it so a late operation cannot alter a new file.
    pub preview_generation: u64,
    /// Cooperative cancellation for stale preview reads/parses. The worker
    /// checks this token between bounded chunks and before expensive parsing.
    pub preview_work_epoch: Arc<AtomicU64>,
    /// Per-navigator parse capacity. Obsolete previews can never occupy a
    /// process-global slot or create an unbounded spawn_blocking backlog;
    /// waiting tasks poll the same epoch and abandon promptly when superseded.
    pub preview_parse_gate: Arc<tokio::sync::Semaphore>,
    /// One shared latest-only settle worker per Full Mindmap instance. The
    /// watch channel replaces requests instead of spawning one uncancellable
    /// sleep per key repeat.
    pub preview_settle_tx: tokio::sync::watch::Sender<Option<PendingFullMindmapPreviewSettle>>,
    pub preview_settle_worker_started: bool,
    /// Remote image loads dispatched by the preview. Values retain the exact
    /// request that owns each loading sentinel: an old completion must not
    /// remove or overwrite a newer preview's same-URL operation.
    pub preview_loading_images: HashMap<String, FullMindmapPreviewAssetIdentity>,
    /// Diagram renders use the same ownership rule because the cache key is
    /// only `(content hash, theme)` and can be reused by A -> B previews.
    pub preview_pending_diagrams: HashMap<(u64, u32), FullMindmapPreviewAssetIdentity>,
    /// Remote/diagram keys used by the accepted preview, including entries
    /// that completed. Legacy document-only completion messages carry no
    /// request identity, so these sets let us keep them from replacing an
    /// active preview-owned result while still reusing loaded cache entries.
    pub preview_asset_images: HashSet<String>,
    /// Image failures are terminal for the current preview identity and
    /// visible-range wave. A later prime may continue with other assets, but
    /// it must not immediately delete `Failed` and retry the same URL forever.
    /// The set is cleared when the identity/range/theme retry boundary changes.
    pub preview_failed_images: HashSet<String>,
    pub preview_asset_diagrams: HashSet<(u64, u32)>,
    pub preview_asset_cursor: usize,
    pub preview_asset_cursor_tag: Option<(u64, u64, (usize, usize))>,
    pub preview_asset_wave_id: u64,
    pub load_error: Option<String>,
    /// Visible workspace graphs are rebuilt only after their source/expansion
    /// changes, not every `view()` frame.
    pub(super) layout_cache: std::cell::RefCell<Option<std::sync::Arc<WorkspaceGraph>>>,
    /// Generation of the visible workspace graph. The shared canvas uses this
    /// to preserve focus when async folder discovery rebuilds the graph without
    /// changing the selected path.
    pub layout_generation: u64,
}

/// Kept only for the stale-worker regression fixture. Runtime preview parsing
/// is always dispatched to the guarded worker, regardless of source length.
#[cfg(test)]
pub(super) const FULL_MINDMAP_PREVIEW_STALE_SOURCE_BYTES: usize = 64 * 1024;

pub(super) const FULL_MINDMAP_PREVIEW_CANCELLED: &str = "full mindmap preview superseded";

pub(super) const FULL_MINDMAP_PREVIEW_READ_CHUNK_BYTES: usize = 64 * 1024;

pub(super) const FULL_MINDMAP_PREVIEW_SETTLE_MS: u64 = 300;

/// Maximum asset descriptors dispatched by one visible-range pass. Further
/// blocks are picked up by a later scroll/remeasure pass, keeping task and
/// cache pressure bounded for asset-heavy Markdown.
pub(super) const FULL_MINDMAP_PREVIEW_ASSET_BATCH: usize = 64;

/// Delayed-reveal verification is deliberately small and fixed. Four bounded
/// filesystem workers keep the UI responsive; the candidate cap bounds both
/// queue memory and total extra scans. Candidates beyond the cap stay visible
/// with their `scan limit reached` lower-bound label.
pub(super) const FULL_MINDMAP_VERIFICATION_CONCURRENCY: usize = 4;

pub(super) const FULL_MINDMAP_VERIFICATION_MAX_CANDIDATES: usize = 256;
