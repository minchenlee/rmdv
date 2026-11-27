# Diagram width modes in Settings

Date: 2026-11-26
Area: `src/prefs.rs`, `src/render.rs`, `src/diagram.rs`, `src/app/settings.rs`,
`src/app/view/settings.rs`, `src/app/message.rs`, `src/app/dispatch.rs`,
`src/app/mod.rs`, `src/app/full_mindmap_view.rs`
Status: uncommitted working-tree change (not merged, not released)

## Problem

Every Mermaid/DOT figure was drawn inside the reading column and clamped to
`MAX_HEIGHT = 600 px`, with "Click to enlarge" the only way to see a taller or
wider diagram whole. There was no setting for how much room a figure gets: a
diagram could never use the reader's full width, even when the reading area was
much wider than the text column.

## Behavior change

`Settings → Reading → Diagram width` (and `prefs.json`'s `diagram_width`) now
chooses between four modes. Every mode keeps the figure's aspect ratio; only
`Auto` caps its height. The default is **Window wide**.

| Mode | Width | Height | Default |
| --- | --- | --- | --- |
| `figure_wide` — Figure wide | the figure's own width, capped to the reading area | full, no cap | |
| `window_wide` — Window wide | the reading area (content width, sidebar excluded) | full, no cap | yes |
| `figure_full` — Figure full | the figure's natural size, even past the reading area (the row scrolls sideways) | full, no cap | |
| `auto` — Auto | the reading column (fills it) | capped at 600 px, then click to enlarge | |

`Reading area` means the content area the reader is actually given: the window
minus the sidebar and the reader's 32 px side padding. A narrow diagram grows to
fill it in Window wide, keeps its natural size in Figure wide, and can overflow
into a horizontal scroller in Figure full. `Auto` is the original behavior: the
reading column plus the 600 px cap.

### Reader column loosening

The reader column is capped at the `Reading width` setting so prose keeps its line
length. A figure in Window wide or Figure full may need to be wider than that
cap, so `render` reports whether a mode can exceed the column
(`DiagramMetrics::loose`). When it can:

- the reader column stops capping its content (`max_width(∞)`), and
- each **prose** row re-caps itself at the reading width, while diagram rows keep
  the full width.

`Auto` keeps the original tight column, so this path is not used there and the
default-rendered geometry of an `auto` document is unchanged. Side panels (the
mind map panel and the Full Mindmap preview) never loosen: a figure is never
wider than the panel it fills, and prose keeps the panel's own cap.

## Implementation notes

- `prefs::DiagramWidth` (`FigureWide`, `WindowWide`, `FigureFull`, `Auto`) with
  `ALL`, `label()`, `step()` (wrapping), `#[serde(rename_all = "snake_case")]`,
  `#[default] WindowWide`, and a lenient deserializer mirroring
  `theme::ReadingWidth` and `ReadingFooter`: an unknown or malformed value falls
  back to the default instead of failing the whole prefs file.
- `render::diagram_fit(mode, figure, available) -> DiagramFit { width, height,
  scrolls }` is a pure function, so the four modes are unit-tested without Iced.
  `Auto` fills the reading column and then caps the height, shrinking the width
  with it, preserving the aspect ratio.
- The figure's **natural size** comes from the SVG's own `viewBox`.
  `diagram::RenderOutput` gained `svg_w`/`svg_h` (the raster is capped at
  1600 device px, i.e. 800 logical px at `RASTER_SCALE = 2.0`, so a wide figure's
  raster is smaller than the figure). `DiagramState::Ready` carries
  `figure_w`/`figure_h`, sized from the SVG rather than the raster, so a wide
  figure still lays out at its true width; the raster is drawn stretched to the
  chosen box.
- The figure is sized at layout time with `iced::widget::responsive`, which
  reports the row's actual width, instead of the app guessing the reader's
  geometry. That keeps the reading area exact when the sidebar or window changes.
- `Auto` keeps the card it always had: it spans the reading column with the
  (height-capped) figure centred inside it. The other three modes hug the figure
  so a narrow diagram does not get a stretched card.
- Switching the setting clears the virt `HeightCache` and rebuilds the window,
  because a diagram's measured height feeds the virtualized scroll geometry
  (exactly like `SetReadingWidth`).

## Tests added

`src/prefs.rs`:

- `diagram_width_defaults_and_round_trips` — missing/unknown/number/casing-wrong
  values fall back to `WindowWide`; all four names parse; serialization uses
  snake_case.
- `diagram_width_steps_wrap_around_the_four_modes`.

`src/render.rs`:

- `diagram_modes_size_the_figure_from_its_own_size` — per-mode width, height,
  and `scrolls`, plus zero-size, infinite-area, and 600 px cap cases.
- `diagram_metrics_keep_auto_in_the_narrow_column` — only `Auto` keeps the tight
  column; panels never loosen.

`src/app/tests.rs`:

- `settings_rows_change_the_same_state_as_their_commands_and_persist` now also
  steps the Diagram width row with → / ←, checks it saves, checks
  `diagrams_may_exceed_column` follows the mode, and that Space resets it.

`src/app/pointer_tests/reader_affordances.rs`:

- `diagram_width_setting_sizes_the_figure_in_the_reader` — lays out a real
  document in each mode and asserts the figure's on-screen box: window wide grows
  past the column, figure wide keeps its natural size, auto fits the column, prose
  keeps its line length throughout.
- `a_figure_wider_than_the_reading_area_scrolls_only_in_full_size` — a 4000 px
  wide figure overflows into a horizontal scroller in Figure full and fits in the
  other three modes.

## Superseded documentation

`docs/superpowers/specs/2026-05-16-diagrams-design.md` documented the fixed
`MAX_HEIGHT = 600 px` clamp as the only sizing rule. That clamp is now
`Auto`'s behavior, one of four modes; the spec's decision is superseded by this
change.

## Verification

| Check | Result |
| --- | --- |
| `cargo check --lib` | pass |
| `cargo check --no-default-features` | pass |
| `cargo check --all-targets` | pass |
| `cargo fmt --all --check` | pass |
| `git diff --check` | pass |
| `cargo test --lib` | 1008 passed, 16 failed (all pre-existing, see below), 3 ignored |

Pre-existing failures on this Windows host are unchanged and unrelated: the six
`pointer_tests::links::*` pill assertions, `terminal::*`, `fonts::tests::*`,
`prefs::tests::*`, `quick_slots::*`, `pointer_tests::tree_menu` /
`tree_ops` / `window` (Windows path separators and a recent-store count), and an
env-path assertion in
`app::tests::full_mindmap_without_workspace_adopts_home_in_background`. The six
`links::*` failures were confirmed to fail identically at `HEAD` in a clean
worktree.

## Not done / follow-ups

- The zoom modal still rasterizes at the viewer's own width cap; a diagram whose
  natural size is far past it remains soft in Figure full rather than re-rastered
  at true size. `rasterize_zoom` already pulls a sharper raster on demand for the
  zoomed viewer, so this only affects the inline (unzoomed) view.
- The setting has no keybinding; it is a Settings row only.
