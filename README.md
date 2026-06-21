# rmdv

**Native Rust markdown viewer — and the only one a coding agent can drive.**

[rmdv.mclee.dev](https://rmdv.mclee.dev) · [Download](https://github.com/minchenlee/rmdv/releases) · MIT

rmdv (Rust Markdown Viewer) is a fast, read-focused desktop app for browsing folders of `.md` files. It renders **Markdown, [Mermaid](https://mermaid.js.org/) diagrams, Graphviz DOT graphs, block LaTeX math, JSON/YAML mind maps, and PDFs** natively — no Electron, no embedded browser, no JavaScript runtime. A single ~33 MB static binary opens to first paint in **~150 ms**.

Its differentiator: a **scriptable IPC socket**. Any program or AI agent — Claude Code, Codex, Cursor, a shell script — can drive the running window (open files, scroll to a section, switch view mode, dump state) through the `rmdv` CLI. No other markdown viewer exposes a machine-readable interface built for agentic workflows.

![rmdv showing a folder file tree beside a rendered document](site/assets/shot-hero.webp)

## Why rmdv

| | rmdv | [Marky] | Marked 2 | Glow | Obsidian | Typora |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| Free | ✅ | ✅ | ❌ ($14) | ✅ | freemium | ❌ ($15) |
| Open source | ✅ | ✅ | ❌ | ✅ | ❌ | ❌ |
| Native (no webview) | ✅ | ⚠️ Tauri | ✅ | ✅ | ❌ | partial |
| Mermaid + LaTeX, no JS | ✅ | ❌ | ❌ | ❌ | plugins | partial |
| Mind-map view | ✅ | ❌ | ❌ | ❌ | plugin | ❌ |
| PDF as Markdown (local) | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Agent-controllable (IPC) | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Windows support | ✅ | ❌ | ❌ | ✅ | ✅ | ✅ |
| Folder workspace | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ |

[Marky]: https://github.com/GRVYDEV/marky

**[Marky](https://github.com/GRVYDEV/marky)** is the closest peer — Tauri+Rust, same viewer-only pitch. rmdv's edges: **Windows support**, a **pure-Rust GUI via [Iced](https://iced.rs/)** (no embedded WebView, no Chromium, no JS runtime — the whole thing renders in native widgets), **native Mermaid/DOT/LaTeX**, and the **agent-control IPC**.

## Features

- **Workspace browser** — open a folder, navigate the file tree sidebar
- **Mermaid diagrams** rendered natively — no JS, no browser
- **Graphviz DOT** diagrams rendered natively
- **Block LaTeX math** (`$$…$$`) via a pure-Rust layout engine ([iced_math](https://crates.io/crates/iced_math)) — no MathJax, no KaTeX
- **Mind-map view** (`⌘M`) — any Markdown, JSON, or YAML document as a collapsible tree
- **PDF viewing** — open a `.pdf` and read it as rendered Markdown; text extracted locally via [liteparse](https://crates.io/crates/liteparse) (PDFium), no cloud, no LLM (macOS + Linux; view-only)
- **Tree-sitter syntax highlighting** — Rust, Python, JS, TS, Go, C, C++, Java, SQL, Bash, JSON, HTML, Markdown, and more
- **Vault-wide search** (`⌘⇧F`) — Zed-style full-page results across every file in the workspace
- **In-document search** (`⌘F`)
- **Command palette** (`⌘⇧P`) and **quick file finder** (`⌘P`)
- **Live reload** — edits in your editor reflect instantly
- **Edit mode** (`⌘E`)
- **7 themes** — One Dark, GitHub, Solarized, Gruvbox, Nord, Dracula, Tokyo Night — with system follow
- **Keyboard-first** — `j`/`k`/`g`/`G`, `⌘↑`/`⌘↓`, heading fold `⌘K 0–6`
- **Auto-update** — checks GitHub releases, SHA-256 verifies; signed + notarized on macOS
- **CJK-friendly** — bundled Inter + JetBrains Mono, system font fallback
- **Drag and drop** files or folders

## Screenshots

| | |
|:-:|:-:|
| ![A Mermaid flowchart rendered natively in rmdv](site/assets/shot-diagrams.webp) | ![A JSON config file shown as a collapsible data mind map](site/assets/shot-mindmap.webp) |
| **Native Mermaid / DOT / LaTeX** | **Mind-map view (`⌘M`)** |
| ![Syntax-highlighted Rust and Java code blocks](site/assets/shot-treesitter.webp) | ![A deeply nested document with outline and breadcrumb navigation](site/assets/shot-search.webp) |
| **Tree-sitter highlighting** | **Vault-wide search (`⌘⇧F`)** |
| ![The demo vault shown in a light theme](site/assets/shot-themes.webp) | ![A notes document with task lists and tables, updated live](site/assets/shot-livereload.webp) |
| **7 themes, light + dark** | **Live reload** |

More on [rmdv.mclee.dev](https://rmdv.mclee.dev).

## Install

### macOS / Linux

Download the latest build from [Releases](https://github.com/minchenlee/rmdv/releases):

- **macOS Apple Silicon** — `rmdv_*_aarch64.dmg`
- **macOS Intel** — `rmdv_*_x86_64.dmg`
- **Linux x86-64** — `rmdv-*-x86_64.AppImage` (`chmod +x` then run)

> macOS builds are signed + notarized.

### Windows / from source

    git clone https://github.com/minchenlee/rmdv && cd rmdv
    cargo build --release
    ./target/release/rmdv path/to/file.md

Requires Rust 1.80+.

## CLI / agent control

rmdv is single-instance. The first invocation opens a window and an IPC listener; subsequent invocations talk to it and return one-line JSON.

```bash
# open a file at a specific line
rmdv path/to/foo.md --line 42

# drive the running instance
rmdv goto --section "Install/Setup"
rmdv mode mindmap
rmdv current                          # prints JSON state

# stateless — no running instance needed
rmdv list-sections path/to/foo.md     # JSON array of headings
rmdv --pretty list-sections foo.md
```

Designed for coding agents (Claude Code, Codex, Cursor) to pull rmdv to the relevant section of a file without manual navigation. `rmdv list-sections spec.md | jq` works as a pure stateless command even when rmdv isn't running.

## Keyboard shortcuts

| Key | Action | | Key | Action |
|---|---|---|---|---|
| `⌘P` | File finder | | `⌘M` | Mindmap view |
| `⌘⇧P` | Command palette | | `⌘E` | Toggle edit mode |
| `⌘O` | Open folder | | `⌘T` | Toggle theme |
| `⌘B` | Toggle sidebar | | `⌘K 0–6` | Fold headings to level |
| `⌘F` | Search in document | | `⌘/` | Shortcut cheatsheet |
| `⌘⇧F` | Search whole vault | | `j` / `k` | Scroll down / up |
| `g` / `G` | Top / bottom | | `Space` / `⇧Space` | Page down / up |
| `Esc` | Close overlay / search | | | |

## Performance

Measured on an M2 MacBook Air (median of 5 runs):

| Metric | Value |
|---|---|
| Cold start to first paint | **~150 ms** |
| Parse a 10,000-line (~1 MB) document | **8.1 ms** |
| Memory on a 10,000-line document | **~70 MB** (was 453 MB before v0.2.2) |
| Binary size | **~33 MB** (static, no runtime) |

The renderer is viewport-aware: only blocks intersecting the visible viewport become widgets, so memory and per-frame work stay roughly constant regardless of document length. Hot reload re-highlights only changed code blocks.

See [`docs/benchmarks.md`](docs/benchmarks.md) for the full table and how to reproduce.

## Demo

The [`demo/`](demo) folder is a sample vault exercising every feature — Mermaid, DOT, LaTeX, mind maps, nested folders, vault search. Open it with `rmdv demo/`.

## Roadmap

- [x] Code signing (mac notarization)
- [x] Auto-update
- [ ] Export to PDF / HTML
- [ ] More tree-sitter grammars

## License

MIT
