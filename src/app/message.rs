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
    /// Reveal this path (or its nearest existing folder) in Finder.
    RevealPath(PathBuf),
    /// Close the error card and show what is beneath it.
    DismissError,
    /// A welcome-page row whose command acts on an open document: say why it
    /// does nothing yet.
    WelcomeNeedsFile,
    CopyFilePath,
    VaultQueryChanged(String),
    /// The vault page's clear button: empty the query and focus the field.
    VaultQueryClear,
    /// Run the search for the current query (Enter when the query has changed).
    VaultRunSearch,
    /// Enter in the vault page: search if the query was edited, else open the hit.
    VaultEnter,
    VaultSearchDone(crate::vault_search::VaultResults),
    VaultMove(isize),
    /// Pointer motion over a vault hit (visible-match index); gated like
    /// [`Message::OverlayHover`].
    VaultHover(usize),
    /// Pointer position over an overlay list or the vault results. A changed
    /// position is real motion; row hovers caused by scrolling under a still
    /// pointer arrive without one.
    ListPointerMoved(iced::Point),
    VaultOpenSelected,
    VaultOpenHit(usize),
    /// Open a vault hit by its file and line: a context menu's Open, which
    /// must not follow a result list that a newer search replaced.
    VaultOpenHitAt {
        path: PathBuf,
        line: u32,
    },
    VaultToggleFile(PathBuf),
    VaultClose,
    /// Apply a measured absolute scroll offset to the vault results page.
    VaultScrollTo(f32),
    ToggleShortcuts,
    /// A shortcut-sheet row click: close the sheet, then run the row's
    /// action.
    ShortcutSheetRun(Box<Message>),
    /// Activate the slot addressed by its zero-based rail index.
    /// Pin (true) or close (false) the Quick Slots popover, the toolbar's
    /// pointer entry to the rail.
    QuickSlotsPopover(bool),
    QuickSlotActivate(usize),
    /// Assign the current reading context to the slot. A slot that holds a
    /// different file asks for confirmation first.
    QuickSlotAssign(usize),
    /// Put this workspace file in the first free Quick Slot; with every slot
    /// full, ask to replace the last one.
    QuickSlotAssignPath(PathBuf),
    /// A right press on the chip of Quick Slot `index`: open its context
    /// menu.
    QuickSlotContextMenu(usize),
    /// Replace the slot named by the open confirmation.
    QuickSlotReplaceConfirm,
    /// Close the replace confirmation and keep the slot.
    QuickSlotReplaceCancel,
    QuickSlotClear(usize),
    QuickSlotClearAll,
    QuickSlotUndo,
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
    /// Go Back or Forward in navigation history (⌘[ ⌘], the mouse side
    /// buttons, the toolbar chevrons, the palette).
    HistoryGo(HistoryDir),
    /// Internal handoff after entering/exiting Full Mindmap for a slot.
    QuickSlotRestorePending,
    /// Identity-bearing file read for a non-Full-Mindmap slot activation.
    QuickSlotFileLoaded {
        index: usize,
        slot: crate::quick_slots::QuickSlot,
        result: Result<(PathBuf, String), String>,
    },
    CloseOverlay,
    /// A right press anywhere in the window, in window coordinates. A
    /// context menu that a widget opens on the same press appears here.
    PointerRightPress(iced::Point),
    /// Pointer motion over a context menu item, with the pointer's position
    /// in the menu; it highlights the item only after real pointer motion
    /// (a changed position, see [`Message::ListPointerMoved`]).
    ContextMenuHover(usize, iced::Point),
    /// Up or Down in the open context menu: the next enabled item.
    ContextMenuMove(isize),
    /// Enter in the open context menu: choose the highlighted item.
    ContextMenuActivate,
    /// A click on a context menu item: close the menu, then run the item.
    ContextMenuChoose(usize),
    /// Close the context menu without choosing (Esc, a press outside it).
    ContextMenuClose,
    PickerNavigate(PathBuf),
    PickerParent,
    PickerHome,
    /// A left press while the folder picker watches presses (it is open, or
    /// a row click just closed it); `double` when the press completes a
    /// double-click.
    PickerPointerPress {
        double: bool,
    },
    /// Picker chose a file: open it AND select its parent as workspace.
    PickerOpenFile(PathBuf),
    OverlayQueryChanged(String),
    /// An overlay query field's clear button: empty the query and focus
    /// the field.
    OverlayQueryClear,
    OverlayMove(isize),
    OverlayConfirm,
    /// Pointer click on a palette or theme row: select it, then confirm
    /// exactly as Enter does.
    OverlayPick(usize),
    /// Pointer motion over an overlay list row. Moves the keyboard selection
    /// only after real pointer motion (see [`Message::ListPointerMoved`]).
    OverlayHover(usize),
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
    /// The pointer entered a link: its destination, and the widget that
    /// holds it (two widgets can hold the same destination). Sent once
    /// per link.
    LinkHovered(String, u64),
    /// The pointer left that link of that widget.
    LinkLeft(String, u64),
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
    /// A pointer click on a file-tree row: moves the tree cursor there, then
    /// opens the file or toggles the folder.
    TreeRowClicked {
        path: PathBuf,
        is_dir: bool,
    },
    /// A pointer click on an outline row: moves the outline cursor there,
    /// then scrolls to the heading's line.
    OutlineRowClicked {
        index: usize,
        line: u32,
    },
    TreeMove(isize),
    TreeActivate,
    OutlineMove(isize),
    OutlineActivate,
    /// The pointer entered (true) or left (false) the outline list.
    OutlineHovered(bool),
    ScrollToLine(u32),
    TreeToggleAtCursor,
    CopyTreePath,
    /// The file tree's context menu: a right press on a row (`Some((path,
    /// is_dir))`) or on the empty tree space, the workspace root (`None`).
    TreeContextMenu(Option<(PathBuf, bool)>),
    /// Copy this text (a path) to the clipboard.
    CopyPath(String),
    /// Create "Untitled.md" (or the next free "Untitled N.md") in this
    /// folder, then rename it in place.
    TreeNewFile(PathBuf),
    /// Create "Untitled Folder" (or the next free one) in this folder, then
    /// rename it in place.
    TreeNewFolder(PathBuf),
    /// Start renaming this file tree row in place.
    TreeRenameStart(PathBuf),
    /// The name typed in the rename field.
    TreeRenameInput(String),
    /// Enter in the rename field: rename the file or folder.
    TreeRenameCommit,
    /// Esc, or a press outside the rename field: keep the old name.
    TreeRenameCancel,
    /// Move this file or folder to the system Trash.
    TreeTrash(PathBuf),
    ScrollBy(f32),
    /// A reader selection drag is past the top (negative) or bottom edge
    /// of the viewport by this many pixels per tick, or back inside.
    ReaderAutoScroll(Option<f32>),
    /// One step of [`Message::ReaderAutoScroll`].
    ReaderAutoScrollTick,
    ScrollToTop,
    ScrollToBottom,
    ToggleSearch,
    QueryChanged(String),
    /// Debounce timer for in-document search; carries the keystroke generation.
    SearchDebounced(u64),
    NextMatch,
    PrevMatch,
    /// A find-bar chevron click: step to the next (`true`) or previous
    /// match, then give the search input its focus back.
    FindStepClicked(bool),
    TreeScrolled(iced::widget::scrollable::Viewport),
    OutlineScrolled(iced::widget::scrollable::Viewport),
    OverlayScrolled(iced::widget::scrollable::Viewport),
    VaultScrolled(iced::widget::scrollable::Viewport),
    BodyScrolled(iced::widget::scrollable::Viewport),
    TableScrolled,
    ScrollerTick,
    /// The pointer entered (`true`) or left the reader scrollbar's strip
    /// at the right edge; the scrollbar widens while it is there.
    ReaderScrollbarHover(bool),
    CopyCode(String),
    /// A block's copy button: copy its source, then show a check on the
    /// button for a moment instead of a toast. The key
    /// ([`crate::render::copy_key`]) tells the check which block it is on,
    /// so blocks with the same source do not all show it.
    CopyBlockSource(String, usize),
    /// The copy button's check timer, by copy number; a later copy keeps
    /// its own check.
    CopyFlashEnd(u64),
    SidebarDragStart,
    SidebarDragMove(f32),
    SidebarDragEnd,
    /// A double click on the sidebar divider: back to the default width.
    SidebarResetWidth,
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
    /// Pinch or ⌘+scroll over the reader: this many font steps (a fraction
    /// of a step adds up). The font changes on the next
    /// [`Message::ReaderZoomFlush`].
    ReaderZoomBy(f32),
    /// Apply the whole font steps the reader zoom gesture collected.
    ReaderZoomFlush,
    /// The reader zoom gesture paused after flush `seq`: write the font
    /// size to prefs once, unless a later flush came since.
    ReaderZoomSave(u64),
    /// Settings → Line spacing.
    SetLineSpacing(crate::theme::LineSpacing),
    /// Settings → Toolbar: shown on hover or always.
    SetToolbarMode(crate::prefs::ToolbarMode),
    ToggleFooter,
    /// A click on the status footer pill shows the next statistic.
    FooterCycleStat,
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
    /// A completed two-finger swipe from the macOS bridge: Back or Forward.
    NativeSwipe(HistoryDir),
    /// A mind map zoom button: Fit, −, 100%, or +. It drives the canvas on
    /// screen (the document mind map or Full Mindmap).
    MindmapZoom(crate::mindmap::ZoomCommand),
    /// An image viewer zoom button or key: Fit, −, 100%, or +.
    ImageZoomCommand(crate::mindmap::ZoomCommand),
    /// The viewer wants this many raster pixels per inline-raster pixel
    /// for the diagram it shows.
    ImageZoomDetail(u32),
    /// The debounce of sharper-raster request `id` ended.
    ImageZoomRasterDue(u64),
    /// Sharper raster request `id` finished.
    ImageZoomRasterReady(u64, Result<iced::widget::image::Handle, String>),
    MindmapToggleNode(crate::ast::BlockId),
    MindmapSelectLeaf(crate::ast::BlockId),
    /// A double click on a document mind map node: read that heading in
    /// the Reader, or the document from its top for the root.
    MindmapActivateNode(crate::ast::BlockId),
    MindmapDeselect,
    MindmapNavigate(MindmapDir),
    MindmapPanelSettle(u64),
    MindmapToggleSelected,
    MindmapPanelDragStart(f32),
    MindmapPanelDragMove(f32),
    MindmapPanelDragEnd,
    /// A double click on the panel divider: back to the default width.
    MindmapPanelResetWidth,
    ToggleMindmapAutocenter,
    /// Settings → Scroll pans the mind map: scroll pans instead of zooming.
    ToggleMindmapScrollPans,
    ToggleMindmapPanel,
    MindmapCyclePanelWidth,
    ToggleFullMindmap,
    /// The pointer entered (true) or left (false) the toolbar's top band.
    ToolbarHover(bool),
    /// The hide delay after the pointer left the toolbar band ran out; the
    /// number is the hover generation that started it.
    ToolbarHideAfter(u64),
    /// The pointer entered (true) or left (false) the sidebar peek: the band
    /// at the left window edge, or the peeking sidebar itself.
    SidebarPeek(bool),
    /// The hide delay after the pointer left the sidebar peek ran out; the
    /// number is the peek generation that started it.
    SidebarPeekHideAfter(u64),
    /// A toolbar segment: show this view.
    ToolbarView(ToolbarView),
    /// A press on the toolbar's blank space or the sidebar header: move the
    /// window, as a press on a titlebar does.
    WindowDrag,
    /// A double click there: zoom the window, as on a titlebar.
    WindowTitlebarDoubleClick,
    ExitFullMindmap,
    FullMindmapToggleNode(WorkspaceNodeId),
    FullMindmapSelectNode(WorkspaceNodeId),
    /// A pointer click on a node: select it and show the panel.
    FullMindmapPointerSelect(WorkspaceNodeId),
    /// A double click on a node: select it, then do what Enter does.
    FullMindmapActivateNode(WorkspaceNodeId),
    FullMindmapDeselect,
    FullMindmapNavigate(MindmapDir),
    FullMindmapDiveWorkspace(WorkspaceNodeId),
    FullMindmapActivate,
    FullMindmapToggleSelected,
    /// Reveal the next page of direct children for a folder's "More…" node.
    FullMindmapShowMore(PathBuf),
    FullMindmapSelectRoot,
    FullMindmapSetRoot(PathBuf),
    FullMindmapWorkspaceParent,
    FullMindmapReturnToFiles,
    FullMindmapTogglePanel,
    FullMindmapCyclePanelWidth,
    FullMindmapPanelDragStart(f32),
    FullMindmapPanelDragMove(f32),
    FullMindmapPanelDragEnd,
    FullMindmapPanelResetWidth,
    FullMindmapFileLoaded {
        request: PendingFullMindmapOpen,
        result: Result<(PathBuf, String), String>,
    },
    /// A link clicked in the Full Mindmap preview. `base` is the previewed
    /// file, which a relative link resolves from.
    FullMindmapPreviewLink {
        base: std::path::PathBuf,
        url: String,
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
    /// Files are dragged over the window (true) or left it (false).
    DropHover(bool),
    /// A file or folder was dropped on the window.
    FileDropped(PathBuf),
    /// The red close button (or the platform's close) asked to close.
    WindowCloseRequested(iced::window::Id),
    /// ⌘Q (the app menu's Quit) asked to quit; macOS only.
    QuitRequested,
    /// Close confirmation: save, then close if the save succeeds.
    CloseConfirmSave,
    /// Close confirmation: close without saving.
    CloseConfirmDiscard,
    /// Close confirmation: keep the window open.
    CloseConfirmCancel,
    WindowUnfocused(iced::window::Id),
    RefreshWindowMode(iced::window::Id),
    /// The window opened at this inner size (the toolbar centers the file
    /// name on it before any resize).
    WindowOpened(iced::window::Id, iced::Size),
    RefreshWindowModeSettled(iced::window::Id),
    WindowModeChanged(iced::window::Mode),
    /// Palette command: capture the window to a timestamped PNG on the Desktop.
    TakeScreenshot,
    /// Async result of a `Cmd::Screenshot` / `TakeScreenshot` capture. Encodes
    /// to PNG, writes the file at `pending_screenshot.0`, and either replies via
    /// the stashed IPC sender or shows a toast.
    ScreenshotCaptured(iced::window::Screenshot),
    HintSelection,
    /// ⌘A in the reader: mark the whole document selected.
    SelectAllDocument,
    /// ⌘C while the whole document is selected: copy its plain text.
    CopyDocument,
    /// Esc, a click, or a new drag ends the whole-document selection.
    ClearSelectAll,
    /// A reader element under a right press (a heading, a code block, an
    /// image, a diagram); the [`Message::ReaderContextMenu`] of the same
    /// press uses it.
    ReaderTarget(ReaderTarget),
    /// A right press in the reader: open the reader's context menu.
    ReaderContextMenu(ReaderHit),
    FoldChordStart,
    FoldChordCancel,
    FoldToLevel(u8),
    ToggleFold(crate::ast::BlockId),
    HeadingHoverEnter(crate::ast::BlockId),
    HeadingHoverExit(crate::ast::BlockId),
    EditorAction(iced::widget::text_editor::Action),
    /// Focus the Zen editor (a click in the margin beside it).
    FocusEditor,
    /// A right press in the Zen editor: open its context menu.
    EditorContextMenu,
    /// The editor menu's Cut: copy the selection, then delete it.
    EditorMenuCut,
    /// The editor menu's Copy: copy the selection.
    EditorMenuCopy,
    /// The editor menu's Paste: read the clipboard.
    EditorMenuPaste,
    /// The clipboard text for the editor menu's Paste.
    EditorPasted(Option<String>),
    /// A right press on the document or data mind map: on a node (`Some`)
    /// or on empty canvas (`None`). Opens its context menu.
    MindmapContextMenu(Option<crate::ast::BlockId>),
    /// Show the node's children, each branch below it folded.
    MindmapCollapseBelow(crate::ast::BlockId),
    /// Unfold the node and every branch below it.
    MindmapExpandBelow(crate::ast::BlockId),
    /// Copy a data map node's value as JSON, YAML, or TOML text.
    MindmapCopyDataValue(crate::ast::BlockId),
    /// A right press on Full Mindmap: on a node (`Some`) or on empty canvas
    /// (`None`). Opens its context menu.
    FullMindmapContextMenu(Option<WorkspaceNodeId>),
    /// A right press on outline row `index`: open its context menu.
    OutlineContextMenu(usize),
    /// A right press on vault search hit `index` (into `vault_results`).
    VaultHitContextMenu(usize),
    /// A right press on a file finder row.
    FileFinderContextMenu(PathBuf),
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
