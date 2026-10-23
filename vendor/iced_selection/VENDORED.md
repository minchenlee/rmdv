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
- `word` (new) gives the word boundaries that double-click and word-drag
  selection use. UAX #29 makes every Chinese or Japanese character its own
  word; on macOS each such run is split again with Core Foundation's
  `CFStringTokenizer` (`kCFStringTokenizerUnitWordBoundary`, through
  `objc2-core-foundation`, a macOS-only dependency already in rmdv's
  tree). Other text and other platforms keep UAX #29. Keyboard word moves
  (Shift+Option+Arrow) keep upstream's `Value` boundaries. The words of the
  last four lines are kept per thread and one tokenizer is reused per
  thread, so a word drag segments its line once, not on every move.
- `text::rich::State::dragging`, `selection_ends`, and `snap` let rmdv's
  reader keep a double-click (word) or triple-click (line) drag whole
  when it crosses into another block, and extend a widget's selection
  from its anchor on Shift+click.
- A double-click selects the word of the grapheme under the pointer (the
  hit's affinity tells which side of the caret it is), not the words on
  both sides of the caret.

Keep changes small and listed here so a later upstream release can replace
this copy.
