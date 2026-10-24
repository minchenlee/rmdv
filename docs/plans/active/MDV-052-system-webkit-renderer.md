# MDV-052 — System WebKit Mermaid renderer

## Outcome and authority

Use system WebKit on macOS to remove Node/headless Chrome runtime dependencies
while retaining official Mermaid.js and AWS/Logos artwork. The owner confirmed
the final transparency fix works and authorized moving this work from the
local experiment into the existing PR branch, `codex/mermaid-js-integration`,
and updating [PR #80](https://github.com/minchenlee/rmdv/pull/80).
Delete the obsolete local `codex/mermaid-webkit` branch after preserving its work.
This authorizes the push/PR update, not merging, release or publication.

## Current contract

- One lazy native helper/WKWebView serves all Mermaid requests and files,
  serially. Release after 60–75 seconds idle; cached images remain in Iced.
- Embed the compiled helper, compressed Mermaid.js, both icon packs and license
  notices. Extract lazily into a private temporary directory. Node/npm and
  Swift command-line tools are build dependencies only on this macOS path.
- Use a local allowlisted URL scheme and CSP; no HTTP listener, external asset
  requests or persistent browser profile. Keep source/SVG/raster/time bounds.
- Send each palette as structured Mermaid configuration. Explicit source
  styles and brand artwork keep their own colors. The diagram canvas is clear;
  the preview modal supplies the opaque theme background.
- On macOS 14+, capture with public transparent PDF output and rasterize
  directly into a cleared sRGB premultiplied RGBA context. Keep original SVG.
  Ordinary WebKit snapshots flatten the frame base color and are not used.
- Non-macOS retains the previous Node/Chrome worker. Older macOS reports that
  this transparent native path requires macOS 14+. Do not imply all platforms
  have the new dependency footprint.

## Evidence and acceptance

- Latest `cargo build --release --bin rmdv --offline` passed on macOS arm64,
  including the embedded Swift helper and asset packaging.
- Focused worker probe: 559×200 PNG, four corner alpha values all zero,
  64,927 transparent and 45,367 opaque pixels; original SVG retains CJK text
  and #ED7100 Lambda artwork. No broad suite or owner-window automation.
- The owner explicitly confirmed the final fix works on the native app.
- Runtime resource usage, full platform/fixture coverage, helper signing and
  notarization, installer integration and CI asset provisioning remain open.
  The helper and WebKit can launch system processes; no measured memory claim.

The superseded attempts and source-level diagnosis are retained in
[trial history](../../status-history/2026-10-24-webkit-transparency-trial.md).


## Owner follow-up: flowchart title overlap (2026-10-25)

The owner's ALB diagram reproduced overlapping sibling subgraph rectangles,
long-title clipping and edge paths through titles. The worker inherited only
JetBrains Mono from Iced even when that font was unavailable to WebKit.

Add a Helvetica/PingFang/sans-serif fallback used consistently by body and
Mermaid theme variables, and await font readiness before layout. Use ELK for
flowcharts and retain Dagre for other types (including the known mindmap case);
explicit config/source layout choices still take precedence. Add modest
flowchart node/rank spacing and subgraph-title margins. Paint decorative title
copies with their own cluster-color backing above edge paths, preserving the
original labels for SVG semantics and hiding copies from accessibility/hit
handling. Serialize SVGMatrix components explicitly for WebKit compatibility.

A focused native-worker comparison of the unchanged ALB source showed separate
sibling rectangles, titles contained within their rectangles and no line/text
overlap in the final PNG. The 1420×1178 result was visually inspected. Temporary
artifacts are /tmp/rmdv-alb-before.png and /tmp/rmdv-alb-after.png. No broad suite
was run or note edited. The release build passed, and the owner accepted the
result and authorized committing/pushing this round to the existing PR branch.
