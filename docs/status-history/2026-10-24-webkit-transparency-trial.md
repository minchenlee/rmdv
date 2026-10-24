# WebKit rendering trial and transparency diagnosis

Historical development evidence; current contract is
[MDV-052](../plans/active/MDV-052-system-webkit-renderer.md).

# MDV-052 — System WebKit Mermaid trial

Owner requested a lighter alternative to the Node/headless Chrome renderer.
Start a separate `codex/mermaid-webkit` branch from the submitted draft PR #80,
without pushing changes into that PR automatically.

## Implementation contract

- macOS uses an AppKit/WebKit helper with one WKWebView per app's rendering
  worker, started lazily at the first Mermaid request and reused across files.
- Embed the compiled helper, standalone Mermaid.js distribution and both icon
  packs into the Rust executable. Node/npm are build-asset preparation tools,
  not macOS runtime requirements; no source-checkout paths at runtime.
- Keep Iced as the UI. Return original SVG plus bounded WebKit snapshot PNG to
  the existing RGBA/cache path. Preserve browser text shaping and icon artwork.
- Serve only embedded scripts through a private custom scheme; no listening
  HTTP port, browser profile, network fonts or external asset requests.
- Keep JSON-line response bounds and a render timeout. Release the worker after
  60 seconds idle (checked every 15 seconds); lazily recreate when needed.
- Keep the old worker on non-macOS for this first prototype. Do not claim
  cross-platform replacement or lower measured memory without evidence.

## Acceptance and limits

Build a local macOS release for owner manual review, including existing notes
with `logos:aws-*`, original `aws:` artwork, CJK, both themes, invalid source,
recovery, image preview/gestures and cache behavior. The owner previously asked
to handle manual verification; do not rerun broad suites for this round.

Swift command-line tools and prepared local npm assets are required to build.
The embedded helper is a separate native process; WebKit can create system
content/network/GPU processes. This removes Node and standalone Chrome but is
not a promise of a process-free or zero-memory-cost renderer. Signing and
notarization of the extracted embedded helper require a later packaging audit.

## Build evidence

`cargo build --release --bin rmdv --offline` completed on macOS arm64, including
Swift helper compilation and embedded assets. Native render acceptance and
resource measurements are pending owner manual review. No push or PR update.


## Owner trial feedback: JavaScript bridge

The first native trial reported `JavaScript execution returned a result of an
unsupported type`. The evaluateJavaScript expression returned its Promise
chain even though results were already sent through the script-message
handler. End the expression with `null` so WebKit bridges a supported scalar;
SVG/PNG completion still travels exclusively through the existing message
handler. Rebuild the embedded helper for owner manual review.


## Owner trial feedback: theme colors

Pass the current palette as structured per-request Mermaid configuration,
including darkMode, text/edge-label/cluster colors, instead of relying only on
an injected source directive. Reset configuration for each diagram and set the
WebKit document background/color scheme to the same palette so snapshots do
not fall back to the web view's default white surface. Keep original AWS/logo
artwork colors. Explicit user init/frontmatter/style colors remain overrides.
Existing palette-keyed cache invalidation continues to request new renders.
Rebuild for owner manual review; no new native visual acceptance is claimed.


## Owner refinement: transparent diagram canvas

Keep structured theme colors but remove the solid HTML/body and root-SVG
background from the rendered image. Set the native window non-opaque/clear,
and use WebKit's public underPageBackgroundColor = clear API on macOS 12+.
The snapshot retains alpha via the RGBA bitmap and PNG encoder. Modal fill
remains opaque and theme-matched. Preserve opaque node fills, explicitly
styled subgraphs, labels and original icon artwork; do not remove pixels by
color matching. Native transparency acceptance remains with the owner.


## Confirmed root cause and transparent output

Owner screenshot still showed a dark rectangle after transparent CSS and the
under-page/window changes. Independent WebKit source review confirmed that
`takeSnapshot` fills its native frame base background; an RGBA destination
cannot recover that lost alpha. Replace that call with public
WKPDFConfiguration.allowTransparentBackground + createPDF (macOS 14+), then
rasterize the PDF page directly into a cleared sRGB premultiplied RGBA
CGContext. Do not use PDFKit page presentation or remove pixels by color.
Older macOS receives an explicit unsupported-version error on this trial path.

Focused worker-only alpha probe passed on a CJK + AWS Lambda flowchart:
559×200 PNG, corner alpha [0,0,0,0], 64,927 fully transparent pixels and 45,367
fully opaque pixels. SVG still contains the CJK label and original #ED7100 AWS
artwork. No full suite or owner-window automation was run. Temporary evidence:
/tmp/rmdv-webkit-alpha.png and /tmp/rmdv-webkit-alpha.svg. This is actual PNG
alpha evidence, not a claim of complete owner visual acceptance.
