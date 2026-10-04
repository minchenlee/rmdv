# rmdv demo — recording script

A hand-driven tour. Record with a keystroke overlay on (so viewers see the
shortcuts). Each beat = **press → what appears → hold**. Whole run ≈ 2–3 min.

**Before you hit record**
- Build + launch on the demo folder: `cargo build --release && ./target/release/rmdv demo/`
- Pick a clean theme; full-screen or a fixed window size.
- Turn on a keycast overlay (e.g. KeyCastr) so ⌘-combos show on screen.
- Start with the **sidebar visible** (⌘B toggles) and README open.

---

## 0. Cold open — the vault (≈8s)

- **Show:** README rendered, sidebar tree on the left, breadcrumb up top.
- **Beat:** slow-scroll the README Tour table once. This is the map of the whole demo.
- **Say:** "One folder, opened as a vault. Native Rust — no browser, no Electron."

## 1. Workspace navigation (≈12s)

- **Press** `⌘P` → fuzzy file jumper. Type `oauth` → Enter.
- **Show:** lands on `reference/api/v2/auth/oauth.md`. Point at the **breadcrumb** —
  four levels deep (`reference / api / v2 / auth`).
- **Press** Home / End or `g` / `G` → reader top/bottom. Outside Zen, `⌘↑` /
  `⌘↓` cycle Quick Slots; Zen keeps Command arrows for native document motion.
- **Say:** "Fuzzy-jump anywhere, deep trees, heading-to-heading nav."

## 2. Markdown kitchen sink (≈18s)

- **Click** `guide/features/markdown/syntax.md` in the sidebar.
- **Scroll** top→bottom slowly. Pause on:
  - the **table**, the **task list** (checked/unchecked boxes),
  - **nested lists** (three deep), the **blockquote**,
  - the **code blocks** — Rust, Python, C++, Java, SQL — each syntax-highlighted.
- **Say:** "Every markdown block. Real tree-sitter highlighting, six languages here."

## 3. Diagrams — Mermaid + DOT (≈15s)

- **Click** `guide/features/diagrams/mermaid/flowcharts.md`.
- **Show:** flowchart, sequence, state diagram — all rendered inline.
- **Click** the in-page link **"Graphviz DOT →"** (or sidebar → `graphviz/dot.md`).
- **Show:** DOT dependency graph + state machine rendered.
- **Say:** "Mermaid and Graphviz, rendered natively. Zero JavaScript."

## 4. Math (≈10s)

- **Click** `guide/features/math/equations.md`.
- **Show:** quadratic formula, Euler's identity, a sum, a **matrix**, a binomial,
  blackboard ℝ⊂ℂ. Scroll through them.
- **Say:** "Block LaTeX via a pure-Rust layout engine."

## 5. Full LaTeX document (≈8s)

- **Click** `papers/research/relativity.tex`.
- **Show:** a whole `.tex` file rendered — sections, numbered equations (Lorentz, E=mc²).
- **Say:** "Not just fenced math — whole .tex documents render too."

## 5b. PDF as Markdown (≈8s)

- **Click** `papers/research/NIST.SP.800-63-4-excerpt.pdf`.
- **Show:** a real published PDF (NIST Digital Identity Guidelines, public domain) opens as rendered Markdown — headings, author list, tables preserved.
- **Say:** "PDFs too — text extracted locally with PDFium, no cloud, no LLM. View-only."

## 6. Images + zoom (≈10s)

- **Click** `guide/features/images/gallery.md`.
- **Show:** local icon + remote logos load inline. The broken path degrades cleanly.
- **Press/Click** an image → **zoom modal** opens. Scroll to zoom, drag to pan, `Esc` to close.
- **Say:** "Local and remote images, click to zoom."

## 7. Document mind map — ⌘M (≈14s)

- **Click** `guide/features/mindmap/document-mindmap.md` (the Product Roadmap).
- **Press** `⌘M` → document folds into a **mind map**; every heading is a node.
- **Beat:** arrow-key `← ↑ → ↓` to walk the tree; the preview panel follows the focus.
  Widen/narrow the panel if you want (shows the snap widths).
- **Press** `⌘M` again → back to rendered view.
- **Say:** "Any document, folded into a mind map. Arrow keys walk it."

## 8. Data mind map — JSON / YAML (≈16s)

- **Click** `reference/data/config.json` → JSON renders.
- **Press** `⌘M` → **data mind map** of the JSON tree (server, auth, scopes, rate_limits).
- Walk a branch with the arrows; the subtree panel shows values.
- **Click** `reference/data/settings.yaml` → **press** `⌘M` again.
- **Beat:** open the `http_status_messages` / `feature_flags` branches — note the
  **non-string keys** (`200`, `true`) resolve correctly in the panel.
- **Say:** "Same mind map for data — JSON and YAML, even non-string keys."

## 9. Edit mode — ⌘E (≈10s)

- Back on any markdown page (e.g. `syntax.md`).
- **Press** `⌘E` → raw source with live, theme-aware highlighting.
- **Type** a quick edit (add a heading or a list item) so the live highlight is visible.
- **Press** `⌘S` to save (or `Esc` to drop) → back to rendered view, change shown.
- **Say:** "Edit in place, live highlighting, save."

## 10. Vault-wide search — ⌘⇧F (≈14s)

- **Press** `⌘⇧F` → full-page search (Zed-style, not a floating box).
- **Type** `mindmap` → results across the whole `demo/` tree, grouped by file.
- **Arrow** down a couple results, **Enter** → page replaces with that file at the hit.
- **Press** `Esc` to exit search.
- **Say:** "Search every file in the vault. Enter opens, Esc exits."

## 11. Themes (≈10s) — optional closer

- Open the command palette, run **Open Themes Folder** (shows the base16 themes live).
- Switch a theme; **show** the whole UI — including the doc and any open mind map —
  recolor instantly. (The update banner, if it shows, recolors too.)
- **Say:** "Drop in any base16 theme. Everything recolors."

## 12. Close (≈5s)

- Return to the README. Let the Tour table sit on screen.
- **Say:** "rmdv. Markdown, diagrams, math, mind maps — native and fast."

---

### Shortcut cheat (keep handy while recording)

| Key | Does |
|-----|------|
| `⌘O` | Open file / folder |
| `⌘P` | Fuzzy file jump |
| `⌘B` | Toggle sidebar |
| `⌘M` | Mind map (markdown → doc map; json/yaml → data map) |
| `⌘E` / `⌘S` | Edit / save |
| `⌘⇧F` | Vault search |
| Home / End or `g` / `G` | Reader top / bottom |
| `⌘↑` / `⌘↓` | Previous / next Quick Slot outside Zen; native document motion in Zen |
| `← ↑ → ↓` | Walk a mind map |
| `⌘=` / `⌘−` | Text zoom |
| `Esc` | Close modal / exit search / drop edit |

### Recording order rationale
Reading → rendering richness (md, diagrams, math, tex, images) → structure views
(doc mindmap, data mindmap) → interaction (edit, search) → polish (themes). Each
beat introduces exactly one new capability; no shortcut shown twice cold.
