# Full Mindmap Mode

**Date:** 2026-07-10
**Branch:** `feat/full-mindmap-mode`
**Status:** Base implementation committed as `ae0b4a8`; keyboard/performance refinement protected as `82afd5a`; native-acceptance corrections under review (2026-07-15)

## Goal

Add an opt-in, full-window mindmap navigator that lets a user choose a project
folder, browse its folder hierarchy, and open a supported document without
falling back to a list as the primary interaction.

This is a workspace-navigation feature. It is deliberately separate from the
existing document-level `ViewMode::Mindmap`, which continues to map one open
document's headings or structured data. Entering Full Mindmap Mode must not
change, clear, or reinterpret the current document view mode.

## Product decisions

| Question | Proposed decision |
|---|---|
| Feature name | **Full Mindmap Mode** in user-facing copy; `FullMindmapState` in code. |
| Activation | `⌘⇧M`, command palette item, and a welcome-screen action. `⌘M` remains the existing document mindmap toggle. |
| Exit | `Esc` or `⌘⇧M`. Exit restores the unchanged underlying document/navigation surface; Full Mindmap has no persistent toolbar button. |
| No workspace yet | Open a mindmap-style folder browser rooted at the current file's parent or the home directory. |
| Workspace already open | Open the workspace graph immediately, revealing the current file when it belongs to that workspace. |
| File activation | Selecting a file loads a bounded, read-only content preview in the side panel. Press `Enter` to open it; there is no **Open File** button. A successful load exits Full Mindmap Mode to show the document. |
| Folder activation | In the chooser, `Space` descends into a selected folder. In a workspace, `Space` toggles the selected folder/root expanded or collapsed without moving selection, matching document Mindmap. `Enter` chooses a project folder or opens a workspace file. |
| Folder discovery | In folder-selection phase, the selected root/folder receives a background count of supported files. The walk is bounded at 12 levels, 5,000 files, and 10,000 entries; its node label shows the result (`5,000+` at the file cap, or an at-least/incomplete result at the entry budget). Files remain hidden until the folder is chosen as a project. |
| Workspace indexing | Choosing a project performs one background pass for the tree, file finder, and root supported-file count, capped at 12 levels, 5,000 supported files, and 10,000 examined entries. The UI remains interactive and a truncated status node makes partial results explicit. |
| Root parent traversal | `←` at a workspace root indexes the filesystem parent in the background, then selects that new root. It changes workspace navigation scope but never loads or replaces the current document. |
| Panel sizing | `⌘⌥W` cycles the Full Mindmap panel through the existing 1/3, 1/2, and 3/5 window-width steps using Full Mindmap-owned state. |
| Sidebar/footer/search | Full Mindmap Mode visually owns the main window. Sidebar, reader search bar, and footer are hidden without mutating their stored state. |
| Detail panel | A separate workspace preview panel. Files show content; folders show only “Select a file to preview its content.” It never reads or writes `mindmap_panel_*`, which remains document-mindmap state. |
| Dirty document | A file-open attempt uses the existing dirty guard. If blocked, stay in Full Mindmap Mode with the same selection and show the existing unsaved-edits toast. |
| Fallback | Existing `⌘O` folder picker and `⌘P` file finder remain available through keyboard shortcuts. |

## UX flow

### 1. Activation

Add `Message::ToggleFullMindmap` and a command-palette entry:

> Enter Full Mindmap Mode  ⌘⇧M

The shortcut is intentionally distinct from `⌘M`. The existing
`Message::ToggleMindmap` and all `ViewMode::Mindmap` behavior remain unchanged.

Activation is available from Rendered, document Mindmap, and Zen edit modes.
It closes a transient overlay if invoked from the command palette, but does not
destroy editor content, change `view_mode`, change document mindmap selection,
or mutate sidebar/search/footer preferences.

When a workspace is open, the workspace graph appears immediately. When no
workspace is open, Full Mindmap Mode starts in **Choose Project Folder**.

### 2. Choose Project Folder phase

The screen contains:

- No top toolbar or action buttons: the mode is keyboard-first.
- A mindmap canvas whose root is the current directory and whose children are
  its immediate visible subfolders.
- A right panel whose footer documents `Space` to descend and `Enter` to choose
  the project folder.

This phase reuses `Picker` with `PickerMode::Folder` for filesystem enumeration,
hidden-file filtering, sorting, home lookup, parent navigation, and readable
error reporting. It changes only the presentation: picker entries are adapted
to workspace mindmap nodes instead of rendered as rows.

Ordinary folders sort before optional dot-folders, and the chooser root aligns
with the first child rather than the midpoint of an arbitrarily tall sibling
column. Enabling hidden entries is therefore additive: it cannot push familiar
top-level destinations such as `Documents` out of the initial usable graph.

Selecting a root/folder starts a background count of the same supported files
that would become workspace nodes. Only the selected folder is counted and
completed results are cached for the chooser session, so drawing a wide
directory never recursively scans every sibling. The background walk stops at
12 levels, 5,000 supported files, or 10,000 examined entries. The node changes
from `notes` to `notes · 12 files`; it shows `5,000+ files` at the file cap,
an at-least count such as `12+ files` when the entry budget ends early, or
`scan limit reached` when no supported file has been encountered yet. No file
nodes appear in this phase. If the selected root cannot be read, the result is
rendered as `count unavailable`, never `0 files`; the existing picker
error/recovery path remains available.

Pressing `Enter` uses the selected/current folder as the workspace, then
starts a bounded background index. The chooser remains visible with an
“Indexing project…” panel until the matching result is ready, then transitions
to the workspace graph. The selected workspace root retains the supported-file
count produced by that same snapshot; no second recursive count is launched.
It does not open or replace a document. Repeated Enter
on the same target is deduplicated, and stale results from an older selection
are ignored.

An unreadable folder produces an error node and keyboard recovery remains
available through `←`, `⌘O`, and `Esc`. Hidden entries follow the existing
`show_hidden` setting.

### 3. Browse Workspace phase

The workspace root is the graph root. Its visible children come from the
existing `workspace_tree`; the same supported-file filter, hidden-file rule,
sort order, depth cap, and excluded directories therefore apply to both the
sidebar and Full Mindmap Mode.

`workspace_tree`, `workspace_files`, and the root supported-file count are
produced by one bounded filesystem pass instead of independent recursive walks.
Full Mindmap performs this
pass on a blocking worker, never in the Iced update/view path. If the pass hits
its 10,000-entry or 5,000-file budget, the graph includes **More files not
indexed** rather than allocating indefinitely or pretending the result is
complete.

Initial expansion contains the workspace root. If the current file is inside
the workspace, its ancestor folders are also expanded and the file starts
selected. Otherwise the root starts selected.

Folder nodes show a hidden-children indicator when collapsed. File nodes are
leaves. Selecting a file opens a bounded, read-only preview without touching
the current document, editor, or dirty state. Selecting any non-file shows the
single hint: “Select a file to preview its content.”

Opening a file records it as the pending Full Mindmap open and delegates to the
existing `load_file_unless_dirty(path)` path. The navigator exits only when the
matching `FileLoaded(Ok(...))` is accepted. A dirty guard, read error, or stale
async completion leaves the navigator open and preserves its selection.

At the workspace root, `←` changes the workspace to that folder's filesystem
parent, rebuilds the graph, and selects the parent root. The current file,
editor, source, and dirty state stay untouched; old file-open and preview
requests are cancelled before they can affect the new graph.

### 4. Exit and return

`Esc` and `⌘⇧M` clear only `full_mindmap: Option<FullMindmapState>`. Because
the underlying `view_mode`, editor, document mindmap fields, sidebar state,
search state, and footer state were not changed, the user returns exactly
where they started.

After a successful file open, Full Mindmap Mode exits to the normal document
surface. Existing `FileLoaded` behavior reveals that file in the sidebar tree.

## Keyboard behavior

Full Mindmap Mode owns unmodified navigation keys before the sidebar or
document-mindmap handlers. Global commands such as theme, save, hidden files,
`⌘O`, and `⌘P` keep their current behavior.

### Common graph controls

| Key | Behavior |
|---|---|
| `↑` / `↓` | Select previous/next sibling. |
| `←` | Select parent. From a folder-selection root it navigates to the filesystem parent. From a workspace root it rebuilds Full Mindmap around the workspace’s filesystem parent and selects that root. |
| `→` | Expand a collapsed folder; if already expanded, select its first child. |
| `Space` | Folder-selection phase: descend into the selected folder. Workspace phase: toggle the selected folder/root expanded or collapsed and retain selection. |
| `Enter` | Folder-selection phase: use the selected/current folder as the project folder. Workspace phase: open the selected file. |
| `Home` | Select the graph root. Folder-selection phase `⌘Home` returns to the home directory. |
| `Esc` | Exit Full Mindmap Mode. An open overlay still gets first refusal and closes before the mode exits. |
| `⌘⇧M` | Toggle Full Mindmap Mode. |
| `⌘⌥W` | Cycle the Full Mindmap side-panel width (1/3 → 1/2 → 3/5 of the window). |
| `⌘O` / `⌘P` | Open the existing folder picker / file finder as fallback overlays. |

Keyboard selection immediately updates the selection ring. The detail panel
updates directly for cheap filesystem metadata; it does not reuse the document
panel's 75 ms rendered-Markdown debounce.

## Mouse behavior

- Clicking a folder selects it and toggles expansion in workspace phase.
- Clicking a folder selects it in folder-selection phase; keyboard `Space` and
  `Enter` perform descend and project-folder confirmation.
- Clicking a file selects it and starts the read-only preview. It does not
  immediately replace the document; `Enter` is the deliberate activation step.
- Drag-to-pan, wheel zoom, auto-center, hover tooltips, node animation, and
  click-empty-to-deselect reuse current canvas behavior where compatible.

## State ownership

Full Mindmap Mode is app-level navigation state, not a `ViewMode` variant:

```rust
pub struct FullMindmapState {
    pub phase: FullMindmapPhase,
    pub selected: Option<WorkspaceNodeId>,
    pub expanded: HashSet<PathBuf>,
    pub panel_open: bool,
    pub panel_width: f32,
    pub panel_step: usize,
    pub pending_open: Option<PendingFullMindmapOpen>,
    pub pending_preview: Option<PendingFullMindmapPreview>,
    pub pending_folder_count: Option<PendingFullMindmapFolderCount>,
    pub pending_workspace_load: Option<PendingFullMindmapWorkspaceLoad>,
    pub folder_file_counts: HashMap<PathBuf, SupportedFileCount>,
    pub folder_file_count_unavailable: HashSet<PathBuf>,
    pub preview: FullMindmapPreview,
    pub layout_cache: Option<Arc<WorkspaceGraph>>,
}

pub struct PendingFullMindmapOpen {
    pub id: u64,
    pub path: PathBuf,
}

pub enum FullMindmapPhase {
    ChooseFolder(Picker),
    Workspace,
}
```

`App` owns one field:

```rust
pub full_mindmap: Option<FullMindmapState>
pub full_mindmap_request_seq: u64
```

The state intentionally does **not** contain a copy of `ViewMode`, editor text,
sidebar state, or document mindmap state. Those remain owned by their existing
fields and are merely obscured while the full-window navigator is active. The
app-level request sequence survives exit/re-entry, and every workspace switch
cancels the current pending Full Mindmap read, so an older completion cannot
collide with a same-path request in a new navigation session.

Hidden-file changes in workspace phase rebuild through the same background,
request-identified snapshot path. A newer toggle rejects an older completion;
the accepted refresh replaces only workspace snapshot data and preserves any
still-valid selection, expansion, preview request, and file-open request. Scan
ordering considers ordinary entries before optional dot entries and discovers
shallow siblings before descending, so hidden caches cannot consume the budget
before ordinary top-level folders admitted by the directory-entry budget are
represented. A directory with more than 10,000 immediate entries remains an
explicitly truncated edge case rather than triggering an unbounded read.

Activating a file while a hidden-filter refresh is pending supersedes that
refresh with a new request carrying the file as `open_after`. The accepted
snapshot therefore starts the file read, and neither completion order can exit
the navigator before the sidebar snapshot matches the filter. A refresh failure
reverts the filter to the last accepted snapshot value and keeps its error
visible.

If the filter changes in the chooser while an older workspace still backs the
hidden Files sidebar, every normal exit first refreshes that snapshot on the
same worker and exits only after the matching result is accepted. `Esc` and
`⌘⇧M` restore the prior surface; Return to Files additionally opens the Files
sidebar.

If exit reconciliation fails, the exit still completes: the hidden-file filter
reverts to the last accepted snapshot value and the failure is promoted to the
application error surface before Full Mindmap state is removed.

Workspace expansion is separate from `App::expanded` (the sidebar tree) and
from `App::mindmap_collapsed` (the current document). The only synchronization
is on entry: reveal the current file in the Full Mindmap graph; and on explicit
return to the Files sidebar: call the existing `reveal_current_file` behavior.

## Workspace node model

Add `src/workspace_mindmap.rs` with domain-specific nodes and pure adapters:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WorkspaceNodeId {
    Root(PathBuf),
    Folder(PathBuf),
    File(PathBuf),
    Status(WorkspaceStatusId),
}

pub enum WorkspaceNodeKind {
    Root,
    Folder,
    File,
    Empty,
    Error(String),
    Truncated,
}

pub struct WorkspaceNode {
    pub id: WorkspaceNodeId,
    pub path: Option<PathBuf>,
    pub kind: WorkspaceNodeKind,
    pub label: String,
    pub full_label: String,
    pub children: Vec<usize>,
    pub has_hidden_children: bool,
    pub depth: usize,
}
```

Identity is path-based, not a synthetic `BlockId`. `Status` identities are
scoped to the directory and status kind so empty/error placeholders are stable
across a redraw. Filesystem nodes never enter the document
`mindmap_collapsed`, `mindmap_selected`, or document panel maps.

The module exposes two pure builders:

1. `from_picker(&Picker, &file_counts, &unavailable_count_paths,
   pending_count_path) -> WorkspaceGraph` for immediate folder browsing without
   filesystem reads during rendering.
2. `from_tree(&tree::Node, &HashSet<PathBuf>, truncated) -> WorkspaceGraph` for
   the open workspace, respecting Full Mindmap expansion and exposing a stable
   truncated status node when the index budget was reached.

It also exposes pure parent/sibling/child navigation helpers and node lookup by
`WorkspaceNodeId`. `app.rs` owns actions and async tasks; the module never opens
a file or changes the workspace.

## Canvas boundary

The current canvas is coupled to document `BlockId` in `MNode`,
`MindmapState`, animation maps, selection, and callbacks. Full Mindmap Mode must
not mint fake `BlockId`s or share the document ID sets.

Refactor the reusable renderer to be generic over its domain ID:

```rust
pub struct CanvasNode<Id> { /* current visual/layout fields + Id */ }
pub type MNode = CanvasNode<BlockId>;

pub struct MindmapState<Id> { /* animation and selection keyed by Id */ }
pub struct MindmapProgram<'a, Id, Message> { /* generic callbacks */ }
```

`Id` requires `Clone + Eq + Hash`; this permits path-bearing
`WorkspaceNodeId` without lossy hashing. Existing document builders continue
to return `MNode`, and data mindmaps continue to use real document `BlockId`s.
The workspace adapter returns `CanvasNode<WorkspaceNodeId>`.

The completed `WorkspaceGraph` and its node vector are `Arc`-backed. Cached
graphs are shared directly with the canvas, so ordinary view frames do not
clone every label, path, and metadata map.

Keep the current document canvas behavior behind a document adapter/callback
mapping: branch click toggles collapse, leaf click selects the preview panel,
background click deselects. The workspace mapping is separate: folder click
selects and toggles expansion, file click selects, background click closes the
workspace detail panel.

This generic refactor is the only shared-renderer change. It must land with
regression tests proving existing document tree construction, collapse, and
navigation semantics are unchanged before workspace wiring is added.

## Rendering and panel separation

In `App::view`, Full Mindmap Mode is checked before error/welcome/document
reader rendering, similar to the current workspace-level vault page. Its page
contains its own canvas, resize handle, and workspace preview panel; it has no
top bar.

Do not call `mindmap_layout()` or `mindmap_panel_view()` for the workspace.
Do not reuse these document fields:

- `mindmap_collapsed`
- `mindmap_selected`
- `mindmap_panel_shown`
- `mindmap_panel_settle_gen`
- `mindmap_data_panel`

The workspace panel may reuse visual style constants and the resize-handle
widget, but its open/selected/content state belongs to `FullMindmapState`.
Initial implementation can use the current panel width preference while keeping
open/drag state separate; persisting a second width is not required.

## Existing-system integration

### `src/app.rs`

- Add `full_mindmap: Option<FullMindmapState>` and Full Mindmap messages.
- Route Full Mindmap keys before sidebar/document-mindmap navigation keys.
- Render the navigator as a full-window workspace page.
- Reuse the shared workspace setup, `load_file_unless_dirty`, `FileLoaded`,
  `ToggleHidden`, `OpenFolderPicker`, `OpenFileFinder`, and
  `reveal_current_file` rather than duplicating their side effects.
- Route file activations from the standard picker/file-finder fallback through
  the same Full Mindmap pending-open helper so a successful fallback open also
  exits the navigator, while a blocked or failed open leaves it intact.
- Invalidate/rebuild the workspace layout when workspace, picker directory,
  hidden visibility, expansion, or selection source changes.

### `src/mindmap.rs`

- Generalize canvas node/program/state identity while preserving `MNode` as
  the document alias.
- Keep document AST/data builders and layout rules behaviorally unchanged.
- Do not add filesystem reads or workspace actions.

### `src/tree.rs`

- Remains the authoritative workspace tree and now builds a
  `WorkspaceSnapshot { root, files, truncated }` in one pass.
- Applies hard depth, supported-file, and examined-entry budgets before
  allocation can grow without bound.
- Keeps filtering, sorting, pruning, and file-finder depth behavior aligned
  with the previous tree and `walk_markdown` paths.

### `src/picker.rs`

- Reuse `PickerMode::Folder` for Choose Project Folder.
- Keep the list overlay as the standard fallback.
- Do not change `PickerMode::OpenAny` behavior.
- Provide a bounded, allocation-light supported-file counter for the selected
  chooser folder. Its filters and 12-level depth cap match `tree::build`; a
  10,000-entry budget prevents unbounded work in wide trees. It runs in a
  background task owned by `app.rs`, never while building a canvas graph. A
  root read error remains unavailable rather than becoming a false `0` count.

## Fallback behavior

Full Mindmap Mode is additive. The following remain intact:

- `⌘O`: current folder/file picker overlay.
- `⌘P`: current workspace file finder; if no workspace, current picker fallback.
- Sidebar Files tree: unchanged and restored on exit.
- Document `⌘M`: unchanged.
- Zen `⌘E`: unchanged.

If Full Mindmap construction returns no visible supported files, show the root
and an **Empty workspace** status node; do not silently open the list tree. The
keyboard fallback paths remain `⌘O`, `⌘P`, and `Esc`; Full Mindmap has no panel
action buttons. If a path disappears between scan and activation, preserve the
navigator, clear `pending_open`, surface the existing load error, and rebuild
the graph on the next workspace refresh.

## Dirty-file and async safety

The open sequence is intentionally two-stage:

1. Resolve the selected `WorkspaceNodeId` to a file path.
2. Pass it to one `begin_full_mindmap_open(path)` helper, also used by fallback
   picker/file-finder activation while the navigator is active. The helper
   calls the same dirty guard used by tree, links, vault, IPC, and drag/drop.
3. Only when not dirty, store `pending_open = Some(path.clone())` and start the
   existing asynchronous load.
4. On a matching accepted `FileLoaded(Ok((path, source)))`, finish the normal
   file-load state updates, clear `pending_open`, then exit Full Mindmap Mode.
5. On error, mismatch, or a late completion blocked by the second dirty check,
   clear only the matching pending request and keep the navigator open.

Changing or choosing a workspace does not replace the current document, so it
is allowed while dirty. Opening a file is always guarded. No Full Mindmap path
may directly assign `file`, `source`, `saved_source`, `dirty`, or `editor`.

## Testing

### Pure workspace-model tests (`workspace_mindmap.rs`)

1. Folder picker graph contains root plus sorted visible child folders and no
   file nodes.
2. Workspace graph preserves folder/file kinds and parent/child indices.
3. Initial root-only expansion hides descendants and marks folders as having
   hidden children.
4. Expanding and collapsing a folder changes only its visible descendants.
5. Path-based IDs remain stable when an unrelated sibling is inserted or
   hidden visibility changes.
6. Empty, unreadable, and depth-truncated inputs produce explicit status nodes
   without panicking.
7. Parent/sibling/first-child navigation matches document mindmap arrow
   semantics.
8. A bounded workspace snapshot produces the tree and file index together,
   stops at the examined-entry budget, and surfaces unreadable roots.
9. A truncated workspace graph contains an explicit status node.

### App state-transition tests (`app.rs`)

1. **Folder selection:** entering without a workspace creates
   `ChooseFolder(PickerMode::Folder)`; choosing a folder uses the shared
   workspace setup and transitions to workspace phase.
2. **Folder expansion/collapse:** Workspace `Space` toggles the selected
   folder/root without moving selection; Full Mindmap expansion remains
   independent from sidebar `expanded` and document `mindmap_collapsed`.
3. **File opening:** activating a supported file schedules the existing load;
   a matching successful `FileLoaded` updates the document and exits Full
   Mindmap Mode.
4. **Dirty-file protection:** dirty activation schedules no load, keeps current
   file/editor/source, stays in Full Mindmap Mode, and shows the unsaved toast.
5. **Late dirty protection:** a pending Full Mindmap load that returns after
   the document becomes dirty is rejected and the navigator stays open.
6. **Returning to normal navigation:** exit preserves underlying `ViewMode`,
   document mindmap state, editor, sidebar preference, search, and footer.
7. `⌘M` still changes only document `ViewMode::Mindmap`; `⌘⇧M` changes only
   `full_mindmap`.
8. Hidden-file toggle adds/removes only dot entries, preserves ordinary
   picker/workspace nodes, and performs workspace refreshes in the background
   with stale-result rejection and valid navigation retention. The folder
   chooser resets to its visible root and invalidates cached file counts.
9. A selected chooser folder receives a bounded supported-file count without
   adding file nodes; the 10,000-entry budget and 5,000-file cap produce an
   explicit at-least/incomplete label, root read failures show `count
   unavailable` rather than a false empty count, and stale count completions
   are ignored. After project selection the workspace root retains the count
   from the accepted snapshot without a second scan.
10. `←` at the workspace root moves to the parent workspace while preserving a
    dirty editor/current document and rejecting late file-open completions.
11. `⌘⌥W` cycles only the Full Mindmap panel state and does not alter the
    document Mindmap panel width.
12. Project selection and root-parent navigation do not mutate the workspace
    until their matching background index completes; stale completions are
    ignored and pending file opens are cancelled immediately.
13. Folder-picker file fallback waits for its parent workspace index before
    opening, and cached graphs reuse the same graph/node allocations.

### Shared canvas regression tests (`mindmap.rs`)

1. Existing document AST builder returns the same labels, hierarchy, and
   `BlockId`s after genericization.
2. Data-mindmap `MNode` construction still compiles and preserves IDs.
3. Generic layout produces the same coordinates for equivalent document and
   workspace trees.
4. Document branch/leaf callback mapping remains toggle/select respectively.

### Verification after implementation

The approved design is implemented as follows:

- `src/workspace_mindmap.rs` owns path-based filesystem graph construction and
  never reuses document `BlockId`s.
- `src/mindmap.rs` now shares a generic canvas/layout adapter while its default
  type remains the existing document `BlockId` path.
- `src/app.rs` owns Full Mindmap phase, selection, expansion, panel, preview,
  and request-identity state; a successful file open exits back to normal
  reading.
- `src/tree.rs` performs one bounded pass for the workspace tree and file
  index. Full Mindmap runs it off the UI thread; `src/workspace_mindmap.rs`
  shares cached graphs/nodes by `Arc` and renders an explicit truncation node.
- Focused model and app-state tests cover the folder picker, expansion,
  successful and dirty file opens, late completion, stale completion, and exit
  state restoration, including workspace-switch cancellation and same-path
  exit/re-entry request identity. The keyboard-first refinement also covers
  Space descent, Enter project confirmation, direct workspace-root parent
  traversal, bounded chooser file counts, read-only previews, stale async
  completions, async bounded workspace indexing, zero-copy graph cache reuse,
  and independent `⌘⌥W` panel sizing.

Verification recorded in `PROJECT_STATUS.md`: the current library suite,
integration suites, focused Full Mindmap tests, and diff validation passed.
The all-target `cargo test` run is presently blocked only by temporary-disk
exhaustion while linking the `pdf_smoke` example. Native desktop automation
timed out locally, so visual interaction needs a manual pass before
release-oriented work.

- The base implementation's prior all-target test and release-build checks
  passed. For the current keyboard-first refinement, the library and
  integration suites plus the focused Full Mindmap tests passed; the all-target
  run could not complete because temporary build storage filled while linking
  `pdf_smoke`.
- `rustfmt --edition 2021 --check src/picker.rs src/workspace_mindmap.rs` and
  `git diff --check` — passed.
- Repository-wide `cargo fmt --check` and strict Clippy remain blocked by
  broad repository formatting/lint debt, so they are not clean pass gates for
  this focused feature change.
- A manual native interaction pass remains outstanding because local desktop
  automation timed out.

## Implemented sequence

1. Genericized the shared canvas identity and retained document regression
   coverage.
2. Added `workspace_mindmap` model/builders and model tests.
3. Added `FullMindmapState`, messages, entry/exit, and keyboard routing.
4. Added Choose Project Folder rendering and workspace selection.
5. Added workspace graph, detail panel, folder expansion, and file activation.
6. Added dirty/async state-transition tests and fallback actions.
7. Ran automated verification and recorded exact evidence in
   `PROJECT_STATUS.md`; only native manual interaction remains outstanding.
