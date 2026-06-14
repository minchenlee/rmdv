# rmdv

**Free, open-source, native, cross-platform markdown viewer.**

A fast, beautiful markdown reader for browsing folders of `.md` files — without spinning up Obsidian's vault, Typora's editor weight, or a static-site build. Just open a folder and read.

Built in Rust with [Iced](https://iced.rs/). ~16 MB binary, no Electron, no Chromium tax.

## Why rmdv

| | rmdv | [Marky] | Marked 2 | Glow | Obsidian | Typora |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| Free | ✅ | ✅ | ❌ ($14) | ✅ | freemium | ❌ ($15) |
| Open source | ✅ | ✅ | ❌ | ✅ | ❌ | ❌ |
| Native (no webview) | ✅ | ⚠️ Tauri | ✅ | ✅ | ❌ | partial |
| Windows support | ✅ | ❌ | ❌ | ✅ | ✅ | ✅ |
| macOS support | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Folder workspace | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ |
| Read-only focus | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |

[Marky]: https://github.com/GRVYDEV/marky

**[Marky](https://github.com/GRVYDEV/marky)** is the closest peer — Tauri+Rust, same viewer-only pitch, actively maintained. rmdv's edges: **Windows support** (Marky is mac+Linux only) and a **pure-Rust GUI via [Iced](https://iced.rs/)** — no embedded WebView, no Chromium, no JS runtime. The whole thing renders in native widgets.

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

Download the latest installer from [Releases](https://github.com/minchenlee/rmdv/releases):

- **macOS Apple Silicon** — `rmdv_*_aarch64.dmg`
- **macOS Intel** — `rmdv_*_x64.dmg`
- **Windows** — `rmdv_*_x64-setup.exe`

> Builds are unsigned. On macOS, right-click → Open the first time. On Windows, click "More info" → "Run anyway" past SmartScreen.

### From source

    cargo build --release
    ./target/release/rmdv path/to/file.md

Requires Rust 1.80+.

## Keyboard shortcuts

| Key | Action |
|---|---|
| `⌘P` | Open file finder |
| `⌘K` | Open command palette |
| `⌘O` | Open folder |
| `⌘B` | Toggle sidebar |
| `⌘F` | Search in document |
| `⌘T` | Toggle theme |
| `j` / `k` | Scroll down / up |
| `g` / `G` | Top / bottom |
| `Space` / `Shift+Space` | Page down / up |
| `Esc` | Close overlay / search |

## CLI / agent control

rmdv is single-instance. The first invocation opens a window and an IPC
listener; subsequent invocations talk to it.

```bash
# open a file at a specific line
rmdv path/to/foo.md --line 42

# navigate the running instance
rmdv goto --section "Install/Setup"
rmdv mode mindmap
rmdv current                          # prints JSON state

# stateless (no running instance needed)
rmdv list-sections path/to/foo.md     # JSON array of headings
rmdv --pretty list-sections foo.md
```

Designed for coding agents (Claude Code, Codex) to pull rmdv to the relevant
section of a file without manual navigation.

## Performance

- Cold start: window paints before system fonts finish loading
- 1 MB / 10k-line documents parse in single-digit milliseconds
- Hot reload re-highlights only changed code blocks

See [`docs/benchmarks.md`](docs/benchmarks.md) for measured numbers and how to reproduce.

## Roadmap

- [ ] Code signing (mac notarization, Windows cert)
- [ ] Auto-update
- [ ] Export to PDF / HTML
- [ ] More tree-sitter grammars

## License

MIT
