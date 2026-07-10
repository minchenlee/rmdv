# rmdv — shared project status

Last reconciled: 2026-07-10 (Asia/Taipei)

## Read this first

- Actual checkout: `/Users/liminchen/Documents/GitHub/mdv`
- Legacy non-repo path: `/Users/liminchen/Documents/GitHub/mdv-main`
- Active branch: `fix/cjk-emphasis-issue-6`, eight commits ahead of
  `origin/fix/cjk-emphasis-issue-6`.
- `main` / `origin/main` is at the released `v0.4.0` tag (`34d352d`).
- One worktree is registered: the checkout above. No parallel Git worktree is
  currently registered.

## Completed and committed

1. **v0.4.0 PDF viewing release** is on `main` and tagged `v0.4.0`.
   It includes local PDF-to-Markdown viewing, PDFium packaging for macOS and
   Linux, and associated site/demo/docs work. Windows deliberately builds
   without the PDF feature.
2. **CJK emphasis fix** is the upstream base of the active branch
   (`origin/fix/cjk-emphasis-issue-6`).
3. **Zen edit mode** is committed only on the active branch in four commits:
   focused edit mode, shared viewer/editor width, and Command-arrow editor
   navigation. It is not merged to `main`.
4. **Zen unsaved-edit protection** is committed as `f49f909`
   (`fix: preserve unsaved Zen edits across navigation`). It prevents dirty
   content from being replaced through UI, IPC, vault, link, and late async
   load paths; it also tracks the last successfully persisted source so a
   failed save cannot clear the switch guard.
5. **README and release-history cleanup** is committed as `739ad4e`
   (`docs: clarify capabilities and archive v0.4 audit`). The README now
   separates historical benchmarks from current claims, accurately describes
   PDF/Windows support, and the v0.4 audit is explicitly historical.
6. **Landing-site simplification and hardening** is committed as `7a02d3f`
   (`style(site): simplify layout and fix accessibility`). It removes the
   carousel/reveal code, fixes the inline-theme CSP hash, makes screenshots
   keyboard-operable, preserves FAQ Space-key behavior, and updates release
   metadata.

## Current state

- No source, documentation, or site work remains uncommitted after the status
  coordination files themselves are committed.
- Do not merge, push, tag, release, or deploy without a new explicit request.

## Verification evidence

- `cargo test --target-dir /private/tmp/mdv-zen-safety-target -q` passed:
  165 library tests plus all integration suites. One pre-existing unused-import
  warning remains in `tests/ipc_protocol.rs`.
- `git diff --check` passed before the three implementation commits.
- Site static QA passed: `node --check site/app.js`, `node --check site/ghost.js`,
  JSON-LD parsing, local resource resolution, screenshot-button count, and the
  inline-theme CSP hash all passed.
- Visual desktop/mobile screenshots could not be captured because the local
  file URL was blocked by the available browser policy. The code-level and
  static checks above are complete; perform a manual browser pass before a
  public site deployment if one is requested.

## Deferred by explicit scope

1. Merge `fix/cjk-emphasis-issue-6` into `main`.
2. Push the branch, tag a release, publish artifacts, or deploy the site.

## Planned next work — not started

**Full Mindmap Mode** should be an opt-in navigation mode, distinct from and
compatible with the existing document-level `ViewMode::Mindmap`. The user wants
folder selection, project-folder browsing, and file selection to be possible
end-to-end through a mindmap-style UI.

Start the next session with a design spec. It should define the activation and
exit UX, workspace-node model, file/folder selection actions, state ownership,
keyboard/panel behavior, and fallback to the current tree/picker/file-finder.
Reuse the current mindmap canvas where safe, but do not conflate document nodes
with filesystem nodes without a clear adapter and tests.

## Safe next action

Begin the Full Mindmap Mode design if the next user request confirms it. If the
user instead asks to merge or release, first re-check the branch against the
then-current `main`, rerun appropriate verification, and follow the release
workflow rather than relying on this historical snapshot.

## Maintenance rule

When status changes, update this file with: branch/commit context, dirty-file
ownership and intent, exact verification command/result, and the next concrete
action. Move items to “Completed and committed” only after they are committed
and name the commit.
