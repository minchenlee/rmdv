# Vendored iced_selection

Upstream: <https://crates.io/crates/iced_selection> version 0.5.0 (MIT, see
`LICENSE`). Copied from the published crate unchanged in the first commit;
later commits add only what rmdv needs to select across reader blocks:

- `text::rich::State` is public, with methods to hit-test a point, read the
  text between two positions, clear the widget's own selection, and draw a
  selection range set from outside.
- `Rich::operate` hands its `State` to `Operation::custom`, so a parent
  widget can find every reader text widget and its bounds.
- Link hover messages go out only when the hovered link changes (once on
  enter, once on leave), not on every hover or selection change. New
  `Rich::on_link_enter` and `Rich::on_link_leave` hooks name the link and
  an id unique to the widget's state, so two widgets that hold the same
  link are told apart. A widget scrolled out of view while it hovers a
  link reports the leave too.
- `text::Style::link_hover` (`LinkHover`: fill and corner radius) tints a
  hovered link instead of drawing a second underline over it.
- `text::rich::State::link_at`, `selected_text`, and `selection_contains`
  hit-test a right press for rmdv's reader context menu: the link under
  the pointer, the widget's own selected text, and whether the pointer is
  inside that selection. They read state only; a right press still changes
  no selection.

Keep changes small and listed here so a later upstream release can replace
this copy.
