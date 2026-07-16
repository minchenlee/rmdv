# rmdv — shared project status

Last reconciled: 2026-07-16 (Asia/Taipei)

## Active work

- **Independently accepted locally (serial maker + lead, 2026-07-16):** commit
  `7e038ba` closes the delayed-reveal lifecycle gaps. Newly materialized
  `LowerBound(0)` children
  stay hidden in a deferred set without changing an active wave denominator;
  the current wave starts a fresh fixed follow-up after it drains. Collapsing a
  branch re-snapshots the other expanded frontier in the same update. Wave
  verification now retains only recursive count/status (`Verified`), while
  explicit expansion owns child/file materialization. Added regressions for
  deferred exact-empty pruning, fixed-cap overflow, collapse restart, stale
  prior-wave completion, and count-only verification. Focused gates pass:
  Full Mindmap app 47, workspace graph 17, all library 250, all integration
  tests 67, `cargo check`, touched-file rustfmt, and `git diff --check`. The
  fresh protected binary is `/private/tmp/mdv-full-mindmap-protect-target/debug/rmdv`
  with SHA-256
  `4f36fbc26f4ab1f52d6d3ad4d0f03a77d936fc0f835ff5baf023fcc2a2018298`.
  Lead review found no P0/P1 issues and independently reran 47 focused Full
  Mindmap tests, 17 workspace-graph tests, all 250 library tests, all 67
  integration tests, `cargo check`, `git diff --check`, and the binary hash.
  Native/manual acceptance remains pending; main/Zoom integration and release
  actions stay out of scope.

- **Completed locally (serial maker, 2026-07-16):** commit `56b44cb` adds the
  fixed four-worker/256-candidate delayed-reveal wave, strict
  request/root/filter/mode/parent-expansion identity, truthful exact /
  lower-bound / unavailable outcomes, and a separate neutral determinate
  progress toast beneath ordinary attention/error toasts. Focused gates pass:
  Full Mindmap app 43, workspace graph 17, all library 246, all integration
  tests 67, touched-file rustfmt, and `git diff --check`. The fresh protected
  binary is `/private/tmp/mdv-full-mindmap-protect-target/debug/rmdv` with
  SHA-256
  `61febf1b17d884377644816e055fdf1bf24e5386cd76d14f1932821b5168559e`.
  Native/manual acceptance remains the next gate; main/Zoom integration and
  release actions stay out of scope.

## Read this first

- Actual checkout: `/Users/liminchen/Documents/GitHub/mdv`
- Legacy non-repo path: `/Users/liminchen/Documents/GitHub/mdv-main`
- Active branch: `feat/full-mindmap-mode`; its latest independently accepted
  implementation candidate is `7e038ba` (`fix: close delayed reveal lifecycle
  gaps`).
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
10. **Full Mindmap manual-acceptance corrections** are committed as `972ebe2`,
   `888b1c6`, `a303d25`, and `8dc9ead`. Workspace Space now toggles the
   selected folder without moving selection, the workspace root retains its
   bounded supported-file count, hidden refreshes are additive and stale-safe
   across navigation, returning from a chooser cannot expose a stale Files
   sidebar, and ordinary home folders are ordered and positioned ahead of
   optional dot entries.
11. **Full Mindmap unified explorer candidate** is committed as `eeb9889`.
   It removes the user-visible chooser/workspace phase split, adopts the
   current file parent or Home through the background workspace loader when no
   project exists, makes Enter re-root folders, gives Right one-step
   expand-and-first-child behavior, and records per-folder recursive counts in
   the existing single bounded scan.
12. **Full Mindmap lazy file materialization candidate** is committed as
   `5a5fb3a`. The retained workspace tree now contains only the folder skeleton
   and recursive counts while the flat `workspace_files` index remains
   available to Cmd+P. Expanded folders load only their immediate supported
   files on a bounded background worker; collapse evicts the branch, and exact
   request/workspace/filter/expansion identity rejects stale results.
13. **Folder-only snapshot sidebar correction** is committed as `1d0b81a`.
   The ordinary Files sidebar transiently combines the folder skeleton with a
   second bounded path list from the same scan. This restores root/nested file
   rows, ordering, expansion visibility, cursor activation, dirty guarding,
   current-file reveal, and hidden refreshes without restoring retained file
   `Node`s or broadening the shallower Cmd+P/vault-search index.
14. **Files-sidebar pre-indexing correction** is committed as `5d421dc`.
   The bounded sidebar paths are grouped by parent and sibling-sorted once when
   the workspace snapshot is built. Repeated render and keyboard-navigation
   flattening now traverses only visible folder nodes and performs parent-local
   lookups, preserving the restored sidebar rows without per-call whole-index
   regrouping, permanent file `Node`s, or changes to Full Mindmap lazy loading.
15. **Full Mindmap native discovery correction** is committed as `1317a06`.
   Exact-empty subtrees are pruned even when an unrelated branch truncates.
   Expanding an interrupted/unreadable or lazily discovered shell starts one
   bounded background pass and retains only shallow immediate counted folders,
   immediate supported files, and truthful status; exact retained branches
   reuse the pre-grouped sidebar index without another filesystem scan.
   Collapse still evicts the branch, and exact request/root/filter/expansion/
   mode identity rejects stale completion.
16. **Full Mindmap child-focus correction** is committed as `400e41b`.
   Full Mindmap now publishes a graph-layout generation to the shared canvas,
   so async folder discovery recenters the acted-on folder or accepted first
   child at its final layout position instead of a transient parent/root seed.
   Document Mindmap leaves the generation unset and retains its prior behavior.
17. **Full Mindmap nearest-ancestor correction** is committed as `6f05ecf`.
   When lazy verification removes a selected exact-empty shell, selection and
   canvas focus now walk to the nearest still-visible folder ancestor; the
   workspace root is used only when no closer graph ancestor remains.
18. **Full Mindmap delayed reveal and verification progress** is committed as
   `56b44cb`. Unresolved `LowerBound(0)` shells on the visible expanded
   frontier are hidden during a fixed four-worker/256-candidate wave; accepted
   exact-positive, lower-bound, interrupted, unavailable, and exact-empty
   outcomes are truthful. A separate neutral determinate progress toast tracks
   checked/total/remaining without replacing ordinary attention/error toasts.
19. **Full Mindmap delayed-reveal lifecycle and count-only correction** is
   independently accepted at `7e038ba`. Branch loads defer newly visible
   unresolved shells behind a fixed follow-up wave, collapse re-snapshots
   remaining expanded parents, and verification retains only recursive
   count/status until a folder owns its explicit branch load. Lead review found
   no P0/P1 issues and independently passed the focused/full automated gates.
   Native/manual acceptance is still pending; no main integration, push, tag,
   release, or deploy was performed.

## Current state

- **Full Mindmap delayed reveal is independently accepted** at implementation
  commit `7e038ba` with no P0/P1 findings. Unsupported-only `LowerBound(0)`
  shells are hidden before rendering and exact-empty results never appear.
  Positive verification retains only recursive count/status; child and file
  listings are discarded until explicit expansion. Newly materialized child
  shells wait behind a fresh fixed follow-up wave, collapse re-snapshots other
  expanded parents without flashing them, and stale prior-wave completions
  cannot reveal nodes. The four-worker wave remains capped at 256 candidates;
  unqueued excess remains visible as `scan limit reached`. A separate neutral
  determinate progress toast shows checked/total/remaining beneath ordinary
  attention/error toasts. Lead evidence: 47 focused Full Mindmap tests, 17
  workspace-graph tests, all 250 library tests, all 67 integration tests,
  `cargo check`, `git diff --check`, and protected-binary SHA-256 verification.
  The manual candidate is
  `/private/tmp/mdv-full-mindmap-protect-target/debug/rmdv`, SHA-256
  `4f36fbc26f4ab1f52d6d3ad4d0f03a77d936fc0f835ff5baf023fcc2a2018298`.
  Native/manual acceptance remains pending; main/Zoom integration and release
  actions remain out of scope.

- **Full Mindmap nearest-ancestor focus correction is independently accepted**
  at implementation commit `6f05ecf` with no P0/P1 findings, after the
  metadata-only `/Shopee backroom`
  investigation on `feat/full-mindmap-mode`. The real path is
  `/Users/liminchen/Documents/Shopee Backroom`. A read-only production scan of
  its parent retained the folder as `LowerBound(0)` because the bounded
  ancestor scan was truncated; the same production `load_expanded_folder`
  retry returned `folders=0`, `files=0`, `Exact(0)`, `truncated=false` with
  hidden files both off and on. Metadata-only inspection found 20 directories,
  34 files, and no files in the current supported scope (`md`, `markdown`,
  `tex`, `json`, `yaml`, `yml`, `toml`, or feature-enabled `pdf`); excluded
  repository metadata accounts for the remaining entries. Exact-empty pruning
  therefore remains correct; keeping that folder would require a separate
  supported-file policy change. The candidate now walks the accepted graph's
  path ancestors after a selected shell is removed, preserving Documents when
  visible and using root only as final fallback. It adds app, graph, and canvas
  regressions while preserving graph-generation, Space/Right, bounds,
  stale-safety, sidebar, finder, and document Mindmap behavior. Focused gates
  pass: 36 canvas, 15 workspace-graph, 12 tree, 39 Full Mindmap app, and 9
  sidebar tests; all 240 library tests and all 67 integration tests also pass,
  alongside `cargo check`, fresh `cargo build --bin rmdv`, touched-file
  rustfmt, and `git diff --check` using
  `/private/tmp/mdv-full-mindmap-protect-target`. The fresh binary is
  `/private/tmp/mdv-full-mindmap-protect-target/debug/rmdv` with SHA-256
  `a5c8f0d39b9c4ae61749531b11c3cf4787c65d4da138ac84e37d6808adc86434`.
  Main/Zoom integration and release remain out of scope; native/manual
  acceptance is pending.

- **Full Mindmap child-focus correction is independently accepted** at
  implementation commit `400e41b` with no P0/P1 findings. It adds a Full Mindmap
  graph-generation focus signal to the shared canvas and deterministic
  regressions for async expansion and Loading-to-first-child replacement. The
  old transition was: selection stayed on the acted-on path while the graph
  rebuilt, so selection bookkeeping suppressed auto-center; a newly accepted
  child was also seeded at its parent animation position. The candidate now
  targets the rebuilt node layout and animates the transform with node motion.
  Main integration, Zoom retargeting, and release actions remain out of scope;
  native/manual acceptance of the corrected pan behavior is still pending.

- **Full Mindmap native correctness correction is independently accepted at
  `1317a06` with no P0/P1 findings.** The bounded retry lets a
  `scan limit reached` shell reveal useful immediate folders/files without
  re-rooting, or a stable local truncation/error status when discovery remains
  incomplete. Both initial and lazy exact-empty folders disappear; a lazy
  exact-empty selection normalizes safely. A wide simulated home scan proves a
  Documents-like late sibling remains reachable from an ancestor. Ordinary
  Files/Cmd+P, hidden/dirty safety, count labels, unified keyboard behavior,
  collapse eviction, and stale rejection remain covered. Main integration and
  Zoom retargeting remain prohibited pending native acceptance.

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
  screenshot repair. Full Mindmap is 9 main-only commits behind and contains
  the accepted implementation line through `7e038ba` plus its status commits;
  Zoom Controls is 9 main-only commits behind and has 10 branch-only commits.
  The Full Mindmap refinement is protected through `7e038ba`; integrate current
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
  preview, makes chooser `Space` descend while workspace `Space` toggles the
  selected folder without moving selection, makes `Enter` choose a folder in
  the chooser (or open a workspace file), makes a selected chooser folder show
  a background supported-file count capped at 5,000 files / 10,000 entries,
  keeps that bounded count on the selected workspace root, labels unreadable
  counts as unavailable rather than reporting a false empty
  folder, moves a workspace root’s `←` directly to its parent workspace graph
  without touching a dirty document, and gives Full Mindmap its own `⌘⌥W`
  panel-width cycle.
- Its committed performance hardening pass makes the folder skeleton, recursive
  counts, and file-finder index come from one pass capped at 12 levels, 5,000
  supported files, and 10,000 examined entries. Full Mindmap project changes
  run that pass off the UI thread with stale-result protection. At `eeb9889`,
  that same pass also stores recursive exact/lower-bound/unavailable counts on
  directory nodes; there are no Full-Mindmap-local per-folder scans. Cached
  workspace graphs and node vectors are shared by `Arc` instead of cloned each
  frame. Partial indexes render an explicit **More files not indexed** node.
- At `5a5fb3a`, files are no longer retained as `tree::Node`s. The initially
  expanded root and every ancestor needed for the current file queue bounded,
  non-recursive immediate-file loads. Pending folders render **Loading files…**;
  accepted files appear only under their expanded parent; local read/truncation
  outcomes render stable status children. Collapse discards accepted and
  pending materialization for the entire branch, and re-expansion creates a new
  request. Current-file reveal completes only after the parent listing accepts
  the file. Cmd+P continues to use the flat bounded file index.
- Lead review rejected `5a5fb3a` as-is because the shared ordinary Files
  sidebar still flattened only `workspace_tree`; once that tree became
  folder-only, file rows disappeared. `1d0b81a` closes the P1 with
  `workspace_sidebar_files`, a bounded lightweight path list through the
  historical tree depth, and `tree::flatten_with_files`. The sidebar creates
  transient file rows beneath visible/expanded parents. Cmd+P and vault search
  remain on the historical file-index depth, and Full Mindmap lazy ownership
  is unchanged.
- Lead review then rejected `1d0b81a` as-is on a second P1: every sidebar
  render/navigation flatten rebuilt a parent map from as many as 5,000 paths
  and re-sorted sibling groups, including lowercase-name allocations, on the
  UI thread. `5d421dc` closes that hot-path issue with a private immutable
  `SidebarFileIndex` built once with the bounded snapshot. Flattening now walks
  the visible folder skeleton and uses average O(1) parent-local lookups; the
  ordinary sidebar correctness restored by `1d0b81a` and Full Mindmap lazy
  ownership remain unchanged.
- At `1317a06`, the expanded-folder request owns shallow branch discovery in
  addition to immediate files. Initial pruning now removes every determinably
  `Exact(0)` subtree even if another branch exhausted the shared scan. A
  lower-bound/unreadable shell remains visible and, when expanded, receives a
  fresh bounded background count/tree pass; only its immediate folder nodes
  and files survive. A complete lazy `Exact(0)` result removes the shell,
  whereas an incomplete zero result renders **More items not indexed**.
  Already-exact retained nodes reuse their counted direct children and the
  pre-grouped sidebar paths without extra filesystem work.
- The feature source and design record are cleanly isolated through `400e41b`;
  this status reconciliation remains separate bookkeeping.
- Manual acceptance on 2026-07-15 passed A/B/C/D/E/G for entry/exit, preview,
  file open, dirty-edit protection, folder choice/count, and root-parent
  traversal. Commits through `8dc9ead` implement the four requested
  corrections: workspace Space toggles expansion, the root retains its count,
  hidden-on remains additive even if navigation occurs during its background
  refresh, and `/Users/liminchen/Documents` is discoverable from the home
  chooser. Targeted manual retest of these four corrections is still pending;
  main integration remains held.
- Follow-up manual testing accepted hidden-entry additivity and
  `/Users/liminchen/Documents` discovery. The user approved Luna's YES
  recommendation for no-project auto-adoption, and `eeb9889` implements the
  requested recursive collapsed-folder counts plus one folder-rooted explorer.
  Expanded folders use plain labels; collapsed exact folders show `N files`,
  interrupted folders show `N+ files`, zero-before-interruption shows `scan
  limit reached`, and unreadable folders show `count unavailable`.
- `eeb9889` removes Full Mindmap's `ChooseFolder` phase and chooser-local count
  requests. Space always folds without moving selection; Right expands and
  selects the first child; Enter opens a file or loads a folder as the new
  root; root Left loads the filesystem parent; Esc exits. Existing-project
  entry uses the accepted snapshot immediately. No-project entry indexes and
  adopts the current file parent or Home in the background.
- The unified/count candidate passed independent lead review with no P0/P1
  findings. Lead review rejected the first lazy-materialization commit
  `5a5fb3a` on the ordinary-sidebar P1 and rejected correction `1d0b81a` on the
  UI-thread regroup/sort P1. Follow-up `5d421dc` passed independent lead
  re-review with no remaining P0/P1 findings. Native testing then exposed the
  interrupted-shell, exact-empty, and ancestor-discovery failures; `1317a06`
  is the independently accepted correction awaiting native acceptance. Main
  integration remains held.
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
- The exact manual-correction candidate at `8dc9ead` passed
  `rustfmt --edition 2021 --check src/app.rs src/picker.rs src/tree.rs
  src/workspace_mindmap.rs`, `git diff --check`, 38 focused
  `full_mindmap_` tests, all 223 library tests, every integration target, and
  `cargo check` using `/private/tmp/mdv-full-mindmap-protect-target`. The
  integration run emitted only the pre-existing unused `Section` import
  warning in `tests/ipc_protocol.rs`. The rebuilt manual-test binary is
  `/private/tmp/mdv-full-mindmap-protect-target/debug/rmdv` with SHA-256
  `3098217566425624e52d8b516bc87c229717c0638bd20b8100f3d6becca293d4`.
- The exact lazy-materialization candidate at `5a5fb3a` passed 51 focused Full
  Mindmap/tree/graph tests (35 Full Mindmap, 9 workspace graph, and 7 tree), all
  221 library tests, all 67 integration tests, `cargo check`, `cargo build --bin
  rmdv`,
  `rustfmt --edition 2021 --check src/app.rs src/tree.rs
  src/workspace_mindmap.rs`, and `git diff --check` using
  `/private/tmp/mdv-full-mindmap-protect-target`. Focused regressions cover
  lazy loading/status, current-file reveal, collapse eviction and re-expansion,
  and stale folder results across hidden refresh, root switch, exit, and
  re-entry. The integration run emitted only the pre-existing unused `Section`
  import warning in `tests/ipc_protocol.rs`. The rebuilt manual-test binary is
  `/private/tmp/mdv-full-mindmap-protect-target/debug/rmdv` with SHA-256
  `240431cb09c36767d8ea5f6af6f79f32c2d79b5ec896099f727c6accd75089bc`.
- Despite those green gates, lead review rejected `5a5fb3a` on one P1: the
  folder-only retained tree removed every normal Files-sidebar file row because
  that shared consumer still used `tree::flatten` without the bounded flat
  files. The following correction evidence supersedes `5a5fb3a` for review.
- The exact sidebar correction at `1d0b81a` passed 36 focused Full Mindmap
  tests, 8 focused sidebar tests, 9 focused tree tests, all 225 library tests,
  all 67 integration tests, `cargo check`, `cargo build --bin rmdv`,
  `rustfmt --edition 2021 --check src/app.rs src/tree.rs
  src/workspace_mindmap.rs`, and `git diff --check` using
  `/private/tmp/mdv-full-mindmap-protect-target`. Regressions prove root and
  nested standard-sidebar file rows, collapsed hiding, historical folder/file
  ordering, keyboard activation through the dirty guard, hidden refresh, and
  distinct tree-depth sidebar versus shallower Cmd+P depth contracts. The
  integration run emitted only the pre-existing unused `Section` import
  warning in `tests/ipc_protocol.rs`. The rebuilt manual-test binary is
  `/private/tmp/mdv-full-mindmap-protect-target/debug/rmdv` with SHA-256
  `3bf11d125fac4eb857485f3c0d87dd1a00c47740b1784321bbc302149b497b13`.
- Despite those green gates, lead review rejected `1d0b81a` on one P1: each
  ordinary Files-sidebar flatten rebuilt and sorted a whole-index parent map
  on the UI thread. The following pre-indexing evidence supersedes `1d0b81a`
  for review while preserving its sidebar correctness coverage.
- The exact pre-indexing correction at `5d421dc` passed 36 focused Full Mindmap
  tests, 9 focused sidebar tests, 10 focused tree tests, all 226 library tests,
  all 67 integration tests, `cargo check`, `cargo build --bin rmdv`,
  `rustfmt --edition 2021 --check src/app.rs src/tree.rs
  src/workspace_mindmap.rs`, and `git diff --check` using
  `/private/tmp/mdv-full-mindmap-protect-target`. The added structural
  regression proves repeated flattening reuses the same pre-grouped,
  sibling-sorted parent slice rather than rebuilding it. The integration run
  emitted only the pre-existing unused `Section` import warning in
  `tests/ipc_protocol.rs`. The target was cleaned after generated artifacts
  filled the volume, then rebuilt successfully. The fresh manual-test binary
  is `/private/tmp/mdv-full-mindmap-protect-target/debug/rmdv` with SHA-256
  `aebba39fdf4b5ce34de25019121b1dea09e0d6cabc0ca74bae5ad5f7bdbbfb96`.
- A fresh lead-side review accepted `5a5fb3a..5d421dc` after verifying the
  folder-only snapshot, lazy file request/eviction generations, the separate
  depth-8 finder and depth-12 sidebar indexes, every ordinary Files consumer,
  and the private pre-grouped sidebar hot-path API. The lead independently
  reran 36 Full Mindmap, 9 sidebar, 10 tree, and 9 workspace-graph focused
  tests, all 226 library tests, every integration target (67 tests), `cargo
  check`, `cargo build --bin rmdv`, touched-file rustfmt, both implementation
  and status `git diff --check` ranges, and a fresh binary build. All passed
  with only the same pre-existing unused `Section` warning. The current manual
  binary SHA-256 is
  `885ab832225d71d48821419065a034ac49f58e084a5285ea5bcaccc2610058f1`.
- The exact native-discovery correction at `1317a06` passed 38 focused Full
  Mindmap app tests, 14 workspace-graph tests, 12 tree tests, 9 focused sidebar
  tests, all 235 library tests, and all 67 integration tests. `cargo check`,
  `cargo build --bin rmdv`, `rustfmt --edition 2021 --check src/app.rs
  src/tree.rs src/workspace_mindmap.rs`, and `git diff --check` passed using
  `/private/tmp/mdv-full-mindmap-protect-target`. Regressions cover
  exact-empty pruning under unrelated truncation, lazy exact-empty selection
  normalization, interrupted-zero expansion yielding useful folders/files or
  a truthful terminal status, a wide Documents-like late sibling recovered
  from an ancestor shell, exact retained-index reuse, collapse/re-expand
  eviction, and stale completion across hidden filter, root, exit/re-entry,
  and request generations. Existing hidden/dirty and ordinary-sidebar tests
  remain green. The integration run emitted only the pre-existing unused
  `Section` import warning in `tests/ipc_protocol.rs`. The target was cleaned
  of 7.0 GiB generated artifacts and rebuilt fresh; the manual binary is
  `/private/tmp/mdv-full-mindmap-protect-target/debug/rmdv` with SHA-256
  `d281e3970ffc9b979e210103e236e4a1b244429fb20af6f1782679f247464853`.
- The exact child-focus candidate at `400e41b` passes 34 shared-canvas mindmap
  tests, 40 focused Full Mindmap app tests, all 237 library tests, all 67
  integration tests, `cargo check`, a fresh `cargo build --bin rmdv`, touched-
  file rustfmt, and `git diff --check` using
  `/private/tmp/mdv-full-mindmap-protect-target`. Regressions cover a selected
  folder surviving async pending-to-accepted relayout and a Right-driven
  Loading status replacement focusing the accepted first child instead of the
  workspace root. The fresh binary is
  `/private/tmp/mdv-full-mindmap-protect-target/debug/rmdv` with SHA-256
  `a6767e77ac2b58b0d6b78cd84760220696128452060ec72ddefdd9b1c0d57437`.
  Native/manual acceptance remains pending; do not integrate to main or
  release until that gate is complete.
- A fresh lead-side review of `473170f..400e41b` found no P0/P1 issue after
  checking Full-Mindmap-only generation ownership, final-layout focus targets,
  one-shot generation bookkeeping, pan/node animation synchronization, and the
  unchanged document-Mindmap path. The lead independently reran 34 canvas/
  mindmap, 40 Full Mindmap, 14 workspace-graph, and 9 sidebar tests plus
  touched-file rustfmt, implementation/status diff checks, worktree cleanliness,
  and the final binary SHA-256. All passed.
- A fresh lead-side review of `e991a67..6f05ecf` found no P0/P1 issue after
  checking graph-root boundaries, path-parent walking, deferred-file selection,
  exact-empty-only normalization, root final fallback, and generation-driven
  canvas focus on the accepted ancestor. The lead independently reran 36
  canvas/mindmap, 42 Full Mindmap, 15 workspace-graph, 12 tree, and 9 sidebar
  tests plus touched-file rustfmt, implementation/status diff checks, worktree
  cleanliness, and the final binary SHA-256. All passed.
- A fresh lead-side static review of `3e2b3fd..1317a06` found no P0/P1 issue
  after checking exact-empty pruning, branch-local shallow ownership, bounded
  background execution, exact request/root/filter/expansion/mode rejection,
  collapse eviction, selection normalization, and preservation of the
  pre-grouped sidebar and Cmd+P indexes. The lead independently reran 38 Full
  Mindmap, 14 workspace-graph, 12 tree, and 9 sidebar focused tests, all 235
  library tests, every integration target (67 tests), `cargo check`, `cargo
  build --bin rmdv`, touched-file rustfmt, implementation/status diff checks,
  worktree cleanliness, and the binary SHA-256. All passed with only the same
  pre-existing unused `Section` warning.
- A fresh independent re-review of `8dc9ead` returned PASS with no P0/P1
  findings after checking both completion orders, stale request rejection,
  dirty and exit intent, and accepted refresh failure fallback.
- The exact unified-explorer candidate `eeb9889` passed touched-file
  `rustfmt --edition 2021 --check`, `git diff --check`, 34 focused Full Mindmap
  app tests, 5 bounded workspace-snapshot tests, all 7 workspace-graph tests,
  all 216 library tests, every integration target (67 tests), `cargo check`,
  and `cargo build` using
  `/private/tmp/mdv-full-mindmap-protect-target`. The only integration warning
  remains the pre-existing unused `Section` import in
  `tests/ipc_protocol.rs`. The fresh manual binary is
  `/private/tmp/mdv-full-mindmap-protect-target/debug/rmdv` with SHA-256
  `bb9f1ba7d4db158946032a455d941d386d288b29d6ec21dd354c53de8f0fbf05`.
- A fresh lead-side review of `76e8db3..eeb9889` found no P0/P1 issues after
  checking both entry paths, single-pass recursive count propagation, exact and
  lower-bound labels, background request identity, hidden refresh completion
  orderings, workspace re-rooting, previews, and dirty-document guards. The
  lead independently reran the 34 Full Mindmap app tests, 5 bounded-snapshot
  tests, 7 workspace-graph tests, all 216 library tests, every integration
  target (67 tests), `cargo check`, `cargo build`, touched-file rustfmt, both
  feature/status `git diff --check` ranges, and the binary SHA-256; all passed
  with only the same pre-existing unused `Section` warning.
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
7. **P2 — Extremely wide directory discovery.** A directory with more than
   10,000 immediate entries is explicitly truncated before sorting, so a later
   ordinary sibling is not guaranteed to enter the bounded snapshot. The real
   `/Users/liminchen` home currently has 116 immediate entries and is not
   affected; revisit only if broader guarantees are required.
8. **P3 — Repository formatting/Clippy debt.** Keep as non-blocking hygiene.
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
Commits through `7e038ba` remove its visual controls, make folder traversal and
file opening keyboard-first, address the manual-acceptance corrections, harden
large-workspace behavior, unify both entry scenarios around one explorer, add
recursive collapsed-folder count labels from the bounded snapshot, lazily
materialize expanded-folder files, and preserve the ordinary Files sidebar
through a pre-grouped bounded path index. Interrupted folder shells can also
acquire shallow counted children through branch-local bounded background
discovery, while exact-empty subtrees are omitted.

The implementation is recorded in
`docs/superpowers/specs/2026-07-10-full-mindmap-mode-design.md` and covers
activation/exit UX, path-based workspace nodes, keyboard and panel behavior,
dirty and late-async protection, shared-canvas adapter boundaries, fallback
picker/tree/file-finder paths, and focused tests.

## Safe next action

For the P0 fixes, run Windows CI/cross-target verification when available and
push local `main` only on an explicit request.
For Full Mindmap, manually exercise both entry scenarios, recursive
exact/lower-bound labels, Space/Right/Enter/Left/Esc, hidden refreshes, previews,
dirty-document protection, and ordinary Files-sidebar scrolling/navigation in
a large workspace with the recorded binary. Specifically verify that a
unsupported-only folders such as `Shopee Backroom` never flash into the graph,
the progress toast advances and disappears, capped unverified excess remains
truthfully labeled `scan limit reached`, exact-empty folders are absent,
and nested folders beneath Documents are reachable from Home or an ancestor
without first making Documents the root. Expand a child beneath Documents with
Space and Right and confirm the viewport remains focused on that folder/child
after Loading completes rather than jumping to the user root. A/B/C/D/E/G and
hidden additivity were accepted on earlier candidates. Expanding
`Shopee Backroom` should not appear at all because it has no supported
descendants. Ordinary attention/error toasts must remain readable above the
progress toast. `1317a06`, `400e41b`, `6f05ecf`, and `7e038ba` passed
independent automated review and still need native acceptance.
Do not integrate main while that gate is held.
After acceptance,
integrate current
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
