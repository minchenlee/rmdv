# rmdv demo vault

Welcome. This folder is a hands-on tour of **rmdv** — a native, pure-Rust markdown viewer with diagrams, math, mind maps, and data trees.

Open this folder as a project (⌘O on the folder, or pass it on the CLI) so the sidebar, breadcrumb, and **⌘⇧F vault search** light up.

## Tour

| Area | Page | Shows off |
|------|------|-----------|
| Install | [macOS](guide/getting-started/install/macos/install.md) · [Linux](guide/getting-started/install/linux/install.md) | code blocks, task lists |
| Basics | [Quickstart](guide/getting-started/first-steps/quickstart.md) | tables, blockquotes, nested lists, links |
| Markdown | [Syntax kitchen sink](guide/features/markdown/syntax.md) | every block + all syntax-highlight grammars |
| Diagrams | [Mermaid](guide/features/diagrams/mermaid/flowcharts.md) · [Graphviz DOT](guide/features/diagrams/graphviz/dot.md) | rendered diagrams |
| Math | [Equations](guide/features/math/equations.md) | block `$$…$$` via iced_math |
| Images | [Gallery](guide/features/images/gallery.md) | local + remote images, zoom modal |
| API ref | [OAuth](reference/api/v2/auth/oauth.md) · [Users endpoint](reference/api/v2/endpoints/users.md) | deep nesting, Rust/Java/SQL |
| Data | [config.json](reference/data/config.json) · [settings.yaml](reference/data/settings.yaml) | **⌘M data mind map** |
| Papers | [relativity.tex](papers/research/relativity.tex) | full .tex document rendering |
| Notes | [Q2 standup](projects/2026/q2/notes/standup.md) | deep dirs, task lists |

## Try these

- **⌘M** on any markdown page → markdown mind map. On a `.json`/`.yaml` → data mind map.
- **⌘E** → edit mode (live, theme-aware highlighting).
- **⌘⇧F** → search this whole vault.
- **⌘↑ / ⌘↓** → jump between headings; the outline panel mirrors the structure.
- **Open Themes Folder** (command palette) → drop in a base16 theme.

> Everything here is rendered natively — no browser, no JS, no Electron.
