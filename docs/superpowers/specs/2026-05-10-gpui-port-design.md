# GPUI Port Design

**Date:** 2026-05-10
**Branch:** `gpui-port`
**Status:** Approved design, pre-implementation

## Goal

Replace the Iced GUI layer with GPUI, using the [longbridge/gpui-component](https://github.com/longbridge/gpui-component) library for shared UI primitives. The port targets full feature parity with `main` (Iced version) and lives on a fresh branch `gpui-port`. Once parity ships, `gpui-port` supersedes `main` and Iced is removed.

The Iced version on `main` is left untouched during the port; no dual-build configuration is maintained.

## Scope

**In scope (parity targets):**

- Workspace folder browser with collapsible file tree
- Command palette (`⌘K`)
- Quick file finder (`⌘P`)
- Folder picker overlay (`⌘O`)
- Theme picker overlay
- Live reload via `notify`
- Tree-sitter syntax highlighting (Rust, Python, JS, TS, Go, C, Bash, JSON, HTML, Markdown)
- Light / dark themes with system follow (`dark-light`)
- Inter + JetBrains Mono + Lucide bundled fonts; CJK system font fallback
- Vim-style scrolling (`j` / `k` / `g` / `G` / `Space` / `Shift+Space`)
- In-document search (`⌘F`) with next/prev navigation
- Drag-and-drop files or folders onto the window
- All current keyboard shortcuts from the README
- macOS transparent titlebar / fullsize content view
- `RMDV_BENCH_STARTUP` first-frame timing instrumentation

**Out of scope:** new features, redesigned UX, changes to parsing or highlighting behavior.

## Dependencies

**Removed from `Cargo.toml`:**

- `iced`

**Added (pinned to a specific commit, resolved at implementation time):**

- `gpui = { git = "https://github.com/zed-industries/zed", rev = "<TBD>" }`
- `gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "<TBD>" }`

**Retained unchanged:** `pulldown-cmark`, `notify`, `image`, `open`, `arboard`, `dirs`, `dark-light`, `tokio`, `serde`, `serde_json`, `anyhow`, `tree-sitter`, `streaming-iterator`, all `tree-sitter-*` grammars, dev-deps `insta` and `criterion`.

`gpui` and `gpui-component` are git-only and are pinned by `rev` (commit SHA) for reproducible builds. Upgrades are explicit.

## Module Layout

The branch reuses every Iced-free module and rewrites only the rendering and event-loop surface.

```
src/
  lib.rs                       module exports
  main.rs                      gpui::App entry, window setup, font registration
  app.rs                       root AppState, action dispatch
  ui/
    mod.rs
    workspace.rs               root view: titlebar + sidebar + main column
    sidebar.rs                 file tree view
    document.rs                markdown render view
    overlay.rs                 modal layer (file finder / command / theme / folder)
    search_bar.rs              in-doc search bar
    icon.rs                    lucide icon-font helper
  state/
    mod.rs
    workspace_state.rs         WorkspaceState entity
    document_state.rs          DocumentState entity
    theme_state.rs             ThemeState entity
  ast.rs                       UNCHANGED
  parser.rs                    UNCHANGED
  highlight.rs                 UNCHANGED
  search.rs                    UNCHANGED
  tree.rs                      UNCHANGED
  recent.rs                    UNCHANGED
  watch.rs                     refactored: notify -> gpui channel
  theme.rs                     keep tokens (Palette, Typography, ThemePreset);
                               drop Iced Theme impl
  picker.rs                    keep Picker filesystem-nav struct;
                               drop Iced widget code
```

Modules deleted on this branch (their behavior is reimplemented under `ui/` and `state/`): the Iced widget halves of `app.rs`, `render.rs`, and `icon.rs`.

## State Model

Hybrid: shared GPUI entities for cross-cutting state, local view state for transient UI.

**Shared entities:**

| Entity | Holds | Observed by |
|---|---|---|
| `ThemeState` | `ThemeMode`, `ThemePreset`, `Palette`, `Typography` | every styled view |
| `WorkspaceState` | workspace path, `Vec<PathBuf>` files, `tree::Node`, `HashSet<PathBuf>` expanded | sidebar, file finder overlay |
| `DocumentState` | current file, `String` source, `Vec<Block>` ast, search query, `Vec<MatchPos>` matches, current match index | document view, search bar |

**Local view state** (kept inside the relevant view, not in entities): overlay query string and selected index, sidebar cursor index, scroll offsets.

State mutation pattern: views call `entity.update(cx, |s, cx| { ...; cx.notify(); })`. Subscribed views re-render via `cx.observe(&entity)`.

## Action Inventory

GPUI actions replace the Iced `Message` enum. Registered via `actions!` and dispatched through `cx.dispatch_action`.

```
file:    Open(PathBuf), OpenWorkspace(PathBuf), FileChanged(PathBuf)
overlay: OpenFolderPicker, OpenFileFinder, OpenCommandPalette,
         OpenThemePicker, CloseOverlay, OverlayMove(isize),
         OverlayConfirm, OverlayDescend, OverlayQueryChanged(String)
picker:  PickerNavigate(PathBuf), PickerParent, PickerHome, PickerSelectFolderHere
theme:   ToggleTheme, SetTheme(ThemePreset)
sidebar: ToggleSidebar, TreeToggle(PathBuf), TreeMove(isize),
         TreeActivate, TreeToggleAtCursor
scroll:  ScrollBy(f32), ScrollToTop, ScrollToBottom
search:  ToggleSearch, QueryChanged(String), NextMatch, PrevMatch
link:    OpenLink(String)
```

The Iced `Noop` and viewport-tracking messages are not ported; GPUI scroll handles its own viewport state.

## Data Flow

1. Key or mouse event in a view → dispatches action.
2. Root `AppState` view (or relevant child) handles the action via `cx.on_action`.
3. Handler updates the appropriate entity inside `entity.update(cx, ...)`.
4. Entity calls `cx.notify()`.
5. Subscribers re-render.

**Async work** (file load, workspace scan, watcher events): `cx.background_spawn` or `cx.spawn` with `tokio::fs` calls. Results return through `this.update(&mut cx, |state, cx| { ... })`. This replaces Iced `Task::perform`.

**File watch:** `notify::RecommendedWatcher` runs on its own thread, pushes events into a `smol::channel::unbounded` channel. A `cx.spawn` task reads the channel and dispatches `FileChanged(path)`. Debounce window: 50 ms (matches current behavior in `watch.rs`).

## Rendering Pipeline

`ui/document.rs` exposes `render_blocks(blocks, palette, typography, highlight) -> impl IntoElement`. The root is `div().flex_col().gap_3p5().max_w(px(780.))`, mirroring the current 780 px reading column cap.

Per-block dispatch:

| `Block` variant | GPUI element |
|---|---|
| `Heading { level, inlines }` | `div` with size from `Typography`, bold weight, child inline runs |
| `Paragraph(inlines)` | `div` at body size with inline runs |
| `CodeBlock { code, spans, lang }` | rounded `div` with code background, `JetBrainsMono`, `StyledText::new(code).with_runs(highlighted_runs)` |
| `BlockQuote(blocks)` | `div` with left border, padded, recurses into `render_blocks` |
| `List { ordered, items }` | `div().flex_col()` over rendered items |
| `Image { src, alt }` | `img(src)` via `gpui::img()`; alt as accessibility label |
| `ThematicBreak` | `div().h_px().bg(rule_color)` |
| `Table { ... }` | grid of header row + body rows |
| `Html(s)` | `div().child(s)` raw passthrough |

**Inline runs:** inlines are flattened to `StyledText::new(text).with_runs(Vec<TextRun>)`. Each `TextRun` carries color, font weight, italic, underline, strikethrough, and font family. Search highlights overlay as background-colored runs at `MatchPos` ranges. Inline `code` spans switch to `JetBrainsMono` and the code-foreground color from the palette.

**Syntax highlight bridge:** `highlight::highlight(code, lang) -> Vec<StyleSpan>` is unchanged. A new helper `style_color(span.style, &palette) -> gpui::Hsla` mirrors the Iced color mapping. `StyleSpan`s are converted to `TextRun`s with the right byte ranges.

**Sidebar (`ui/sidebar.rs`):** `gpui_component::list::List` (or `uniform_list` if perf demands it) over a flattened walk of `tree::Node`. Each row is chevron icon (lucide font) + filename. Click → dispatches `Open(path)` for files or `TreeToggle(path)` for directories.

**Overlay (`ui/overlay.rs`):** a single modal view discriminated by an enum `OverlayMode { FolderPicker, FileFinder, Command, ThemePicker }`. Centered `div` with backdrop, `gpui_component::input::TextInput` at top, filtered list below. Existing `picker::Picker` filesystem-nav logic feeds the folder picker; `recent.rs` and `WorkspaceState.files` feed the file finder.

**Search bar (`ui/search_bar.rs`):** docked above the document when active. `TextInput` + match counter (`3 / 12`) + prev/next buttons. Match highlights are background-colored `TextRun`s injected during inline rendering.

## Window, Fonts, Theming

**Window options (`main.rs`):**

- macOS: `WindowOptions { titlebar: Some(TitlebarOptions { appears_transparent: true, traffic_light_position: Some(point(px(12.), px(12.))), ..default() }), ..default() }` — preserves the current Iced `title_hidden` + `titlebar_transparent` + `fullsize_content_view` behavior.
- Other platforms: standard titlebar.

**Initial CLI argument:** parsed before `App::run`, dispatched as `Open(path)` after the first frame. The `--benchmark-startup` flag continues to set `RMDV_BENCH_STARTUP=1` early, before any GPUI threads spawn.

**Fonts:** Inter, JetBrains Mono, and Lucide are registered via `cx.text_system().add_fonts(...)` in `main.rs`. Default font is `Inter`. Code blocks use `JetBrainsMono`. Icons use `lucide`. CJK fallback comes from the platform font system through GPUI's text system.

**Theme:** `ThemeState` exposes `Palette` (bg, fg, accent, muted, code_bg, border) and `Typography` (h1-h6 sizes, body size, code size, line height). Views read it via `theme_state.read(cx)`. System mode detection via `dark-light::detect()` runs on app start and on a `cx.spawn` polling loop (matching current behavior). `ToggleTheme` cycles `ThemePreset`; `SetTheme(preset)` sets directly.

## Keybindings

Registered with `KeyBinding::new("...", Action, context)` at app init. Context filtering ensures vim keys only fire when no text input is focused.

```
cmd-p             OpenFileFinder
cmd-k             OpenCommandPalette
cmd-o             OpenFolderPicker
cmd-b             ToggleSidebar
cmd-f             ToggleSearch
cmd-t             ToggleTheme
escape            CloseOverlay (overlay context) | search off (search context)
j / k             ScrollBy(+/- line)        (Document context)
g / G             ScrollToTop / Bottom      (Document context)
space             ScrollBy(+page)           (Document context)
shift-space       ScrollBy(-page)           (Document context)
enter             OverlayConfirm | TreeActivate
up / down         OverlayMove(-1/+1) | TreeMove(-1/+1)
right             OverlayDescend | TreeToggleAtCursor expand
left              TreeToggleAtCursor collapse
```

## Drag and Drop and Links

`WindowEvent::FileDrop` handler inspects the path: file → `Open(path)`, directory → `OpenWorkspace(path)`. Multiple drops use the first path; later paths are ignored (matches current behavior).

Markdown link clicks dispatch `OpenLink(href)`, which calls the `open` crate with the URL or a resolved local path.

## Milestones

Each milestone is buildable and visually testable; each ends in one or more commits.

1. **M1 — Skeleton boots.** Cargo swap (remove iced, add `gpui` + `gpui-component` pinned). Empty window opens with fonts registered and titlebar styled. Stub `app.rs`.
2. **M2 — Render markdown.** `DocumentState` entity. CLI arg → file load → parser → AST → `ui/document.rs` renders all `Block` variants. Verify visual parity against an Iced screenshot of the same document.
3. **M3 — Themes.** `ThemeState`, light/dark/system, `cmd-t` toggle, `dark-light` system detection. Palette wired through document.
4. **M4 — Syntax highlight.** Code blocks consume `highlight.rs` spans through the `style_color` bridge into `TextRun`s. Verify all ten grammars render.
5. **M5 — Workspace and sidebar.** `WorkspaceState`, file tree view, `cmd-o` folder picker, click-to-open, `cmd-b` sidebar toggle.
6. **M6 — Overlays.** File finder (`cmd-p`), command palette (`cmd-k`), theme picker. Shared overlay view, fuzzy filter via existing picker logic.
7. **M7 — Keys.** Vim scrolling, tree navigation, overlay navigation; all keybindings with context filtering.
8. **M8 — Search-in-doc.** `cmd-f` bar, `MatchPos` highlight runs, next/prev navigation.
9. **M9 — Live reload.** `watch.rs` refactored to push through a gpui channel; debounce preserved.
10. **M10 — Drag and drop + links.** File drop dispatches `Open` or `OpenWorkspace`; link clicks call the `open` crate.
11. **M11 — Polish.** Scroll position persistence, recents, packaging metadata sanity, README updates, `RMDV_BENCH_STARTUP` first-frame timing wired in workspace view.

## Testing

Pure-logic tests in `tests/` (parser, highlight, search) survive the port unchanged. Insta AST snapshots remain valid. Each milestone adds at least one smoke test that constructs the relevant GPUI view in a headless `TestAppContext` and verifies it renders without panic. The `cold_start` Criterion bench is updated to point at the GPUI binary in M1.

## Risks and Open Questions

- **`gpui` and `gpui-component` API churn.** Both are git-only and unstable. Pinning to a specific rev mitigates this; upgrades will be intentional and may require porting effort.
- **Reading-column reflow performance.** The Iced version uses `rich_text` with span batching. GPUI `StyledText` performance on long documents (>10k lines) is unverified. M2 should bench against the existing `cold_start` bench corpus.
- **CJK font fallback** in GPUI is delegated to the platform text system. Verify on macOS and Windows in M2.
- **macOS traffic-light position.** Exact pixel offset (`point(px(12.), px(12.))`) is a best-guess match for the current Iced layout and may need visual tuning in M1.
- **Tokio runtime integration.** GPUI provides its own executor; `tokio::fs` calls run in `cx.background_spawn`. Confirm no double-runtime issues during M2.

## Non-Goals

- No changes to parsing semantics, AST shape, or syntax-highlight output.
- No new viewer features beyond current README parity.
- No iced/gpui dual-build, feature flag, or workspace split.
- No code signing, auto-update, or PDF/HTML export work (those remain Roadmap items, untouched).
