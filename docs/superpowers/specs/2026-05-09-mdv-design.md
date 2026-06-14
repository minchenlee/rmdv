# rmdv — Lightweight Beautiful Markdown Viewer

**Status:** Design approved
**Date:** 2026-05-09
**Owner:** Min

## Goal

A native desktop markdown viewer that prioritizes beauty and near-zero overhead. Standalone read-only app: open a `.md` file, see it rendered with refined typography. No editing, no plugins, no bloat.

## Non-Goals (YAGNI)

- Editing or live-preview pairing
- Export to PDF/HTML
- Plugin system
- Math (KaTeX), mermaid diagrams, footnotes
- Multi-tab or workspace concept
- Network fetch beyond image URLs
- Wikilinks / Obsidian flavor
- Sync, collaboration, auth

## Stack

| Concern | Choice | Reason |
|---|---|---|
| Language | Rust | Native, fast, small binaries |
| GUI | `iced` 0.13 | Best beauty/effort ratio; retained mode; theming |
| Parser | `pulldown-cmark` 0.12 | Fast, GFM compliant |
| Highlight | `tree-sitter` 0.24 + per-lang grammars | ~10 langs, lean |
| File watch | `notify` 7 | Hot reload on save |
| Image | `image` 0.25 | Decode |
| File dialog | `rfd` 0.15 | Native OS picker |
| Browser open | `open` 5 | Link clicks |
| Clipboard | `arboard` 3 | Copy code blocks |
| Config dir | `dirs` 5 | Recent files store |

Target binary size: ~8–12 MB stripped, release.

## Markdown Scope

CommonMark + GFM:
- Headings h1–h6
- Paragraphs, line breaks
- Emphasis, strong, strikethrough
- Inline code, code blocks (fenced, with lang)
- Lists (ordered, unordered, task lists)
- Blockquotes (nested)
- Tables
- Links (inline, autolinks)
- Images (local + http(s))
- Horizontal rules
- Raw HTML: ignored / shown as escaped text (security + simplicity)

## Highlighted Languages

Bundled tree-sitter grammars: Rust, Python, JavaScript, TypeScript, Go, C, Bash, JSON, HTML, Markdown. Unknown lang → fallback to plain monospace.

## Architecture

```
rmdv/
├── Cargo.toml
├── src/
│   ├── main.rs          // CLI parse, iced::application entry
│   ├── app.rs           // State, Message, update, view, subscriptions
│   ├── parser.rs        // pulldown-cmark events → Vec<Block> AST
│   ├── render.rs        // AST → iced Element tree
│   ├── highlight.rs     // tree-sitter wrappers, span coloring
│   ├── theme.rs         // Light/Dark palettes, typography tokens
│   ├── watch.rs         // notify watcher → Message::FileChanged
│   ├── recent.rs        // Recent files JSON store
│   └── assets/
│       ├── fonts/       // Inter, Charter (or Iowan), JetBrains Mono
│       └── icons/
└── docs/
    └── superpowers/specs/2026-05-09-rmdv-design.md
```

## Data Flow

```
CLI arg / file dialog / drag-drop
        │
        ▼
read file (UTF-8 lossy)
        │
        ▼
pulldown-cmark events ──► AST: Vec<Block>
                                │
                                ▼
                         render::view(ast, theme) ──► iced Element
                                                            │
                                                            ▼
                                                       GPU draw

notify watcher ──► Message::FileChanged ──► reparse + rerender
theme toggle ──► Message::ThemeChanged ──► rerender (AST cached)
```

Re-render triggers: file change, theme toggle, window resize, scroll. AST and highlighted code spans cached in state; not re-parsed per frame.

## Internal AST

```rust
enum Block {
    Heading { level: u8, id: String, inlines: Vec<Inline> },
    Paragraph(Vec<Inline>),
    CodeBlock { lang: Option<String>, code: String, spans: Vec<HlSpan> },
    Blockquote(Vec<Block>),
    List { ordered: bool, items: Vec<ListItem> },
    Table { headers: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>> },
    Image { url: String, alt: String },
    Rule,
}

enum Inline {
    Text(String),
    Code(String),
    Emph(Vec<Inline>),
    Strong(Vec<Inline>),
    Strike(Vec<Inline>),
    Link { url: String, children: Vec<Inline> },
}

struct ListItem { task: Option<bool>, blocks: Vec<Block> }
struct HlSpan  { range: Range<usize>, style: HlStyle }
```

## UI Layout

```
┌─────────────────────────────────────────────────────────┐
│ [≡] file.md                              [☀/🌙] [⚙]    │  ← top bar (auto-hide on scroll)
├──────────┬──────────────────────────────────────────────┤
│  TOC     │                                              │
│  H1      │           # Heading                          │
│  ├ H2    │                                              │
│  └ H2    │           Body paragraph in serif…           │
│  H1      │                                              │
│          │           ```rust                            │
│          │           fn main() { … }                    │
│          │           ```                                │
│          │                                              │
└──────────┴──────────────────────────────────────────────┘
```

- TOC: collapsible left rail, auto-built from headings, click → scroll to anchor
- Content area: max-width ~70ch, centered, generous gutters
- Top bar: file name, theme toggle, settings; fades on scroll, returns on hover/move
- Empty state: centered card "Drop a .md file here or [Open File]"

## Beauty Levers

- **Typography:** Charter (or Iowan Old Style) for body; Inter for UI; JetBrains Mono for code. Body 16px, line-height 1.6, letter-spacing 0.
- **Measure:** body text capped at ~70ch for readability.
- **Vertical rhythm:** 8px base; headings/paragraphs/lists snap to multiples.
- **Color:** muted neutrals — `#1a1a1a` on `#fafaf7` (light), `#e8e6e1` on `#16181b` (dark). Single restrained accent for links.
- **Code blocks:** subtle tinted background, soft 1px border, internal padding 16px.
- **Scroll:** smooth, momentum, soft auto-hiding scrollbar.
- **Motion:** 150ms fade on file load and theme switch. No bouncy.
- **Selection:** custom selection color matching accent at low alpha.

## Features

### File Loading
- CLI: `rmdv path/to/file.md`
- File menu → Open… (Cmd/Ctrl+O) → `rfd` native dialog
- Drag-drop onto window
- Recent files: last 5 in `dirs::config_dir()/rmdv/recent.json`, surfaced in menu

### Hot Reload
- `notify` watches active file
- On change → reparse → rerender
- Preserves scroll position when content shape unchanged

### Theme
- Light / Dark / System
- Toggle in top bar (Cmd/Ctrl+T)
- System mode follows OS preference via `dark-light` crate

### Navigation
- `j`/`↓` line down, `k`/`↑` line up
- `Space`/`PgDn` page down, `Shift+Space`/`PgUp` page up
- `g` top, `G` bottom
- `/` or Cmd/Ctrl+F find-in-doc; `n`/`N` next/prev match
- TOC click → smooth scroll to heading
- Link click → `open` in OS default browser
- Code block: hover → "Copy" button → `arboard`

### Search
- Inline find bar at top (Ctrl+F)
- Highlights matches, scrolls to first; `n`/`N` cycles
- Case-insensitive by default, regex toggle

## Error Handling

| Case | Behavior |
|---|---|
| File not found | Centered error card with "Open another" button |
| Bad UTF-8 | Lossy decode, banner: "Some bytes were not valid UTF-8" |
| Image load fail | Broken-image placeholder, no panic |
| Unknown code lang | Plain monospace, no highlight |
| Watcher error | Silent fall back to manual reload (Cmd/Ctrl+R) |
| File too large (>10 MB) | Confirm dialog before load |

## Performance

- Parse only on file change, not per frame (iced is retained)
- Pre-build widget tree once into state; reuse across redraws
- tree-sitter parses code blocks once; cache spans on the `CodeBlock` node
- Image decode off-thread via `tokio` task; placeholder until ready
- No heap allocation in scroll/redraw hot path
- Cold start target: <80 ms to first paint on M-series Mac for 10 KB doc
- Memory: <50 MB resident for typical docs

### Binary Size

Bundling 10 tree-sitter grammars (rust, python, js, ts, go, c, bash, json, html, md) pushes the release binary substantially over the original ~8-12 MB target. This is acceptable for v0.1 — correctness and language coverage matter more than a few extra MB on disk. Future work could feature-gate grammars behind cargo features or drop low-value ones for an MD viewer (e.g., `markdown`, `html`).

## Testing

- **Unit (parser):** snapshot AST for CommonMark spec subset + GFM samples via `insta`
- **Unit (highlight):** known-input → expected span ranges per lang
- **Integration:** open fixture `.md` files, assert no panic, basic shape of state
- **Manual visual:** golden screenshots checked into repo; eyeball diff on changes
- **Fuzz (light):** `cargo-fuzz` on parser entry to ensure no panics on random input

## Security

- Raw HTML in markdown: stripped, not executed (no WebView)
- Image URLs: only http/https/file; reject other schemes
- Link clicks: passed through `open` crate (delegates to OS)
- No remote code execution surface; no JS engine; no shell-out
- File watching: only the active file path, no recursive watching

## Build & Distribution

- `cargo build --release`
- Strip with `cargo install cargo-strip` or `strip` on macOS
- Bundled fonts in binary via `include_bytes!`
- macOS: `.app` bundle via `cargo-bundle`
- Linux: AppImage
- Windows: `.exe` + installer via `cargo-wix`

## Open Questions

None at design time. Font licensing for Charter requires verification before bundling — fallback to Iowan Old Style or system serif if licensing blocks.

## Milestones

1. **M1 — Skeleton:** iced window, CLI arg, read file, render plain text
2. **M2 — Parser + basic blocks:** headings, paragraphs, lists, code blocks (no highlight), blockquote, hr
3. **M3 — Inline styles + links:** emphasis, strong, code, links (clickable)
4. **M4 — Tables + images + GFM extras:** tables, task lists, strike, autolinks, images
5. **M5 — Theming + typography:** light/dark, font bundling, refined spacing
6. **M6 — Highlight:** tree-sitter wired, 10 langs, span coloring
7. **M7 — UX polish:** TOC, search, keyboard nav, copy-code, drag-drop, file dialog, recent files
8. **M8 — Hot reload:** `notify` integration
9. **M9 — Distribution:** bundles for mac/linux/windows
