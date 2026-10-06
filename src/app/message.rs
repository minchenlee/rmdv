//! The application message enum.

use super::*;

#[derive(Debug, Clone)]
pub enum Message {
    Open(PathBuf),
    /// File-finder activation. Full Mindmap Mode intercepts this narrow path
    /// so fallback opens retain its async dirty-file safety contract.
    OpenFileFinderPath(PathBuf),
    OpenWorkspace(PathBuf),
    OpenFolderPicker,
    OpenFileFinder,
    OpenCommandPalette,
    OpenThemePicker,
    OpenVaultSearch,
    Refresh,
    RevealFileInFinder,
    CopyFilePath,
    VaultQueryChanged(String),
    /// Run the search for the current query (Enter when the query has changed).
    VaultRunSearch,
    /// Enter in the vault page: search if the query was edited, else open the hit.
    VaultEnter,
    VaultSearchDone(crate::vault_search::VaultResults),
    VaultMove(isize),
    VaultOpenSelected,
    VaultOpenHit(usize),
    VaultToggleFile(PathBuf),
    VaultClose,
    /// Apply a measured absolute scroll offset to the vault results page.
    VaultScrollTo(f32),
    ToggleShortcuts,
    /// Activate the slot addressed by its zero-based rail index.
    QuickSlotActivate(usize),
    /// Assign/overwrite the slot with the current reading context.
    QuickSlotAssign(usize),
    QuickSlotClear(usize),
    QuickSlotClearAll,
    QuickSlotUndo,
    QuickSlotNew,
    QuickSlotClose,
    QuickSlotCloseWindow,
    QuickSlotCycle(i8),
    /// Primary modifier press/release controls the transient rail.
    QuickSlotsModifier(bool),
    /// Delayed reveal for the transient rail; the generation ignores a
    /// modifier release or a newer press that superseded this timer.
    QuickSlotsModifierReveal(u64),
    /// Debounced persistence generation for ordinary scrolling checkpoints.
    QuickSlotsPersist(u64),
    /// Internal handoff after entering/exiting Full Mindmap for a slot.
    QuickSlotRestorePending,
    /// Identity-bearing file read for a non-Full-Mindmap slot activation.
    QuickSlotFileLoaded {
        index: usize,
        slot: crate::quick_slots::QuickSlot,
        result: Result<(PathBuf, String), String>,
    },
    CloseOverlay,
    PickerNavigate(PathBuf),
    PickerParent,
    PickerHome,
    PickerSelectFolderHere,
    /// Picker chose a file: open it AND select its parent as workspace.
    PickerOpenFile(PathBuf),
    OverlayQueryChanged(String),
    OverlayMove(isize),
    OverlayConfirm,
    OverlayDescend,
    FileLoaded(Result<(PathBuf, String), String>),
    FileLoadCompleted {
        generation: u64,
        result: Result<(PathBuf, String), String>,
    },
    RefreshFileLoaded {
        request: PendingRefreshFile,
        result: Result<(PathBuf, String), String>,
    },
    RefreshWorkspaceLoaded {
        request: PendingRefreshWorkspace,
        result: Result<(PathBuf, tree::WorkspaceSnapshot), String>,
    },
    /// Dedicated scan of an expanded sidebar folder the workspace-wide budget
    /// stopped at. `epoch` drops results from a replaced workspace snapshot.
    SidebarFolderScanned {
        epoch: u64,
        folder: PathBuf,
        result: Result<(PathBuf, tree::WorkspaceSnapshot), String>,
    },
    FileChanged(PathBuf),
    /// Identity-bearing completion for a file watcher reload. A watcher read
    /// must not share the generic navigation completion path: an explicit
    /// slot/manual open may own the reader by the time it finishes.
    FileChangedLoaded {
        request: PendingWatcherReload,
        result: Result<(PathBuf, String), String>,
    },
    CheckClipboardCopy(String),
    ClipboardCopyChecked {
        expected: String,
        actual: Option<String>,
    },
    OpenLink(String),
    ToggleTheme,
    SetTheme(ThemePreset),
    SetCustomTheme(String),
    ReloadThemes,
    ThemeFilesChanged,
    OpenThemesDir,
    ToggleSidebar,
    SetSidebarTab(SidebarTab),
    /// Toggle visibility of dot-prefixed entries in tree + picker.
    ToggleHidden,
    TreeToggle(PathBuf),
    TreeMove(isize),
    TreeActivate,
    OutlineMove(isize),
    OutlineActivate,
    ScrollToLine(u32),
    TreeToggleAtCursor,
    CopyTreePath,
    ScrollBy(f32),
    ScrollToTop,
    ScrollToBottom,
    ToggleSearch,
    QueryChanged(String),
    /// Debounce timer for in-document search; carries the keystroke generation.
    SearchDebounced(u64),
    NextMatch,
    PrevMatch,
    TreeScrolled(iced::widget::scrollable::Viewport),
    OutlineScrolled(iced::widget::scrollable::Viewport),
    OverlayScrolled(iced::widget::scrollable::Viewport),
    VaultScrolled(iced::widget::scrollable::Viewport),
    BodyScrolled(iced::widget::scrollable::Viewport),
    TableScrolled,
    ScrollerTick,
    CopyCode(String),
    SidebarDragStart,
    SidebarDragMove(f32),
    SidebarDragEnd,
    /// Deferred body scroll restore. Emitted after a toggle that reinitialises
    /// the body scrollable. `RestoreBodySnap(y)` uses relative offset [0..1];
    /// `RestoreBodyScroll(y)` uses absolute px offset.
    RestoreBodySnap(f32),
    RestoreBodyScroll(f32),
    /// Real laid-out heights for windowed blocks, harvested by a widget
    /// operation after a virt-window rebuild. Feeds `HeightCache` so prefix
    /// estimates converge to real geometry. The `f32` is the body offset at
    /// dispatch time: scroll-anchoring compensation is only valid if the
    /// viewport hasn't moved since (a nav jump in between would make the
    /// compensation fight the landing).
    BlockHeightsMeasured(Vec<(crate::ast::BlockId, f32)>, f32),
    ToastExpire(u64),
    /// Install the CLI symlink from the packaged app.
    InstallCli,
    /// Result of the asynchronous CLI installation attempt.
    CliInstallFinished(Result<(), String>),
    /// An update was downloaded + verified and is ready to install.
    UpdateAvailable(crate::update::ReadyUpdate),
    /// User confirmed install: self-replace + relaunch.
    InstallUpdate,
    /// User dismissed the update banner.
    DismissUpdate,
    ImageFetched(String, Result<Vec<u8>, String>),
    SvgRasterized(String, Result<(Vec<u8>, u32, u32), String>),
    OpenImageZoom(String),
    ToggleViewMode,
    FontSizeUp,
    FontSizeDown,
    FontSizeReset,
    ToggleFooter,
    ToggleSoftSyntax,
    /// Window glass: off → sidebar → whole window (macOS).
    CycleGlass,
    /// Window glass tint: 60 → 70 → 80 → 90 %.
    CycleGlassOpacity,
    /// Set the window glass mode directly (Settings page).
    SetGlass(crate::macos_vibrancy::Glass),
    /// Set the window glass tint directly (Settings page slider).
    SetGlassOpacity(f32),
    /// Settings page (⌘,): open, close, or toggle it.
    OpenSettings,
    CloseSettings,
    ToggleSettings,
    /// Move the Settings row cursor by ±1.
    SettingsMove(i32),
    /// Put the Settings row cursor on a row (pointer hover).
    SettingsCursor(usize),
    /// Space / Enter on the Settings cursor row.
    SettingsActivate,
    /// ← / → on the Settings cursor row.
    SettingsStep(i32),
    SettingsScrollTo(f32),
    /// Relaunch rmdv so a launch-time setting (window glass) takes effect.
    RestartApp,
    /// Show `prefs.json` in Finder.
    RevealPrefsFile,
    ToggleMindmap,
    /// A macOS magnification delta from the native event bridge. The shared
    /// canvas consumes the cumulative stream on its next redraw.
    MindmapNativePinch(f32),
    MindmapToggleNode(crate::ast::BlockId),
    MindmapSelectLeaf(crate::ast::BlockId),
    MindmapDeselect,
    MindmapNavigate(MindmapDir),
    MindmapPanelSettle(u64),
    MindmapToggleSelected,
    MindmapPanelDragStart(f32),
    MindmapPanelDragMove(f32),
    MindmapPanelDragEnd,
    ToggleMindmapAutocenter,
    ToggleMindmapPanel,
    MindmapCyclePanelWidth,
    ToggleFullMindmap,
    ExitFullMindmap,
    FullMindmapToggleNode(WorkspaceNodeId),
    FullMindmapSelectNode(WorkspaceNodeId),
    FullMindmapDeselect,
    FullMindmapNavigate(MindmapDir),
    FullMindmapDiveWorkspace(WorkspaceNodeId),
    FullMindmapActivate,
    FullMindmapToggleSelected,
    FullMindmapSelectRoot,
    FullMindmapSetRoot(PathBuf),
    FullMindmapWorkspaceParent,
    FullMindmapReturnToFiles,
    FullMindmapTogglePanel,
    FullMindmapCyclePanelWidth,
    FullMindmapPanelDragStart(f32),
    FullMindmapPanelDragMove(f32),
    FullMindmapPanelDragEnd,
    FullMindmapFileLoaded {
        request: PendingFullMindmapOpen,
        result: Result<(PathBuf, String), String>,
    },
    FullMindmapPreviewSettle {
        request: PendingFullMindmapPreviewSettle,
    },
    FullMindmapPreviewLoaded {
        request: PendingFullMindmapPreview,
        result: Result<(PathBuf, String), String>,
    },
    FullMindmapPreviewParsed {
        request: PendingFullMindmapPreview,
        result: Result<FullMindmapPreview, String>,
    },
    /// Scroll and measured-height feedback for the Full Mindmap preview panel.
    /// These messages are deliberately not `BodyScrolled`/
    /// `BlockHeightsMeasured`: the panel owns a different viewport and cache.
    FullMindmapPreviewScrolled {
        namespace: u64,
        path: Option<PathBuf>,
        identity: u64,
        viewport: iced::widget::scrollable::Viewport,
    },
    FullMindmapPreviewBlockHeightsMeasured {
        path: PathBuf,
        namespace: u64,
        identity: u64,
        generation: u64,
        measured: Vec<(crate::ast::BlockId, f32)>,
        at_offset: f32,
    },
    FullMindmapPreviewImageFetched {
        identity: FullMindmapPreviewIdentity,
        range: (usize, usize),
        wave: u64,
        url: String,
        result: Result<Vec<u8>, String>,
    },
    FullMindmapPreviewAssetWave {
        identity: FullMindmapPreviewIdentity,
        range: (usize, usize),
        wave: u64,
    },
    FullMindmapWorkspaceLoaded {
        request: PendingFullMindmapWorkspaceLoad,
        result: Result<(PathBuf, tree::WorkspaceSnapshot), String>,
    },
    FullMindmapFolderLoaded {
        request: PendingFullMindmapFolderLoad,
        result: Result<(PathBuf, tree::ExpandedFolderSnapshot), String>,
    },
    FullMindmapVerificationLoaded {
        request: PendingFullMindmapVerification,
        result: Result<(PathBuf, tree::ExpandedFolderSnapshot), String>,
    },
    WindowResized(iced::window::Id, iced::Size),
    WindowUnfocused(iced::window::Id),
    RefreshWindowMode(iced::window::Id),
    RefreshWindowModeSettled(iced::window::Id),
    WindowModeChanged(iced::window::Mode),
    /// Palette command: capture the window to a timestamped PNG on the Desktop.
    TakeScreenshot,
    /// Async result of a `Cmd::Screenshot` / `TakeScreenshot` capture. Encodes
    /// to PNG, writes the file at `pending_screenshot.0`, and either replies via
    /// the stashed IPC sender or shows a toast.
    ScreenshotCaptured(iced::window::Screenshot),
    HintSelection,
    FoldChordStart,
    FoldChordCancel,
    FoldToLevel(u8),
    ToggleFold(crate::ast::BlockId),
    HeadingHoverEnter(crate::ast::BlockId),
    HeadingHoverExit(crate::ast::BlockId),
    EditorAction(iced::widget::text_editor::Action),
    SaveFile,
    FileSaved {
        result: Result<(), String>,
        saved_source: String,
    },
    EditorUndo,
    EditorRedo,
    /// Zoom a rendered diagram into the image-viewer overlay. Looks up the
    /// `Ready` SVG bytes by content-hash and opens [`Overlay::ImageZoom`].
    DiagramZoom(u64),
    /// Copy a diagram's raw source to the system clipboard.
    CopyDiagramSource(u64),
    /// Result of an async diagram render dispatched by `prime_diagram_cache`.
    /// `theme_id` is the snapshot at dispatch time — stale results are dropped.
    DiagramRendered {
        hash: u64,
        theme_id: u32,
        result: Result<crate::diagram::RenderOutput, String>,
    },
    FullMindmapPreviewDiagramRendered {
        identity: FullMindmapPreviewIdentity,
        range: (usize, usize),
        wave: u64,
        hash: u64,
        theme_id: u32,
        result: Result<crate::diagram::RenderOutput, String>,
    },
    Noop,
    /// Toggle the `auto_focus_on_nav` preference and persist it.
    ToggleAutoFocusOnNav,
    /// IPC request from the listener subscription. The sender is wrapped in
    /// `Arc<Mutex<Option<…>>>` so the variant is `Clone` (Iced 0.14 requires
    /// `Message: Clone`). The handler takes the sender out of the mutex once
    /// to reply.
    Ipc(
        crate::ipc::Request,
        std::sync::Arc<
            std::sync::Mutex<Option<futures::channel::oneshot::Sender<crate::ipc::Response>>>,
        >,
    ),
}
