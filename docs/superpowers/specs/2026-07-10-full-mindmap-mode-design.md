# Full Mindmap Mode

**Date:** 2026-07-10
**Branch:** `feat/full-mindmap-mode`
**Status:** Unified folder-rooted explorer and recursive count correction submitted for manual acceptance (2026-07-15)

## Goal

Add an opt-in, full-window, folder-rooted mindmap navigator that lets a user
browse any filesystem folder and open a supported document without falling
back to a list as the primary interaction.

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
| No workspace yet | Adopt the current file's parent as the app workspace, or Home when no file is open, through the same bounded background workspace loader used by every root change. |
| Workspace already open | Open the workspace graph immediately, revealing the current file when it belongs to that workspace. |
| File activation | Selecting a file loads a bounded, read-only content preview in the side panel. Press `Enter` to open it; there is no **Open File** button. A successful load exits Full Mindmap Mode to show the document. |
| Folder activation | `Space` always toggles the selected folder/root without moving selection. `Right` expands a collapsed folder and selects its first child in one step. `Enter` makes a folder the new root or opens a file. |
| Folder discovery | The one bounded background workspace pass records recursive counts for every discovered folder. Collapsed exact folders show `N files`; interrupted folders show `N+ files`; zero before interruption shows `scan limit reached`; unreadable folders show `count unavailable`. Expanded folders use plain labels. |
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

When a workspace is open, the workspace graph appears immediately. When none
is open, Full Mindmap starts the same bounded background load for the current
file's parent, or Home, and adopts the matching accepted result as the app
workspace. There is no separate user-visible chooser mode.

### 2. Unified folder-rooted explorer

The screen has one workspace graph and one keyboard contract in every entry
scenario. Root changes, including first entry and `Enter` on a folder, run the
existing bounded scan off the UI thread. The accepted request atomically
replaces the app workspace snapshot; stale completions are ignored.

That scan remains capped at 12 tree levels, 5,000 supported files, and 10,000
examined entries. While building the tree and file-finder list it also rolls
each supported file into its ancestor folders. It never starts per-folder
walks. A folder whose own directory cannot be read retains an unavailable
count; an ancestor or subtree cut short by the scan retains the number already
seen as a lower bound.

Hidden entries remain additive and use the same request-identified refresh
ordering. File previews remain read-only and independent of the current dirty
document. `⌘O` remains the standard folder-picker fallback, but it is not a
second Full Mindmap presentation.

### 3. Browse workspace

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

Collapsed folder nodes show their recursive count and a hidden-children
indicator. Expanded folders use plain labels. File nodes are leaves. Selecting
a file opens a bounded, read-only preview without touching the current
document, editor, or dirty state. Selecting any non-file shows the single hint:
“Select a file to preview its content.”

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
| `←` | Select the graph parent. At the root, load the filesystem parent as the new workspace root in the background. |
| `→` | Expand a collapsed folder and select its first child in one step; if already expanded, select its first child. |
| `Space` | Toggle the selected folder/root expanded or collapsed and retain selection. |
| `Enter` | Open the selected file, or load the selected folder as the new workspace root. |
| `Home` | Select the graph root. |
| `Esc` | Exit Full Mindmap Mode. An open overlay still gets first refusal and closes before the mode exits. |
| `⌘⇧M` | Toggle Full Mindmap Mode. |
| `⌘⌥W` | Cycle the Full Mindmap side-panel width (1/3 → 1/2 → 3/5 of the window). |
| `⌘O` / `⌘P` | Open the existing folder picker / file finder as fallback overlays. |

Keyboard selection immediately updates the selection ring. The detail panel
updates directly for cheap filesystem metadata; it does not reuse the document
panel's 75 ms rendered-Markdown debounce.

## Mouse behavior

- Clicking a folder selects it and toggles expansion.
- Clicking a file selects it and starts the read-only preview. It does not
  immediately replace the document; `Enter` is the deliberate activation step.
- Drag-to-pan, wheel zoom, auto-center, hover tooltips, node animation, and
  click-empty-to-deselect reuse current canvas behavior where compatible.

## State ownership

Full Mindmap Mode is app-level navigation state, not a `ViewMode` variant:

```rust
pub struct FullMindmapState {
    pub selected: Option<WorkspaceNodeId>,
    pub expanded: HashSet<PathBuf>,
    pub panel_open: bool,
    pub panel_width: f32,
    pub panel_step: usize,
    pub pending_open: Option<PendingFullMindmapOpen>,
    pub pending_preview: Option<PendingFullMindmapPreview>,
    pub pending_workspace_load: Option<PendingFullMindmapWorkspaceLoad>,
    pub preview: FullMindmapPreview,
    pub layout_cache: Option<Arc<WorkspaceGraph>>,
}

pub struct PendingFullMindmapOpen {
    pub id: u64,
    pub path: PathBuf,
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

Hidden-file changes rebuild through the same background,
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

If the filter changes while an older workspace snapshot still backs the hidden
Files sidebar, every normal exit first refreshes that snapshot on the same
worker and exits only after the matching result is accepted. `Esc` and `⌘⇧M`
restore the prior surface; Return to Files additionally opens the Files sidebar.

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

The module exposes a pure adapter from the retained folder skeleton plus Full
Mindmap-owned accepted/pending branch-local materialization into a
`WorkspaceGraph`. A bounded background expansion may replace an interrupted
folder shell with shallow immediate folder children carrying recursive counts,
plus immediate supported files. It respects expansion, exposes files only
beneath their expanded parent, prunes determinably exact-empty folders, retains
lower-bound/unreadable unknowns, and renders stable loading/error/local-
truncation status children while a folder load is pending or incomplete. The
global bounded-scan truncation status remains distinct.

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
- Invalidate/rebuild the workspace layout when workspace, hidden visibility,
  expansion, or selection source changes.

### `src/mindmap.rs`

- Generalize canvas node/program/state identity while preserving `MNode` as
  the document alias.
- Keep document AST/data builders and layout rules behaviorally unchanged.
- Do not add filesystem reads or workspace actions.

### `src/tree.rs`

- Remains the authoritative workspace tree and builds a
  `WorkspaceSnapshot { root, files, truncated }` in one pass; every directory
  node also retains its exact, lower-bound, or unavailable recursive count.
- Applies hard depth, supported-file, and examined-entry budgets before
  allocation can grow without bound.
- Keeps filtering, sorting, pruning, and file-finder depth behavior aligned
  with the previous tree and `walk_markdown` paths.

### `src/picker.rs`

- Keep the list overlay as the standard `⌘O` fallback.
- Do not change `PickerMode::OpenAny` behavior.
- Full Mindmap does not use picker-owned snapshots or per-folder count walks.

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

1. Workspace graph preserves folder/file kinds and parent/child indices.
2. Collapsed exact, lower-bound, zero-before-limit, and unavailable folders use
   the specified labels; expanded folders use plain labels.
3. Initial root-only expansion hides descendants and marks folders as having
   hidden children.
4. Expanding and collapsing a folder changes only its visible descendants.
5. Path-based IDs remain stable when an unrelated sibling is inserted or
   hidden visibility changes.
6. Empty, unreadable, and depth-truncated inputs produce explicit metadata/status nodes
   without panicking.
7. Parent/sibling/first-child navigation matches document mindmap arrow
   semantics.
8. A bounded workspace snapshot produces the tree and file index together,
   stops at the examined-entry budget, and surfaces unreadable roots.
9. A truncated workspace graph contains an explicit status node.

### App state-transition tests (`app.rs`)

1. **Unified entry:** entering without a workspace starts one background load
   for the current file's parent or Home; entering with a workspace reuses its
   accepted snapshot immediately.
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
   workspace nodes, and performs workspace refreshes in the background with
   stale-result rejection and valid navigation retention.
9. Per-folder counts come only from the accepted snapshot; exact and interrupted
   subtrees retain the correct count semantics without a second scan.
10. `←` at the workspace root moves to the parent workspace while preserving a
    dirty editor/current document and rejecting late file-open completions.
11. `⌘⌥W` cycles only the Full Mindmap panel state and does not alter the
    document Mindmap panel width.
12. Project selection and root-parent navigation do not mutate the workspace
    until their matching background index completes; stale completions are
    ignored and pending file opens are cancelled immediately.
13. Standard picker file fallback waits for its parent workspace index before
    opening, and cached graphs reuse the same graph/node allocations.

### Shared canvas regression tests (`mindmap.rs`)

1. Existing document AST builder returns the same labels, hierarchy, and
   `BlockId`s after genericization.
2. Data-mindmap `MNode` construction still compiles and preserves IDs.
3. Generic layout produces the same coordinates for equivalent document and
   workspace trees.
4. Document branch/leaf callback mapping remains toggle/select respectively.

### 5. Delayed reveal and verification feedback (2026-07-16)

The bounded workspace snapshot can contain a folder shell whose recursive
count is `LowerBound(0)`: the scan stopped before proving whether the folder
contains a supported file. Full Mindmap must not flash such a shell and then
remove it (the unsupported-only `Shopee Backroom` case is the motivating
example). A fixed delayed-reveal wave therefore snapshots only unresolved
zero-lower-bound folders currently visible below expanded parents, hides those
folders immediately, and verifies them on blocking workers before revealing
them.

The wave owns a fixed candidate denominator and a small worker window (four
requests in flight, at most 256 candidates). Candidates beyond that cap remain
visible with the truthful `scan limit reached` label; no candidate is silently
dropped. A result is accepted only when its request id, wave id, workspace root,
hidden-file filter, Full Mindmap ownership, parent path, and parent expansion
generation still match. Collapse, root/filter changes, and exit clear the wave
ownership; late futures cannot reveal stale nodes. Explicitly expanding a shell
keeps the selected parent visible with its existing `Loading files…` status and
starts a new fixed wave for the newly visible frontier.

Accepted `Exact(0)` folders are pruned and never rendered. Exact positive and
positive lower-bound counts use the normal count labels. A scan-limit result
keeps a lower-bound/status node, while an unreadable or otherwise unverifiable
result is shown as `count unavailable` with an explicit error status.

While a wave is active, Full Mindmap shows one persistent neutral toast with a
determinate bar and `checked/total` plus remaining text. The counter advances
only for accepted results, so it is monotonic and the denominator never grows.
Completion or cancellation removes this progress toast. It is rendered below
the existing attention/error toast layer, so blocked-action and failure copy
retain their own priority and expiry timing; no ordinary toast is overwritten.

Verification results retain only the recursive count/status (`Verified`), not
the scanned child or file listing. A collapsed positive shell therefore gets a
truthful count without retaining a second branch snapshot; explicit expansion
evicts that count-only fact and owns the separate child/file materialization.
When an expanded-folder load introduces new `LowerBound(0)` child shells after
the current wave was frozen, those children are hidden in a deferred follow-up
set and a new fixed wave starts after the active denominator drains. Collapsing
one branch cancels and re-snapshots the remaining expanded frontier in the same
update, so unresolved siblings never flash into the graph.

### Verification after implementation

The approved design is implemented as follows:

- `src/workspace_mindmap.rs` owns path-based filesystem graph construction and
  never reuses document `BlockId`s.
- `src/mindmap.rs` now shares a generic canvas/layout adapter while its default
  type remains the existing document `BlockId` path.
- `src/app.rs` owns Full Mindmap selection, expansion, panel, preview,
  branch-local materialization, and request-identity state; a successful
  file open exits back to normal reading. Collapse evicts the branch's accepted
  and pending materialization, so re-expansion always owns a new request.
- `src/tree.rs` performs one bounded pass for the folder skeleton, recursive
  counts, and flat file-finder index. Expanded folders request a separate
  bounded background pass; only its shallow immediate folder nodes and
  immediate supported files are retained. This lets an interrupted shell
  discover useful children without re-rooting, while each newly expanded child
  owns another bounded request. Exact-empty subtrees are pruned even when an
  unrelated branch truncated; lower-bound and unreadable unknowns remain.
  `src/workspace_mindmap.rs` shares cached graphs/nodes by `Arc`, keeps file
  nodes out of collapsed branches, and renders explicit loading/error/
  truncation nodes.
- The same bounded pass retains a second lightweight path index through the
  historical tree depth for the ordinary Files sidebar. Paths are grouped by
  parent and sibling-sorted once while the snapshot is built; each subsequent
  `tree::flatten_with_files` traverses only visible folders and performs
  parent-local lookups. It transiently splices files beneath visible/expanded
  folder rows, so the sidebar keeps root/nested files, ordering, keyboard
  activation, and hidden refresh behavior without permanent file `Node`s or
  per-frame full-index regrouping. Cmd+P and vault search continue to use their
  historical shallower file-index depth.
- Focused model and app-state tests cover unified entry, expansion,
  successful and dirty file opens, late completion, stale completion, and exit
  state restoration, including workspace-switch cancellation and same-path
  exit/re-entry request identity. The keyboard-first refinement also covers
  Space folding, Enter folder re-rooting, direct workspace-root parent
  traversal, recursive snapshot counts, read-only previews, stale async
  completions, async bounded workspace indexing, lazy root/current-file reveal,
  collapse/re-expand eviction, hidden/root/re-entry stale rejection, zero-copy
  graph cache reuse, and independent `⌘⌥W` panel sizing.

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
4. Added folder-rooted workspace rendering and selection.
5. Added workspace graph, detail panel, folder expansion, and file activation.
6. Added dirty/async state-transition tests and fallback actions.
7. Ran automated verification and recorded exact evidence in
   `PROJECT_STATUS.md`; only native manual interaction remains outstanding.
8. Replaced the visible chooser/workspace split with one workspace explorer,
   auto-adopted current-file parent/Home entry, and added per-folder recursive
   count metadata to the existing single bounded scan.
