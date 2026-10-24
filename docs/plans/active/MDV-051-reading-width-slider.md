# MDV-051 — Reading width slider

Owner requested a bottom-right slider controlling the rendered document's
maximum width. Apply it to the main reading column, including Mermaid blocks;
keep preview-modal size and diagram gesture scale independent.

Use a compact theme-matched control at the bottom-right, below a shared row
containing the word-count/read-time pill and keyboard shortcut button. Use a
numeric width label and 1px steps, from 480px (or the available width if smaller)
to the available reading width, capped at 1920px. Default remains
720px. Allow the column to shrink with a narrow window. Keep the shortcut button and word-count pill beside each other above the slider. Hide the
slider in raw/data/mindmap views, settings and overlays.

Store the selected width in user prefs on release, with tolerant defaulting and
range clamping on load. During drag, update the reading layout immediately,
invalidate measured heights and rebuild/re-measure the virtual window. Remove
the renderer's fixed inner cap only for main content, leaving the outer reading
container responsible for the chosen width. Existing side-panel caps remain.

Acceptance: owner manually verifies widening/narrowing text and Mermaid,
window limits, scroll reflow and restart persistence on the release build.
The owner has asked to handle manual verification; no automated suite is run
for this iteration. Work is local and uncommitted; no push/publication.


Owner refinement: compute the upper endpoint from the current layout width,
subtracting the actual sidebar width + 5px divider and 64px reader padding.
Display the effective clamped width, so the thumb reaches the end when the
reading area is full instead of continuing through ineffective larger values.
Retain the saved requested cap so a temporarily narrower window does not lose
it. Use palette accent, ink rail, foreground and theme borders for all slider
states instead of the global Iced theme's default colors.

Owner layout refinement: the shared row places keyboard shortcuts on the left
and the word-count/read-time pill on the right, above the width slider.

Submitted for upstream review in Draft [PR #80](https://github.com/minchenlee/rmdv/pull/80).
The owner explicitly authorized this push and PR. It is not merged; native
acceptance and runtime packaging/CI follow-up remain as described above.
