# mdv

**Free, open-source, native, cross-platform markdown viewer.**

A fast, beautiful markdown reader for browsing folders of `.md` files — without spinning up Obsidian's vault, Typora's editor weight, or a static-site build. Just open a folder and read.

Built in Rust with [Iced](https://iced.rs/). ~16 MB binary, no Electron, no Chromium tax.

## Why mdv

| | mdv | [Marky] | Marked 2 | Glow | Obsidian | Typora |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| Free | ✅ | ✅ | ❌ ($14) | ✅ | freemium | ❌ ($15) |
| Open source | ✅ | ✅ | ❌ | ✅ | ❌ | ❌ |
| Native (no webview) | ✅ | ⚠️ Tauri | ✅ | ✅ | ❌ | partial |
| Windows support | ✅ | ❌ | ❌ | ✅ | ✅ | ✅ |
| macOS support | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Folder workspace | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ |
| Read-only focus | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |

[Marky]: https://github.com/GRVYDEV/marky

**[Marky](https://github.com/GRVYDEV/marky)** is the closest peer — Tauri+Rust, same viewer-only pitch, actively maintained. mdv's edges: **Windows support** (Marky is mac+Linux only) and a **pure-Rust GUI via [Iced](https://iced.rs/)** — no embedded WebView, no Chromium, no JS runtime. The whole thing renders in native widgets.

## Features

- **Workspace browser** — open a folder, navigate the file tree
- **Command palette** (`⌘K`) — every action one keystroke away
- **Quick file finder** (`⌘P`) — fuzzy jump to any `.md` in workspace
- **Live reload** — edits in your editor reflect instantly
- **Syntax highlighting** via tree-sitter — Rust, Python, JS, TS, Go, C, Bash, JSON, HTML, Markdown
- **Light / dark themes** with system follow
- **CJK-friendly** — bundled Inter + JetBrains Mono, system font fallback
- **Vim-style scrolling** — `j` / `k` / `g` / `G`
- **Drag and drop** files or folders

## Install

### macOS / Windows

Download the latest installer from [Releases](https://github.com/minchenlee/mdv/releases):

- **macOS Apple Silicon** — `mdv_*_aarch64.dmg`
- **macOS Intel** — `mdv_*_x64.dmg`
- **Windows** — `mdv_*_x64-setup.exe`

> Builds are unsigned. On macOS, right-click → Open the first time. On Windows, click "More info" → "Run anyway" past SmartScreen.

### From source

    cargo build --release
    ./target/release/mdv path/to/file.md

Requires Rust 1.80+.

## Keyboard shortcuts

| Key | Action |
|---|---|
| `⌘P` | Open file finder |
| `⌘⇧P` | Open command palette |
| `⌘O` | Open folder |
| `⌘B` | Toggle sidebar |
| `⌘F` | Search in document |
| `⌘⇧F` | Search whole vault |
| `⌘T` | Toggle theme |
| `⌘E` | Toggle edit mode |
| `⌘M` | Mindmap view |
| `⌘K` `0–6` | Fold headings to level |
| `⌘/` | Shortcut cheatsheet |
| `j` / `k` | Scroll down / up |
| `g` / `G` | Top / bottom |
| `Space` / `Shift+Space` | Page down / up |
| `Esc` | Close overlay / search |

## CLI / agent control

mdv is single-instance. The first invocation opens a window and an IPC
listener; subsequent invocations talk to it.

```bash
# open a file at a specific line
mdv path/to/foo.md --line 42

# navigate the running instance
mdv goto --section "Install/Setup"
mdv mode mindmap
mdv current                          # prints JSON state

# stateless (no running instance needed)
mdv list-sections path/to/foo.md     # JSON array of headings
mdv --pretty list-sections foo.md
```

Designed for coding agents (Claude Code, Codex) to pull mdv to the relevant
section of a file without manual navigation.

## Performance

- Cold start: window paints before system fonts finish loading
- 1 MB / 10k-line documents parse in single-digit milliseconds
- Hot reload re-highlights only changed code blocks

See [`docs/benchmarks.md`](docs/benchmarks.md) for measured numbers and how to reproduce.

## Roadmap

- [x] Code signing (mac notarization)
- [x] Auto-update
- [ ] Export to PDF / HTML
- [ ] More tree-sitter grammars

## License

MIT
