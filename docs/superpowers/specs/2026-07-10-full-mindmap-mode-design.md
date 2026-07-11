# Full Mindmap Mode

**Date:** 2026-07-10
**Branch:** `fix/cjk-emphasis-issue-6`
**Status:** Implemented in the working tree — uncommitted (2026-07-10)

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
| Exit | `Esc`, `⌘⇧M`, or an always-visible **Exit Mindmap Navigator** button. Exit restores the unchanged underlying document/navigation surface. |
| No workspace yet | Open a mindmap-style folder browser rooted at the current file's parent or the home directory. |
| Workspace already open | Open the workspace graph immediately, revealing the current file when it belongs to that workspace. |
| File activation | Select a file to show its action panel; press `Enter` or click **Open File**. A successful load exits Full Mindmap Mode to show the document. |
| Folder activation | Select and expand/collapse folders in the workspace graph. In folder-selection phase, `Enter` descends and the panel offers **Use as Project Folder**. |
| Sidebar/footer/search | Full Mindmap Mode visually owns the main window. Sidebar, reader search bar, and footer are hidden without mutating their stored state. |
| Detail panel | A separate workspace detail/action panel. It never reads or writes `mindmap_panel_*`, which remains document-mindmap state. |
| Dirty document | A file-open attempt uses the existing dirty guard. If blocked, stay in Full Mindmap Mode with the same selection and show the existing unsaved-edits toast. |
| Fallback | Existing `⌘O` folder picker and `⌘P` file finder remain available. Errors/empty states include explicit **Use Standard Picker** or **Return to Files Sidebar** actions. |

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

- A fixed top bar with the phase title, current path, **Home**, **Parent**,
  **Use Current Folder**, **Standard Picker**, and **Exit** actions.
- A mindmap canvas whose root is the current directory and whose children are
  its immediate visible subfolders.
- A right detail panel for the selected folder, with **Open Folder** (descend)
  and **Use as Project Folder** actions.

This phase reuses `Picker` with `PickerMode::Folder` for filesystem enumeration,
hidden-file filtering, sorting, home lookup, parent navigation, and readable
error reporting. It changes only the presentation: picker entries are adapted
to workspace mindmap nodes instead of rendered as rows.

Choosing **Use as Project Folder** dispatches the existing
`Message::OpenWorkspace(path)`. The mode then transitions to the workspace
graph; it does not open or replace a document.

An unreadable folder produces an error node and panel copy with **Parent** and
**Standard Picker** recovery. An empty folder still permits **Use Current
Folder**. Hidden entries follow the existing `show_hidden` setting.

### 3. Browse Workspace phase

The workspace root is the graph root. Its visible children come from the
existing `workspace_tree`; the same supported-file filter, hidden-file rule,
sort order, depth cap, and excluded directories therefore apply to both the
sidebar and Full Mindmap Mode.

Initial expansion contains the workspace root. If the current file is inside
the workspace, its ancestor folders are also expanded and the file starts
selected. Otherwise the root starts selected.

Folder nodes show a hidden-children indicator when collapsed. File nodes are
leaves. Selecting a node opens the workspace detail panel:

- Folder: relative path, visible child counts, and **Expand**/**Collapse**.
- File: relative path, detected supported type, and **Open File**.
- Root: workspace path, visible file count, **Change Project Folder**, and
  **Return to Files Sidebar**.

Opening a file records it as the pending Full Mindmap open and delegates to the
existing `load_file_unless_dirty(path)` path. The navigator exits only when the
matching `FileLoaded(Ok(...))` is accepted. A dirty guard, read error, or stale
async completion leaves the navigator open and preserves its selection.

### 4. Exit and return

`Esc`, `⌘⇧M`, and **Exit Mindmap Navigator** all clear only
`full_mindmap: Option<FullMindmapState>`. Because the underlying `view_mode`,
editor, document mindmap fields, sidebar state, search state, and footer state
were not changed, the user returns exactly where they started.

After a successful file open, Full Mindmap Mode exits to the normal document
surface. Existing `FileLoaded` behavior reveals that file in the sidebar tree.
If the user explicitly chooses **Return to Files Sidebar**, Full Mindmap Mode
exits, opens the sidebar, selects the Files tab, and calls the existing reveal
logic for the current file.

## Keyboard behavior

Full Mindmap Mode owns unmodified navigation keys before the sidebar or
document-mindmap handlers. Global commands such as theme, save, hidden files,
`⌘O`, and `⌘P` keep their current behavior.

### Common graph controls

| Key | Behavior |
|---|---|
| `↑` / `↓` | Select previous/next sibling. |
| `←` | Select parent. In folder-selection phase, from the root it navigates to the filesystem parent. |
| `→` | Expand a collapsed folder; if already expanded, select its first child. |
| `Space` | Toggle the selected folder. No action on a file. |
| `Enter` | Workspace phase: toggle a folder or open a file. Folder-selection phase: descend into the selected folder. |
| `⌘Enter` | Folder-selection phase only: use the selected folder as the project folder. |
| `Home` | Select the graph root. Folder-selection phase `⌘Home` returns to the home directory. |
| `Esc` | Exit Full Mindmap Mode. An open overlay still gets first refusal and closes before the mode exits. |
| `⌘⇧M` | Toggle Full Mindmap Mode. |
| `⌘O` / `⌘P` | Open the existing folder picker / file finder as fallback overlays. |

Keyboard selection immediately updates the selection ring. The detail panel
updates directly for cheap filesystem metadata; it does not reuse the document
panel's 75 ms rendered-Markdown debounce.

## Mouse behavior

- Clicking a folder selects it and toggles expansion in workspace phase.
- Clicking a folder selects it in folder-selection phase; the panel provides
  explicit **Open Folder** and **Use as Project Folder** actions.
- Clicking a file selects it and opens the detail panel. It does not
  immediately replace the document; **Open File** or `Enter` is the deliberate
  activation step.
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
    pub pending_open: Option<PendingFullMindmapOpen>,
    pub layout_cache: Option<WorkspaceMindmapLayout>,
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

1. `from_picker(&Picker) -> WorkspaceGraph` for immediate folder browsing.
2. `from_tree(&tree::Node, &HashSet<PathBuf>) -> WorkspaceGraph` for the open
   workspace, respecting Full Mindmap expansion.

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
contains its own top bar, canvas, resize handle, and workspace detail panel.

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
- Reuse `OpenWorkspace`, `load_file_unless_dirty`, `FileLoaded`,
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

- Remains the authoritative built workspace tree.
- Add only pure lookup/count helpers if the workspace adapter needs them.
- Keep its filtering, sorting, pruning, and depth behavior unchanged.

### `src/picker.rs`

- Reuse `PickerMode::Folder` for Choose Project Folder.
- Keep the list overlay as the standard fallback.
- Do not change `PickerMode::OpenAny` behavior.

## Fallback behavior

Full Mindmap Mode is additive. The following remain intact:

- `⌘O`: current folder/file picker overlay.
- `⌘P`: current workspace file finder; if no workspace, current picker fallback.
- Sidebar Files tree: unchanged and restored on exit.
- Document `⌘M`: unchanged.
- Zen `⌘E`: unchanged.

If Full Mindmap construction returns no visible supported files, show the root
and an **Empty workspace** status node; do not silently open the list tree. The
panel offers **Change Project Folder**, **Use Standard Picker**, and **Return to
Files Sidebar**. If a path disappears between scan and activation, preserve the
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

### App state-transition tests (`app.rs`)

1. **Folder selection:** entering without a workspace creates
   `ChooseFolder(PickerMode::Folder)`; choosing a folder dispatches
   `OpenWorkspace` and transitions to workspace phase.
2. **Folder expansion/collapse:** Full Mindmap expansion is independent from
   sidebar `expanded` and document `mindmap_collapsed`.
3. **File opening:** activating a supported file schedules the existing load;
   a matching successful `FileLoaded` updates the document and exits Full
   Mindmap Mode.
4. **Dirty-file protection:** dirty activation schedules no load, keeps current
   file/editor/source, stays in Full Mindmap Mode, and shows the unsaved toast.
5. **Late dirty protection:** a pending Full Mindmap load that returns after
   the document becomes dirty is rejected and the navigator stays open.
6. **Returning to normal navigation:** explicit exit preserves underlying
   `ViewMode`, document mindmap state, editor, sidebar preference, search, and
   footer; **Return to Files Sidebar** opens Files and reveals the current file.
7. `⌘M` still changes only document `ViewMode::Mindmap`; `⌘⇧M` changes only
   `full_mindmap`.
8. Hidden-file toggle rebuilds workspace/picker graphs while preserving any
   still-valid path selection and expansion.

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
- `src/app.rs` owns Full Mindmap phase, selection, expansion, panel, and
  request-identity state; a successful file open exits back to normal reading.
- Focused model and app-state tests cover the folder picker, expansion,
  successful and dirty file opens, late completion, stale completion, and exit
  state restoration, including workspace-switch cancellation and same-path
  exit/re-entry request identity.

Verification recorded in `PROJECT_STATUS.md`: targeted Rust formatting,
full test suite, release build, and diff validation passed. Native desktop
automation timed out locally, so visual interaction needs a manual pass before
release-oriented work.

- `cargo test --target-dir /private/tmp/mdv-full-mindmap-target -q` — passed.
- `cargo build --release --target-dir /private/tmp/mdv-full-mindmap-target -q`
  — passed.
- `rustfmt --edition 2021 --check src/mindmap.rs src/workspace_mindmap.rs` and
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
