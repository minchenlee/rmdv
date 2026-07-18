# Zen Edit Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the existing raw edit mode with a centered Zen editing surface while preserving save, undo/redo, IPC, footer, help, and core app commands.

**Architecture:** Keep `ViewMode::Raw` as the edit-mode discriminator and add transient Zen restore state to `App`. Route all edit entry/exit paths through helper methods so keyboard toggles and IPC mode changes initialize the editor consistently. Render Raw as a centered editor column while leaving footer/help layers available.

**Tech Stack:** Rust, Iced 0.14, existing `iced::widget::text_editor`, existing in-file `src/app.rs` tests.

## Global Constraints

- `ViewMode::Raw` is the only edit mode; do not add a separate normal edit mode.
- No new persisted preference.
- No automatic native fullscreen.
- PDF files remain view-only.
- Command palette remains full in Zen mode.
- Footer and floating keyboard/help button remain visible in Zen mode.
- Existing unrelated dirty files must not be reverted.

---

## File Structure

- Modify `src/app.rs`: add Zen restore state, entry/exit helpers, Raw layout, keyboard handling, IPC edit initialization, shortcuts overlay copy, and focused unit tests.
- Create `docs/superpowers/plans/2026-07-06-zen-edit-mode.md`: implementation plan.

### Task 1: Zen State Helpers

**Files:**
- Modify: `src/app.rs`

**Interfaces:**
- Produces: `ZenRestoreState`, `App::enter_zen_edit_mode() -> Task<Message>`, `App::exit_zen_edit_mode() -> Task<Message>`, `App::sync_editor_to_source() -> bool`.
- Consumes: existing `App` fields `source`, `editor`, `view_mode`, `sidebar_open`, `show_footer`, `search_open`, `overlay`, `edit_history`, `edit_redo`, and `dirty`.

- [x] **Step 1: Add failing tests for restore behavior**

Add tests inside the existing `#[cfg(test)] mod tests` in `src/app.rs`:

```rust
#[test]
fn zen_entry_hides_sidebar_search_and_keeps_footer() {
    let mut app = App::default();
    app.file = Some(std::path::PathBuf::from("note.md"));
    app.source = "# Title\n\nBody".into();
    app.sidebar_open = true;
    app.show_footer = true;
    app.search_open = true;
    app.overlay = Overlay::Command;

    let _ = app.enter_zen_edit_mode();

    assert_eq!(app.view_mode, ViewMode::Raw);
    assert!(app.editor.is_some());
    assert!(!app.sidebar_open);
    assert!(app.show_footer);
    assert!(!app.search_open);
    assert_eq!(app.overlay, Overlay::None);
    assert!(app.zen_restore.is_some());
}

#[test]
fn zen_exit_restores_saved_chrome_state() {
    let mut app = App::default();
    app.file = Some(std::path::PathBuf::from("note.md"));
    app.source = "before".into();
    app.sidebar_open = true;
    app.show_footer = false;
    app.search_open = true;

    let _ = app.enter_zen_edit_mode();
    let _ = app.exit_zen_edit_mode();

    assert_eq!(app.view_mode, ViewMode::Rendered);
    assert!(app.sidebar_open);
    assert!(!app.show_footer);
    assert!(app.search_open);
    assert!(app.zen_restore.is_none());
}
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test zen_`

Expected: compile failure naming missing `zen_restore`, `enter_zen_edit_mode`, and `exit_zen_edit_mode`.

- [x] **Step 3: Implement state and helpers**

Add near `PendingNav`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZenRestoreState {
    pub sidebar_open: bool,
    pub show_footer: bool,
    pub search_open: bool,
}
```

Add `pub zen_restore: Option<ZenRestoreState>,` to `App`, initialize it to `None` in `Default`, and add helpers in `impl App`:

```rust
fn sync_editor_to_source(&mut self) -> bool {
    let Some(ed) = self.editor.as_ref() else {
        return false;
    };
    let text = ed.text();
    if text == self.source {
        return false;
    }
    self.source = text;
    self.reparse_source();
    true
}

fn enter_zen_edit_mode(&mut self) -> Task<Message> {
    if self.file.is_none() {
        return Task::none();
    }
    if is_pdf_path(self.file.as_deref()) {
        return self.show_toast("PDFs are view-only".into());
    }
    if self.zen_restore.is_none() {
        self.zen_restore = Some(ZenRestoreState {
            sidebar_open: self.sidebar_open,
            show_footer: self.show_footer,
            search_open: self.search_open,
        });
    }
    self.sidebar_open = false;
    self.search_open = false;
    self.overlay = Overlay::None;
    self.mindmap_panel_drag = None;
    self.editor = Some(iced::widget::text_editor::Content::with_text(
        self.source.as_str(),
    ));
    self.edit_history.clear();
    self.edit_redo.clear();
    self.dirty = false;
    self.view_mode = ViewMode::Raw;
    Task::none()
}

fn exit_zen_edit_mode(&mut self) -> Task<Message> {
    self.sync_editor_to_source();
    self.editor = None;
    self.edit_history.clear();
    self.edit_redo.clear();
    self.view_mode = ViewMode::Rendered;
    if let Some(restore) = self.zen_restore.take() {
        self.sidebar_open = restore.sidebar_open;
        self.show_footer = restore.show_footer;
        self.search_open = restore.search_open;
    }
    self.restore_body_scroll()
}
```

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test zen_`

Expected: both tests pass.

### Task 2: Route Toggle And IPC Through Zen Helpers

**Files:**
- Modify: `src/app.rs`

**Interfaces:**
- Consumes: `App::enter_zen_edit_mode`, `App::exit_zen_edit_mode`, `App::sync_editor_to_source`.
- Produces: consistent keyboard and IPC entry into `ViewMode::Raw`.

- [x] **Step 1: Update `Message::ToggleViewMode`**

Change the `Message::ToggleViewMode` arm so Rendered/Mindmap call `enter_zen_edit_mode()` and Raw calls `exit_zen_edit_mode()`.

- [x] **Step 2: Update `Message::ToggleMindmap`**

When toggling from Raw to Mindmap, call `sync_editor_to_source()`, clear edit history/redo, clear `zen_restore`, and set `view_mode = ViewMode::Mindmap`.

- [x] **Step 3: Update IPC `Cmd::Mode`**

For `Mode::Edit`, initialize Zen via the same helper path. For `Mode::View`, exit Raw through `sync_editor_to_source()` and clear `zen_restore`.

- [x] **Step 4: Run targeted tests**

Run: `cargo test zen_`

Expected: both tests pass after routing changes.

### Task 3: Zen Layout And Keyboard Access

**Files:**
- Modify: `src/app.rs`

**Interfaces:**
- Consumes: existing `text_editor` builder and `status_footer`.
- Produces: centered editor surface, footer/help visibility, and app shortcut access while editing.

- [x] **Step 1: Change Raw editor layout**

Wrap the `text_editor` in a centered container with max width around the existing reader width plus editor padding. Keep markdown highlighting, JetBrains Mono, line height, and style.

- [x] **Step 2: Keep footer and keyboard button in Raw**

Keep current `footer_visible` condition as `self.show_footer && self.file.is_some() && self.view_mode != ViewMode::Mindmap`. Keep the keyboard button condition as `self.view_mode != ViewMode::Mindmap && self.overlay == Overlay::None`.

- [x] **Step 3: Allow core shortcuts while editing**

In the keyboard subscription, handle app command chords before the final `if editing { return Message::Noop; }`. Ensure `⌘P`, `⌘O`, `⌘⇧P`, `⌘B`, `⌘T`, `⌘+`, `⌘-`, `⌘0`, `⌘/`, `⌘S`, undo/redo, and `⌘E` work. Add `Esc` to exit Raw by returning `Message::ToggleViewMode` when `editing`.

- [x] **Step 4: Update shortcuts overlay copy**

Change the edit shortcut row from `Toggle Raw / Rendered` to `Toggle Zen Edit`, and include `Esc` as `Exit Zen Edit`.

- [x] **Step 5: Run compiler check**

Run: `cargo check`

Expected: check finishes successfully.

### Task 4: Verification And Commit

**Files:**
- Modify: `src/app.rs`
- Modify: `docs/superpowers/plans/2026-07-06-zen-edit-mode.md`

**Interfaces:**
- Consumes: completed Tasks 1-3.
- Produces: verified implementation commit.

- [x] **Step 1: Run automated checks**

Run: `cargo test zen_`

Expected: both tests pass.

Run: `cargo check`

Expected: check finishes successfully.

- [x] **Step 2: Inspect diff**

Run: `git diff -- src/app.rs docs/superpowers/plans/2026-07-06-zen-edit-mode.md`

Expected: diff only contains Zen edit mode implementation and this plan.

- [x] **Step 3: Commit implementation**

Run:

```bash
git add src/app.rs docs/superpowers/plans/2026-07-06-zen-edit-mode.md
git commit -m "feat: make edit mode zen focused"
```

Expected: commit succeeds without staging unrelated README/site changes.
