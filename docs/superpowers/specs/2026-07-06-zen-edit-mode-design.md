# Zen edit mode

**Date:** 2026-07-06
**Target:** `/Users/liminchen/Documents/GitHub/mdv`

Replace the existing raw edit mode with a Zen editing experience. `ViewMode::Raw`
remains the app's edit mode internally and over IPC, but the user-facing surface
becomes a low-clutter editor instead of a raw source pane embedded in the normal
reader chrome.

## Goals

- `⌘E` enters a focused editing surface.
- The editor uses a centered writing column with readable width and generous
  margins.
- The Zen editor outer column uses the same max width as rendered viewer mode
  (`READING_MAX`) so switching between read and edit does not shift the document
  column wider or narrower.
- Sidebar is hidden when Zen starts, while the footer and small keyboard help
  button stay visible.
- Core app commands still work while editing: command palette, file finder,
  folder picker, sidebar toggle, theme commands, and font commands.
- The command palette shows all commands in Zen mode.
- Exiting Zen restores the sidebar/footer/search-related UI state that existed
  before entering edit mode.
- Existing save, undo, redo, dirty-state, parse, and IPC semantics are preserved.

## Non-goals

- No separate normal edit mode.
- No new persisted preference for Zen mode.
- No automatic native fullscreen entry.
- No change to PDF view-only behavior.
- No rewrite of the editor widget or markdown highlighter.

## State model

`ViewMode::Raw` becomes Zen edit mode. Add a small restore snapshot on `App`,
for example:

```rust
pub struct ZenRestoreState {
    sidebar_open: bool,
    show_footer: bool,
    search_open: bool,
}
```

These fields are required because Zen changes their visible state on entry. The
snapshot is transient runtime state, not saved in preferences. Overlays are
closed on entry and are not restored on exit, because reopening a stale modal
after an editing session is more surprising than returning to the document.

Entering Zen from Rendered or Mindmap:

1. If a document is open and not a PDF, create `text_editor::Content` from
   `self.source`.
2. Store a restore snapshot if one is not already active.
3. Hide sidebar and search UI for the default Zen surface.
4. Close overlays that would visually cover the editor on entry.
5. Clear edit undo/redo stacks and reset dirty state as today.
6. Set `view_mode = ViewMode::Raw`.

Exiting Zen by `⌘E` or `Esc`:

1. Read the editor buffer.
2. If it differs from `self.source`, assign it to `self.source` and reparse.
3. Clear edit undo/redo stacks as today.
4. Set `view_mode = ViewMode::Rendered`.
5. Restore the saved sidebar, footer, and search-open state; then clear the
   snapshot.

`⌘S` saves without leaving Zen. Save still writes the editor buffer, updates
`self.source`, reparses, primes diagrams, clears dirty state, and reports errors
through the existing error/toast surfaces.

## Default Zen surface

The Raw branch of `App::view` renders a full-height background with a centered
editor column. The editor should keep JetBrains Mono, markdown highlighting,
existing line height, and existing edit actions. The column should use
`READING_MAX`, the same outer max width as rendered viewer mode, with responsive
side padding so narrow windows still work.

Visible by default in Zen:

- Centered editor column.
- Footer/status pill.
- Floating keyboard/help button.

Hidden by default in Zen:

- Sidebar.
- Search bar.
- Any overlay until explicitly opened.
- Mindmap panel and canvas.
- Rendered-reader scroll UI.

The sidebar can be reopened with `⌘B` during Zen. The footer remains visible
while editing because it gives useful document context.

## Keyboard and command behavior

Zen is visually quiet but not command-restricted. While editing, keep:

- Editor-native typing, cursor movement, selection, mouse placement, copy, cut,
  paste, and select-all.
- `⌘S`, `⌘Z`, `⌘Y`, `⌘⇧Z`, `⌘E`, and `Esc`.
- `⌘/` and the floating keyboard/help button.
- Command palette, file finder, folder picker, sidebar toggle, theme commands,
  font commands, and all command-palette entries.

The app-level keyboard subscription should stop treating `editing == true` as a
blanket swallow for app shortcuts. Instead, it should allow the selected command
shortcuts and editor essentials, then avoid non-command rendered-reader
navigation such as `j`, `k`, `Space`, arrow scroll, and fold navigation while
the text editor is focused.

The `text_editor.key_binding` filter should continue preventing non-editor
command chords from also being inserted as text. Standard editor bindings
remain available.

The shortcuts overlay should mention Zen-specific escape hatches: `Esc / ⌘E`
to exit edit mode and `⌘S` to save.

## IPC behavior

IPC mode strings remain unchanged:

- `mode edit` maps to `ViewMode::Raw`, which now opens Zen edit mode.
- `current` reports `"edit"` for `ViewMode::Raw`.
- PDF edit requests remain coerced to rendered view.

If an IPC `mode edit` request changes the app into Raw, it should initialize the
editor content and Zen restore state the same way the keyboard toggle does. It
must not leave `view_mode == Raw` with `editor == None`.

## Error handling

- Save failures keep the user in Zen and surface the existing `save failed: ...`
  error.
- PDF edit attempts still show the current view-only toast and do not enter Zen.
- If Zen exits with no editor content for any reason, fall back to the existing
  source text instead of panicking.

## Verification

- `cargo check`.
- Manual app verification:
  - Open a Markdown document.
  - Press `⌘E`: sidebar hides, footer remains, keyboard/help button remains,
    centered editor appears.
  - Type text, undo/redo, and save with `⌘S`.
  - Press `⌘B`: sidebar can open while still editing.
  - Use `⌘P`, `⌘O`, `⌘⇧P`, `⌘/`, theme shortcut, and font shortcuts while in
    Zen.
  - Confirm command palette lists all commands.
  - Press `Esc` and `⌘E`: return to rendered view and restore pre-Zen sidebar
    and footer state.
  - Open a PDF and confirm `⌘E` still refuses editing.
  - Use IPC `mode edit` and confirm it opens the Zen editor; `current` still
    reports `"edit"`.
