# KB shortcut hints + Open-Themes-Folder command

**Date:** 2026-06-14
**Branch:** `feat/kb-hints-custom-themes` (worktree off `main`)

Four additive, low-risk UI features. No new deps. All styling pulls from `Palette`.

## Feature 1 — Mindmap right-panel hint row

The mindmap (⌘M) right-side leaf panel gains a pinned hint row at its bottom.

- **Where:** `App::mindmap_panel_view` (`src/app.rs:1317`). Currently ends in
  `container(scrolled).width(Fixed(panel_width)).height(Fill).center_y(Fill).style(...)`.
- **Change:** wrap content as `column![ container(scrolled).height(Fill), divider, hint_row ]`
  inside the same outer styled container so the hint pins to the bottom and the
  scrollable takes remaining height (drop `center_y`).
- **Hint content:** `←↑→↓ navigate · Space fold · ⌘⌥B panel · ⌘B sidebar`
- **Style:** small pills — `surface_alt` cap background, `rule` 1px border, `fg` cap
  text size 11, `subtle` label size 11 — copied from the `picker_hint_footer` inner
  closure (`src/app.rs:5808`). A 1px `rule` divider line above the row.

## Feature 2 — Floating cheatsheet button (document view)

A rounded lucide-keyboard icon button, bottom-right of the doc viewport, opens the
shortcuts overlay.

- **Where:** the `base` stack at `src/app.rs:3711`
  (`stack![main, footer_layer, overlay_layer]`). Insert a `kb_button_layer` before
  `overlay_layer` (so overlay stays topmost):
  `stack![main, footer_layer, kb_button_layer, overlay_layer]`.
- **Button:** `ghost_lu(ic::KEYBOARD, pal).on_press(Message::ToggleShortcuts)` in a
  full-size container `align_x(Right).align_y(Bottom)`, padded to sit ABOVE the
  word-count pill so the two don't overlap (extra bottom padding).
- **Visibility:** only when `!self.mindmap` and `self.overlay == Overlay::None`
  (no button over the mindmap canvas or while an overlay is open).
- **Icon:** add `pub const KEYBOARD: char = '\u{e5de}';` to `src/icon.rs`.

## Feature 3 — Sidebar tab-row hint

The empty space right of the Files/Outline tabs gets a compact nav hint.

- **Where:** `tab_row` irow in `sidebar_view` (`src/app.rs:4370`).
- **Change:** append `Space::new().width(Fill)` then the hint element after the two
  tab buttons. `align_y(Center)` on the row.
- **Hint content:** `↑↓ move · Enter open` (the keys that drive the file/outline list).
- **Style:** same small-pill helper as Feature 1.

## Feature 4 — "Open Themes Folder" command

Reveal the custom-themes directory in Finder; the existing theme watcher
(`theme_watch`, 500ms poll) already hot-reloads any edits.

- **Message:** add `OpenThemesDir` variant (`src/app.rs` Message enum, ~line 272).
- **Palette entry:** in `command_items()` after `("Reload Custom Themes", …)`
  (`src/app.rs:1415`): `("Open Themes Folder", Message::OpenThemesDir)`.
- **Handler:** in `update()` after the `ReloadThemes`/`ThemeFilesChanged` arms:
  ```rust
  Message::OpenThemesDir => {
      if let Ok(dir) = crate::theme_load::ensure_themes_dir() {
          let _ = open::that_detached(&dir);
      }
      Task::none()
  }
  ```
  `ensure_themes_dir()` creates the dir on first use; `open` crate already in
  `Cargo.toml` and used for URLs at `src/app.rs:2470`.

## Shared helper

Extract a `hint_pills<'a>(items: &[(&'a str, &'a str)], pal: Palette) -> Element<'a, Message>`
from the `picker_hint_footer` inner closure pattern. Features 1 and 3 both use it.
Each item = (keys, label); rendered as cap-pill + label, separated by a `·`-style
gap (Space). Keeps one styled-pill builder.

## Verification

Build `--release`. Drive the running binary over IPC (mdv-cli skill):
1. Open a multi-heading doc → ⌘M → confirm mindmap panel shows the bottom hint row.
2. Default doc view → confirm floating keyboard button bottom-right, not overlapping
   word-count pill → click sends ToggleShortcuts (cheatsheet opens).
3. Open sidebar (⌘B) → confirm hint beside Files/Outline tabs.
4. Command palette (⌘⇧P) → type "theme" → confirm "Open Themes Folder" entry → run it
   → themes dir opens in Finder.
Screenshot each surface and inspect before claiming done.

## Out of scope

- Theme import-from-file picker and new-theme-from-template (the `import_auto` path in
  `theme_import.rs` stays unwired). User chose folder-reveal only.
