# MDV-003 — Explain image-only PDF results

State: ready
Owner / accountable lead: unassigned
Active writer: none
Created: 2026-07-18
Updated: 2026-07-18

## Outcome

When local PDF extraction succeeds but returns no text, show a clear
OCR-disabled explanation instead of an unexplained blank document.

## Non-goals

- Do not add OCR, cloud processing, document editing, or Windows PDF support.
- Do not treat an extraction error as the same state as a valid image-only PDF.

## Constraints and authority

- PDF extraction remains local and feature-gated behind `pdf`.
- Native text-layer PDFs must retain their current Markdown rendering.
- The message should be accessible and actionable without implying OCR exists.

## Owned and excluded surfaces

- Owned: `src/pdf.rs`, the PDF load path in `src/app.rs`, focused fixtures/tests,
  and user-facing empty-text guidance.
- Excluded: liteparse/PDFium internals, OCR dependencies, packaging, updater,
  and non-PDF empty documents.

## Acceptance evidence

- A successful zero-text extraction produces the named OCR-disabled state.
- Extraction failures retain error semantics; text PDFs and empty Markdown
  files remain unchanged.
- Feature-on tests cover the three states, feature-off builds still compile,
  and relevant app/PDF checks pass.

## Progress

- [x] Confirm `src/pdf.rs` explicitly disables OCR and may return empty text.
- [ ] Define the result/state boundary and exact user message.
- [ ] Add regressions and implement the smallest change.
- [ ] Verify feature-on and no-default-feature paths.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-07-18 | Distinguish valid empty extraction from parser failure. | These states require different user guidance and acceptance evidence. |

## Blockers and escalation

- Escalate if product direction expands from feedback into OCR or external
  processing; that would require a new task and privacy/dependency decisions.

## Final evidence

- Pending: message contract, fixtures/tests, diff/commit, commands/results, and
  manual rendering check if UI layout changes.
