# MDV-031 — Visual refinement on the design tokens

State: ready
Owner / accountable lead: owner (minchenlee); implementation by Claude
Active writer: none
Created: 2026-10-05
Updated: 2026-10-05

## Outcome

rmdv looks calmer and more consistent in every built-in theme. Chrome states
(hover, borders, selection) come from a few derived tokens instead of hand-picked
colors. Light themes get a deliberate surface hierarchy, and an opt-in "Soft"
syntax option is added. The method follows the zeron design research
(`docs/research/zeron-design-research-2026-10-04.md`, local only).

Owner scope decision (2026-10-05): items 1–4 below; keep the font; add Soft
syntax as an option.

## Scope

1. **Derived overlay tokens.** Add `ink(alpha)`, `hairline()`, and
   `wash(alpha)` helpers on `Palette`.
   - `ink` tints with `fg` toward the dark/light side. Dark or light is decided
     by the luminance of `bg`, not by preset, so custom themes work too.
   - `hairline` is the 1px border; in light themes it is ×1.35 stronger.
   - `wash` is an accent-tinted fill.
   - Replace the hand-written hover fills (`pal.surface_alt` in the ~18
     `button::Status::Hovered`/`Pressed` arms of `src/app/view/*`) and the
     chrome borders with these helpers.
2. **Radius ladder and elevation.**
   - Collapse `theme::radius` to 6 (controls, rows), 10 (code blocks, popovers,
     in-document cards), and 16 (dialogs, cards, sheet). Keep `XS` = 4 for
     inline code and keycaps, and keep `PILL`.
   - Rename call sites by step, not by pixel value.
   - Elevation has three surface steps: content `bg`; sidebar and code; and
     popovers, which are separated from content by a hairline plus the
     existing `shadow` tokens rather than by a heavier fill.
3. **Light-theme role reassignment.** Applies to One Light, GitHub Light, and
   Solarized Light.
   - Content is the lightest surface; the sidebar is a grey step below it.
   - Popovers float by border and shadow.
   - Body text is not maximum contrast: no pure black on white.
   - Each theme keeps its signature background hue (Solarized keeps `base3`).
   - To clear the contrast floors, Solarized Light shifts one step darker:
     body text moves from `base00` to `base01`, and muted text moves from
     `base0` to `base00` (see the decision log).
4. **Code-block recipe and accent-derived tokens.**
   - Code blocks: `ink(0.035)` fill, 1px hairline, radius 10, padding 12×10.
     Inline code uses radius `XS` and a lighter fill.
   - `selection`, `match_bg`, and `tree_selected_bg` become accent-derived
     `wash` values with per-mode alphas. `match_current_bg` stays a strong
     solid.
5. **Soft syntax option.**
   - A command-palette toggle, "Toggle Soft Syntax Colors", persisted as
     `prefs.soft_syntax` (default off).
   - When on, the active `SyntaxPalette` is transformed: hues desaturated ×0.72,
     and `variable`, `operator`, and `punctuation` take the text color.
   - Works on top of built-in and custom themes. The upstream-accurate preset
     constants are not edited.
   - Palette assignment goes through one `App::apply_palette` so the
     transform cannot be skipped. `self.palette` is assigned at about 6 sites
     today (`src/app/mod.rs`, `src/app/dispatch.rs`).

## Non-goals

- No font change (Inter stays), and no change to typography sizes or layout
  metrics.
- No window vibrancy or frosted glass, and no GPUI work. A macOS vibrancy spike
  is a separate optional follow-up.
- No sidebar or overlay fade animations (the old backlog wording; outside the
  chosen 1–4 scope).
- No Theme Settings or Theme Studio UI (`MDV-019` builds on these tokens later).
- Do not edit the upstream-accurate `SyntaxPalette` presets.

## Constraints and authority

- Custom theme files (`src/theme_load.rs`) keep working.
  - Explicit `[ui]` overrides still win over derived values.
  - A theme that overrides `surface_alt` keeps that colour where `surface_alt`
    is still used. Hover no longer reads it, so record this in the theme-file
    docs if any shipped example relies on it.
- Release-mode checks only. Heavy cargo runs are announced to `tk-main-orch` and
  never overlap its slot.
- Push branch and open PR once verified; stop before merge.

## Owned and excluded surfaces

- Owned:
  - `src/theme.rs` (tokens, helpers, light presets, radius, Soft transform)
  - Style closures in `src/app/view/*.rs`, `src/render.rs`, and
    `src/app/full_mindmap_view.rs`
  - `src/prefs.rs` (`soft_syntax`)
  - The palette-assignment sites and the command-palette entry in `src/app/`
- Excluded:
  - Parser and layout code
  - `src/mindmap.rs` geometry
  - The site, packaging, and release metadata

## Delivery

One branch with one commit per scope item. One PR, one Verify app build, and
one owner acceptance round across all ten themes. A slice that the owner
rejects can be reverted alone.

## Acceptance evidence

- Unit tests in `src/theme.rs` for every built-in preset, Soft on and off:
  - `fg` vs `bg`, `sidebar`, and `code_bg` ≥ 4.5.
  - `muted` vs `bg` and `sidebar` ≥ 3.0.
  - `accent_fg` vs `accent` ≥ 3.0.
  - Light themes: `fg` vs `bg` ≤ 17 (not maximum contrast).
  - Helper tests: `ink`/`hairline` direction follows `bg` luminance, and a
    custom palette with an explicit override keeps it.
- Baseline before this work (WCAG ratio, computed 2026-10-05 from
  `src/theme.rs`): Solarized Light fails five checks: `fg` vs `bg`
  4.13, `sidebar` 3.85, and `code_bg` 3.64; `muted` vs `bg` 2.93 and
  `sidebar` 2.73.
  - One Light `muted/sidebar` 4.30 and `accent_fg/accent` 3.12 pass the
    floors above.
  - Every other preset already passes.
- Before/after screenshots of `demo/` for all ten themes via the IPC smoke
  harness (`rmdv screenshot`), attached to the PR.
- `cargo test --release -- --test-threads=1`, the lean
  `--no-default-features` check, a release build, `cargo fmt --all --check`,
  and `git diff --check` pass. PR CI is green.
- Owner native acceptance on `~/Applications/rmdv Verify.app`, per theme and
  with Soft on and off.

## Progress

- [x] Owner chose scope 1–4, no font change, Soft as an option (2026-10-05).
- [x] Survey the style call sites and baseline contrast.
- [ ] Item 1: overlay helpers and hover/border replacement.
- [ ] Item 2: radius ladder and elevation.
- [ ] Item 3: light-theme roles and contrast tests.
- [ ] Item 4: code-block recipe and accent-derived tokens.
- [ ] Item 5: Soft syntax option.
- [ ] Screenshots, PR, Verify app, owner acceptance.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-10-05 | Scope items 1–4, keep Inter, add Soft syntax as opt-in. | Owner decision. |
| 2026-10-05 | Derive dark/light for overlays from `bg` luminance. | Custom themes have no preset flag; one rule covers both. |
| 2026-10-05 | Solarized Light text and muted shift one step darker (`base01`, `base00`). | `base00` on `base3` is 4.13:1, below the 4.5 floor; `base0` muted is 2.93:1, below 3.0. `base01` is Solarized's own emphasis tone. |
| 2026-10-05 | Ship as one PR with per-item commits. | Visual acceptance is one owner round; per-item commits keep rejection cheap. |

## Blockers and escalation

- If a theme cannot satisfy both its upstream identity and the contrast floor,
  bring the two options to the owner with screenshots rather than choosing
  silently.

## Final evidence

- Pending.
