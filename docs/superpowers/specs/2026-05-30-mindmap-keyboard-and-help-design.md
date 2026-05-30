# Mindmap keyboard panel sizing, shortcut cheatsheet, outline auto-scroll

Date: 2026-05-30
Status: approved (pre-implementation)

Three independent features. Each ships and verifies on its own.

---

## Feature 1 — Cycle mindmap panel width by keyboard (⌘⌥W)

### Goal
In mindmap mode the content panel can only be sized by dragging its handle.
Add a keyboard step-cycle through three window-relative widths:

- **1/3** of window width
- **1/2** of window width
- **3/5** of window width

Single key cycles `1/3 → 1/2 → 3/5 → 1/3`. Drag handle stays and still
produces arbitrary px widths (free resize coexists with stepping).

### Key
`⌘⌥W` (W = width). Matched by **physical key code** `Code::KeyW`, because
`alt`+letter swaps the produced character on macOS — same handling already
used for `⌘⌥B` (panel toggle) at `src/app.rs:2430`.

### State
Add to `App`:

- `mindmap_panel_step: u8` — current step index `0|1|2`. Init `0`.
- `window_size: Option<iced::Size>` — latest window size, used to compute
  fraction widths. Init `None`.

Fraction table: `const MIND_PANEL_FRACS: [f32; 3] = [1.0/3.0, 0.5, 0.6];`

### Window width source
No window size is tracked today; the canvas's `last_bounds_w` is
program-state-local and not reachable from `update`. Add a subscription to
`iced::window::resize_events()` (`Subscription<(Id, Size)>`, confirmed present
in iced_runtime 0.14) mapped to a new `Message::WindowResized(iced::Size)`,
which stores `self.window_size = Some(size)`.

`resize_events` fires on resize, not at startup. If `window_size` is still
`None` when the cycle key is first pressed, fall back to the default
`MIND_PANEL_DEFAULT` px width for that press (advance the step but apply the
default width); the next press after any resize uses the real width. This is
the only edge case and is acceptable — most cycles happen after the window has
been sized at least once.

### Message + behavior
New `Message::MindmapCyclePanelWidth`:

1. If panel closed, open it (`mindmap_panel_open = true`).
2. `mindmap_panel_step = (mindmap_panel_step + 1) % 3`.
3. Compute target: `w = window_w * MIND_PANEL_FRACS[step]` (or
   `MIND_PANEL_DEFAULT` if `window_size` is `None`).
4. `mindmap_panel_width = w.clamp(MIND_PANEL_MIN, MIND_PANEL_MAX)`.

Re-clamp keeps the existing 240–900 invariant intact (the canvas snap logic
in `mindmap.rs` already reads `panel_width`).

### Key wiring
In the `⌘⌥` physical-code block (`src/app.rs:2430`), add:

```rust
if let Physical::Code(Code::KeyW) = physical {
    if mindmap {
        return Message::MindmapCyclePanelWidth;
    }
}
```

Gated on `mindmap` (already computed in subscription scope). Does nothing
outside mindmap mode.

### Discoverability
- Command palette: add `("Cycle Mindmap Panel Width  ⌘⌥W", Message::MindmapCyclePanelWidth)`
  near the other mindmap entries (`src/app.rs:~997`).
- Cheatsheet (Feature 2): listed under **Mindmap**.

### Interaction with drag
Drag still writes a free `mindmap_panel_width`. The step index is not
recomputed from drag width — after a manual drag the next ⌘⌥W simply advances
to `step+1` from wherever the cursor was in the cycle. Accepted: stepping is a
quick-snap convenience, not a precise stateful slider. (No attempt to infer the
"nearest step" from a dragged width — that adds complexity for no real gain.)

---

## Feature 2 — Shortcut cheatsheet overlay (⌘/)

### Goal
A read-only, grouped, **non-searchable** keyboard cheatsheet for users not yet
fluent in the shortcuts. Distinct from the existing command palette (which is
searchable + executable). ⌘/ is the conventional "show shortcuts" key.

### Overlay
Add `Overlay::Shortcuts` variant. ⌘/ toggles it:

- Char block (`src/app.rs:2451`): `"/" if cmd => return Message::ToggleShortcuts`.
- `Message::ToggleShortcuts`: if `overlay == Overlay::Shortcuts` close it
  (`Overlay::None`), else `open_overlay(Overlay::Shortcuts)`.
- Esc closes it — existing escape handling covers any open overlay; confirm
  `Overlay::Shortcuts` is treated like other overlays in the close path.

### Rendering
New `fn shortcuts_overlay(pal) -> Element` reusing `overlay_frame`. No input
field, no cursor, no filtering. A scrollable `Column` of grouped rows.

Group → rows, each row = key chip + action label. Groups:

- **File** — Open Folder ⌘O, Find File ⌘P, Save ⌘S
- **Navigation** — Find in Document ⌘F, Scroll Top ⌘↑, Scroll Bottom ⌘↓,
  outline/tree arrows
- **View** — Toggle Sidebar ⌘B, Raw/Rendered ⌘E, Theme ⌘T, Hidden Files ⌘⇧.,
  Font ⌘+/⌘-/⌘0, Command Palette ⌘⇧P
- **Mindmap** — Toggle Mindmap ⌘M, Toggle Panel ⌘⌥B, **Cycle Panel Width ⌘⌥W**,
  arrows to navigate, Space to fold/unfold
- **Help** — Show Shortcuts ⌘/

The list is hand-authored in `shortcuts_overlay` (small, ~25 rows). Not
generated from `filtered_commands` because the cheatsheet groups by category
and includes non-command bindings (arrows, Space) that the palette omits.
Acceptable minor duplication; both reference the same human-facing key strings.

### Overlay sizing / scroll
Static height card (reuse the overlay frame sizing used by command palette).
Wrap the grouped column in a `scrollable` in case it exceeds the frame.

### Esc / dismiss
No cursor navigation. Esc and clicking the backdrop dismiss (same as other
overlays). No `OverlayMove`/`OverlayConfirm` handling for this overlay.

---

## Feature 3 — Outline panel auto-scroll on keyboard nav

### Goal
When the Outline sidebar tab is open and the user arrows through sections, the
focus can move past the last visible row without the panel scrolling. The
outline scrollable should auto-follow the cursor, exactly like the file tree
already does.

### Pattern
The file **tree** already implements this end-to-end. Replicate it for the
outline:

| Tree (exists)              | Outline (to add)              |
|----------------------------|-------------------------------|
| `tree_scroll_id()`         | `outline_scroll_id()` (Id `"outline"`) |
| `tree_viewport: Option<Viewport>` | `outline_viewport: Option<Viewport>` |
| `Message::TreeScrolled(v)` | `Message::OutlineScrolled(v)`  |
| `.id` + `.on_scroll` on scrollable | same on outline scrollable |
| `scroll_tree_to_cursor()`  | `scroll_outline_to_cursor()`  |
| `TreeMove` returns scroll task | `OutlineMove` returns scroll task |

### Changes
1. `App` fields: `outline_viewport: Option<iced::widget::scrollable::Viewport>`
   (init `None`). Add `outline_scroll_id()` returning `Id::new("outline")`.
2. `sidebar_outline_body` (`src/app.rs:3564`) scrollable: add
   `.id(App::outline_scroll_id())` and `.on_scroll(Message::OutlineScrolled)`.
3. `Message::OutlineScrolled(v)` handler: `self.outline_viewport = Some(v)`.
4. `fn scroll_outline_to_cursor(&self) -> Task<Message>`:
   `edge_scroll(Self::outline_scroll_id(), self.outline_viewport.as_ref(),
   self.outline_cursor, self.outline_sections.len(), 26.0)`.
   Row height **26.0** — matches `outline_row`'s `.height(Length::Fixed(26.0))`.
5. `Message::OutlineMove(d)` (`src/app.rs:2035`): after updating
   `outline_cursor`, return `self.scroll_outline_to_cursor()` (currently returns
   `Task::none()`/`Noop`).

`edge_scroll` already handles the no-viewport fallback (relative snap) and the
top/bottom-edge math; no new scroll logic needed.

---

## Risk / sequencing

- **Feature 3** — lowest risk. Mirror of tested tree code, one row-height
  constant, no new UX. Do first.
- **Feature 2** — medium. New overlay variant but read-only; reuses
  `overlay_frame`. Verify Esc/backdrop dismiss path includes the new variant.
- **Feature 1** — one real unknown (window width), resolved via
  `window::resize_events` subscription + `None` fallback. Verify the resize
  subscription is added to the combined `Subscription::batch` and that the
  fallback path doesn't panic when `window_size` is `None`.

### Success criteria
- F1: In mindmap mode, ⌘⌥W cycles the panel through ~1/3, ~1/2, ~3/5 of the
  window width (clamped 240–900); drag still works; opens panel if closed.
- F2: ⌘/ toggles a grouped static cheatsheet listing every shortcut incl.
  ⌘⌥W; Esc/backdrop closes it.
- F3: Arrowing the outline past the visible edge scrolls the outline panel to
  keep the focused section in view, matching file-tree behavior.
