# rmdv — shared project status

Last reconciled: 2026-07-15 (Asia/Taipei)

## Read this first

- Actual checkout: `/Users/liminchen/Documents/GitHub/mdv`
- Legacy non-repo path: `/Users/liminchen/Documents/GitHub/mdv-main`
- Active branch: `feat/full-mindmap-mode`; its latest feature commit is
  `82afd5a` (`feat: refine full mindmap navigation`).
- Local `main` is at `67564e5`, eleven commits ahead of `origin/main`: Windows
  IPC fix `6fa6450`, CJK emphasis fix `0df1fe2`, reviewed CJK repair `d97370e`,
  the six-commit reviewed Zen feature/repair line `1199455..f2b0519`, and Zen
  editor/toast polish `68fc8d0`, followed by reliable CLI screenshots
  `67564e5`.
  `origin/main` and released tag `v0.4.0` remain at `34d352d`.
- Three worktrees are currently registered: the active checkout above, the
  clean `feat/mindmap-zoom-controls` worktree, and
  `.claude/worktrees/zen-ui-polish` on `codex/zen-ui-polish`, clean at
  `68fc8d0` and one commit behind local `main` after the screenshot repair.

## Completed and committed

1. **v0.4.0 PDF viewing release** is on `main` and tagged `v0.4.0`.
   It includes local PDF-to-Markdown viewing, PDFium packaging for macOS and
   Linux, and associated site/demo/docs work. Windows deliberately builds
   without the PDF feature.
2. **Windows IPC lifetime fix** `6fa6450` is merged into local `main`. Static
   review against `interprocess` 2.4.2 found the owned `Name<'static>` path
   correct; a Windows Rust target was unavailable for local cross-compilation.
3. **CJK emphasis rendering** `0df1fe2` plus regression repair `d97370e` are
   merged into local `main`. The repair preserves authored U+200B/private-use
   content, reserves decoded numeric entities, cleans synthetic markers from
   link/image destinations, and bounds marker selection to O(source length).
4. **Zen edit mode and unsaved-edit protection** are merged into local `main`
   as the six-commit line `1199455..f2b0519`. The final repair makes repeated
   entry idempotent, serializes saves, rejects stale file loads by request and
   document revision, scopes pending navigation to its load, keeps pending
   writes behind the file-switch guard, and preserves native non-macOS
   Ctrl-arrow behavior. Two independent review passes ended with no findings
   and both readiness gates set to YES.
5. **README and release-history cleanup** is committed as `739ad4e`
   (`docs: clarify capabilities and archive v0.4 audit`). The README now
   separates historical benchmarks from current claims, accurately describes
   PDF/Windows support, and the v0.4 audit is explicitly historical.
6. **Landing-site simplification and hardening** is committed as `7a02d3f`
   (`style(site): simplify layout and fix accessibility`). It removes the
   carousel/reveal code, fixes the inline-theme CSP hash, makes screenshots
   keyboard-operable, preserves FAQ Space-key behavior, and updates release
   metadata.
7. **Zen editor and toast polish** is committed as `68fc8d0`
   (`feat: polish Zen editor feedback`) and fast-forwarded into local `main`.
   It removes Zen editor vertical padding, introduces neutral/default,
   guidance, and accent-attention toast profiles, routes blocked and failed
   actions to attention feedback, prefixes failures with `⚠`, and records the
   product, design, and native UI screenshot-testing contracts.
8. **Full Mindmap keyboard and performance refinement** is protected as
   `82afd5a` (`feat: refine full mindmap navigation`). It adds bounded
   background workspace indexing and folder counts, stale-result rejection,
   shared graph allocations, explicit truncation status, read-only previews,
   direct parent-workspace navigation, and the reviewed keyboard-first flow.
9. **Reliable CLI screenshots** are committed as `67564e5`
   (`fix: make CLI screenshots reliable`) and fast-forwarded into local
   `main`. The fix waits for a render settle, preserves IPC request identity,
   rejects overlapping captures, validates near-black frames, retries three
   times, and reports explicit failure instead of writing a blank PNG.

## Current state

- **Zen UI polish is implemented, verified, committed, and merged into local
  `main` as `68fc8d0`.** Zen editor vertical padding is removed; toast feedback
  now has neutral 1.5-second, neutral guidance 2.5-second, and accent attention
  3.5-second profiles. Unsaved-edit guards, ignored external changes, PDF edit
  refusal, screenshot failure, and update failure use attention feedback;
  failures also carry a `⚠` text marker. `PRODUCT.md` and `DESIGN.md` record the
  approved product and toast hierarchy. `docs/ui-toast-screenshot-testing.md`
  records the native macOS UI-test and screenshot workflow. The isolated Zen
  worktree is clean and points at the same commit as local `main`.

- **Zen mode is merged and review-clean on local `main` at `f2b0519`.** The
  reviewer first found and blocked a pending-save navigation transition after
  the four original fixes; the follow-up guard and regression test closed it,
  and the final re-review returned `FEATURE_READY: YES` and
  `MERGE_READY_INTO_CURRENT_MAIN: YES` with no findings.

- **The P0 fixes are merged into local `main` at `d97370e`.** The first review
  blocked the original CJK commit on authored-marker and destination corruption;
  a second adversarial pass then caught numeric-entity collision and O(6400*N)
  marker selection. `d97370e` fixes all four issues, passed the final review,
  and was fast-forwarded only after the exact candidate passed its test gates.
  Nothing was pushed.
- **CLI screenshot reliability is accepted on local `main@67564e5`.** The
  isolated maker commit received a fresh lead review, deterministic tests, and
  a native 30-capture probe. The temporary worktree was removed after the
  fast-forward; branch `codex/fix-cli-screenshots` remains as a clean reference.
- `feat/full-mindmap-mode` and `feat/mindmap-zoom-controls` still follow the old
  `0df1fe2` line and do not contain repair `d97370e`, the Zen feature, or the
  screenshot repair. Full Mindmap is 9 main-only commits behind and has 11
  branch-only commits; Zoom Controls is 9 main-only commits behind and has 10
  branch-only commits. The Full Mindmap refinement is protected at `82afd5a`;
  integrate current
  `main@67564e5` only after the requested manual acceptance, then retest.
- **Mindmap Zoom Controls remains clean at `46e3a6b` but is blocked from a
  direct rebase onto `main`.** Its commit directly uses Full Mindmap state and
  canvas APIs that do not exist on `main`; retarget it only after main is
  integrated into the protected Full Mindmap branch. No rebase was performed.

- **Full Mindmap Mode is committed as `ae0b4a8`.** It uses
  `FullMindmapState` and path-based `WorkspaceNodeId`s to keep project
  navigation independent of document `ViewMode::Mindmap`, `BlockId`, document
  collapse, and document-layout state.
- The committed keyboard-first refinement removes the Full Mindmap
  toolbar and action buttons, gives selected files a read-only async content
  preview, makes `Space` descend folders, makes `Enter` choose a folder in the
  chooser (or open a workspace file), makes a selected chooser folder show a
  background supported-file count capped at 5,000 files / 10,000 entries,
  labels unreadable counts as unavailable rather than reporting a false empty
  folder, moves a workspace root’s `←` directly to its parent workspace graph
  without touching a dirty document, and gives Full Mindmap its own `⌘⌥W`
  panel-width cycle.
- Its committed performance hardening pass makes workspace tree and file-finder
  data come from one pass capped at 12 levels, 5,000
  supported files, and 10,000 examined entries. Full Mindmap project changes
  run that pass off the UI thread with stale-result protection; folder counts
  use a safe 100 ms debounce; cached workspace graphs and node vectors are
  shared by `Arc` instead of cloned each frame. Partial indexes render an
  explicit **More files not indexed** node.
- The feature source and design record are cleanly isolated in `82afd5a`; this
  status reconciliation is intentionally separate bookkeeping.
- Do not merge, push, tag, release, or deploy without a new explicit request.

## Verification evidence

- The exact Zen UI polish commit `68fc8d0` passed focused toast/profile, PDF refusal,
  dirty external-change, Zen padding, and dirty late-load tests. It also passed
  `cargo check`, `cargo build`, `git diff --check`, all 182 library tests, and
  all 67 integration tests using `/private/tmp/mdv-zen-fix-target`. The only
  warning remains the pre-existing unused `Section` import in
  `tests/ipc_protocol.rs`. The rebuilt test binary is
  `/private/tmp/mdv-zen-fix-target/debug/rmdv`.
- Seven toast states were exercised through an isolated, uniquely identified
  macOS test bundle and captured through the rmdv CLI. Every retained frame was
  inspected for the correct current toast, semantic styling, and non-black app
  background. The temporary capture command was removed; the product diff hash
  returned exactly to
  `6f6605783752bc017a3de67a112f9b61c8d2d4c398b6edba12e75f96df12546e`,
  `git diff --check` passed, and the reopened final binary has no demo-toast
  command. The selected evidence is under the current Codex visualization
  directory's `rmdv-toasts/` folder.
- The exact Zen candidate at `f2b0519` passed `cargo check`, `cargo build`,
  `git diff --check`, 179 library tests, and all 67 integration tests. Focused
  regressions cover repeated Zen entry, platform-specific arrow bindings,
  serialized and failed/queued saves, stale load after edit + successful save,
  and navigation attempted after reverting to the old baseline while a newer
  write is still pending. The only test warning is the pre-existing unused
  `Section` import in `tests/ipc_protocol.rs`.
- The final independent re-review returned no findings and both
  `FEATURE_READY: YES` and `MERGE_READY_INTO_CURRENT_MAIN: YES`. Local `main`
  fast-forwarded from `d97370e` to the exact reviewed commit `f2b0519`; a
  post-merge focused pending-save navigation test also passed.
- The isolated Zen review covered exactly `0df1fe2..f49f909`; range
  `git diff --check` passed. Focused tests passed (6 Zen, 3 editor keybinding),
  as did the exact-snapshot 165 library tests and 67 integration tests. Those
  tests did not cover the three P1 races or non-mac Ctrl+arrow behavior, so
  that historical candidate was blocked; the final evidence above supersedes
  this earlier result.
- The final isolated P0 candidate was exactly `34d352d` -> `6fa6450` ->
  `0df1fe2` -> `d97370e`; `git diff --check origin/main..main` passed and local
  `main` is clean at that commit.
- Exact-candidate tests passed: 30 focused parser tests, 159 library tests, and
  67 integration tests. `rustfmt --edition 2021 --check src/parser.rs` and
  `git diff --check` passed. The only warning was the pre-existing unused
  `Section` import in `tests/ipc_protocol.rs`.
- Windows cross-compilation was unavailable because this Homebrew Rust setup
  has no Windows standard library/rustup target. Static review against
  `interprocess` 2.4.2 confirmed that passing an owned `String` selects the
  owning `ToNsName` path and produces a valid `Name<'static>`.
- `cargo test --target-dir /private/tmp/mdv-zen-safety-target -q` passed:
  165 library tests plus all integration suites. One pre-existing unused-import
  warning remains in `tests/ipc_protocol.rs`.
- `git diff --check` passed before the three implementation commits.
- `cargo test --lib --target-dir /private/tmp/mdv-full-mindmap-target -q`
  passed before this refinement: 189 library tests.
- `cargo test --tests --target-dir /private/tmp/mdv-full-mindmap-target -q`
  passed before this refinement: 189 library tests plus all integration suites. The same pre-existing
  unused `Section` import warning remains in `tests/ipc_protocol.rs`.
- `cargo test --lib --target-dir /private/tmp/mdv-full-mindmap-refine-target -q`
  passed: 202 library tests.
- `cargo test --tests --target-dir /private/tmp/mdv-full-mindmap-refine-target -q`
  passed: 202 library tests plus all integration suites. The same pre-existing
  unused `Section` import warning remains in `tests/ipc_protocol.rs`.
- `cargo test --target-dir /private/tmp/mdv-full-mindmap-refine-target
  full_mindmap_ -q` passed: 26 focused Full Mindmap tests.
- `cargo check --target-dir /private/tmp/mdv-full-mindmap-refine-target -q`
  passed.
- `cargo test --lib --target-dir /private/tmp/mdv-full-mindmap-perf-target -q`
  passed: 209 library tests.
- `cargo test --tests --target-dir /private/tmp/mdv-full-mindmap-perf-target -q`
  passed: 209 library tests plus all integration suites. The same pre-existing
  unused `Section` import warning remains in `tests/ipc_protocol.rs`.
- `cargo test --lib --target-dir /private/tmp/mdv-full-mindmap-perf-target
  full_mindmap_ -q` passed: 29 focused Full Mindmap tests.
- `cargo check --target-dir /private/tmp/mdv-full-mindmap-perf-target -q`
  passed.
- The exact protected refinement at `82afd5a` passed
  `rustfmt --edition 2021 --check src/app.rs src/picker.rs src/tree.rs
  src/workspace_mindmap.rs`, `git diff --check`, 209 library tests, all
  integration targets, 29 focused `full_mindmap_` tests, and `cargo check`
  using `/private/tmp/mdv-full-mindmap-protect-target`. The integration run
  emitted only the pre-existing unused `Section` import warning in
  `tests/ipc_protocol.rs`.
- The exact screenshot repair `67564e5` passed 5 focused screenshot tests, all
  187 library tests, `cargo check`, and `git diff --check` using
  `/private/tmp/mdv-zen-fix-target`. A native isolated probe captured 30/30
  valid 2048x1536 non-black frames; sampled dark frames had 17,435 colors and
  mean intensity 0.400521, and an immediate One Light transition captured the
  light UI. Evidence remains in
  `/private/tmp/rmdv-cli-screenshot-probe-output`. Repository-wide
  `rustfmt --check` still reports the pre-existing `src/app.rs` baseline debt;
  no new screenshot or test region appears in that output.
- Remaining screenshot limitation: Iced's offscreen capture can omit Zen
  `text_editor` content while retaining the app background/footer. This is not
  the intermittent black-frame defect and is tracked below rather than hidden
  as completed work.
- `cargo build --release --target-dir
  /private/tmp/mdv-full-mindmap-refine-target -q` passed; the optimized local
  binary is `/private/tmp/mdv-full-mindmap-refine-target/release/rmdv`.
- A full `cargo test` attempt could not finish because the target filesystem
  had only 1.0 GiB free and linking `pdf_smoke` failed with `errno=28`; the
  library and integration suites above completed successfully.
- `rustfmt --edition 2021 --check src/picker.rs src/workspace_mindmap.rs`
  passed, and `git diff --check` passed. Repository-wide `cargo fmt --check`
  and strict Clippy are not clean because the repository already has broad
  formatting/lint debt (and test-style lint output); do not claim a clean
  baseline from this feature work.
- The focused headless Iced view test for both Full Mindmap phases passed.
  Native desktop visual automation was unavailable because the local
  Computer Use service timed out; perform a manual interaction pass before a
  future release or visual-polish request.
- Site static QA passed: `node --check site/app.js`, `node --check site/ghost.js`,
  JSON-LD parsing, local resource resolution, screenshot-button count, and the
  inline-theme CSP hash all passed.
- Visual desktop/mobile screenshots could not be captured because the local
  file URL was blocked by the available browser policy. The code-level and
  static checks above are complete; perform a manual browser pass before a
  public site deployment if one is requested.

## Prioritized backlog

1. **P1 — Windows build verification.** Run `6fa6450` through a pushed Windows
   CI candidate before the next release. Local verification was static only,
   and `.github/workflows/release.yml` currently marks the Windows job
   `continue-on-error`.
2. **P1 — Search/highlight memory bounds.** Implement a fresh capped-search and
   highlight byte-budget pass after current feature integration. `find_all` /
   `find_in_blocks` currently collect unbounded match vectors, while `HlCache`
   caps entry count but not source bytes or total memory. Do not cherry-pick the
   stale archived memory branch wholesale.
3. **P2 — Image-only PDF feedback.** Detect successful PDF extraction with
   empty text and show clear OCR-disabled guidance instead of a blank document;
   adding OCR is not part of this item.
4. **P2 — Full Mindmap discoverability.** After manual acceptance and merge,
   add Full Mindmap to the README feature/shortcut tables and the in-app
   shortcut overlay. Do not publish it as shipped before acceptance.
5. **P2 — Zen editor screenshot coverage.** Investigate the Iced offscreen
   `text_editor` omission separately from black-frame retry reliability.
6. **P2 — Stale documentation reconciliation.** Correct the Zoom worktree
   status that still calls `46e3a6b` uncommitted, the resolved fullscreen-exit
   note in the KB-hints spec, and the outdated measured-height statement in
   `docs/benchmarks.md` as their owning branches are next touched.
7. **P3 — Repository formatting/Clippy debt.** Keep as non-blocking hygiene.
   README already tracks PDF/HTML export and additional tree-sitter grammars;
   do not create duplicate backlog entries for them.

## Deferred by explicit scope

1. Integrate current local `main` at `67564e5` into `feat/full-mindmap-mode`
   only after the user's manual Full Mindmap acceptance, then re-review and
   retest that feature.
2. Rebase/retarget `feat/mindmap-zoom-controls` onto the resulting updated Full
   Mindmap tip, then review and retest it. Direct rebase onto `main` is invalid.
3. Push the branch, tag a release, publish artifacts, or deploy the site.

## Protected Full Mindmap refinement — awaiting main integration

The Full Mindmap feature is an opt-in, full-window navigation mode, distinct
from and compatible with the existing document-level `ViewMode::Mindmap`.
Commit `82afd5a` removes its visual controls and makes folder traversal and
file opening keyboard-first while hardening large-workspace behavior.

The implementation is recorded in
`docs/superpowers/specs/2026-07-10-full-mindmap-mode-design.md` and covers
activation/exit UX, path-based workspace nodes, keyboard and panel behavior,
dirty and late-async protection, shared-canvas adapter boundaries, fallback
picker/tree/file-finder paths, and focused tests.

## Safe next action

For the P0 fixes, run Windows CI/cross-target verification when available and
push local `main` only on an explicit request.
For Full Mindmap, collect the user's A-H manual acceptance result first. Do not
integrate main while that gate is held. After acceptance, integrate current
`main@67564e5`, rebuild and repeat the large-folder interaction; only then
retarget and review Zoom Controls.
If the user instead asks to merge or release, first re-check
the branch against the then-current `main`, rerun appropriate verification, and
follow the release workflow rather than relying on this historical snapshot.

## Maintenance rule

When status changes, update this file with: branch/commit context, dirty-file
ownership and intent, exact verification command/result, and the next concrete
action. Move items to “Completed and committed” only after they are committed
and name the commit.
