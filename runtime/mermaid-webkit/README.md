# macOS system WebKit trial

`codex/mermaid-js-integration` keeps the Iced reader and preview but replaces the
macOS Node/headless Chrome rendering worker with a native AppKit/WKWebView
helper. One helper/WebView is shared across all Mermaid requests and files.
It starts on demand, renders sequentially, and is released after 60–75 seconds
idle; subsequent cache misses recreate it. Cached images do not need WebKit.

## Build

Prepare the pinned npm assets and AWS icon cache under `../mermaid-js` as
usual. Node/npm are build tools only for this macOS path. `cargo build
--release --bin rmdv` uses Xcode's `swiftc` to compile the helper and embeds it
along with compressed Mermaid/icon data. It needs macOS command-line tools.

## Run

Run the ordinary `target/release/rmdv`. The macOS binary does not read scripts
from the checkout or invoke Node, Chrome, npm, or Swift at runtime. Embedded
files are lazily extracted into a private per-process temporary directory.
The worker's stdin closing or idle eviction releases it and removes that
folder. Abrupt app/worker termination may leave OS temporary files.

WebKit loads local scripts over an allowlisted custom scheme. A content
security policy blocks network connections/external images/fonts, and
navigation is limited to the private scheme/about page. There is no localhost
HTTP server or persistent browser profile. Both icon packs and original
license notices are retained.

The offscreen render view stays attached to a non-activating window so WebKit
can lay out text and take transparent PDF capture (macOS 14+). The PDF is rasterized into a cleared RGBA
context to preserve alpha; ordinary WebKit snapshots flatten the page background.
Only original SVG and bounded PNG cross
the existing JSON-lines protocol; Rust decodes the PNG into the Iced cache.

## Limits

Transparent rendering on this trial branch requires macOS 14 or newer.
This still has one native helper process plus WebKit-managed processes. No
claim of memory reduction is made without measurement. The owner confirmed the native transparency fix works; full gesture/platform
acceptance and resource measurements are not claimed. Non-macOS platforms retain the
Node/Chrome fallback. Production helper signing/notarization, sandbox policy
and installer integration require a separate audit. This native macOS path is included in upstream PR #80.


## Layout and fonts

Flowcharts default to Mermaid's ELK layout; other diagram types retain Dagre.
Explicit source/config layout choices still apply. Measurement and final SVG
use an explicit Helvetica/PingFang/sans-serif fallback because Iced's bundled
fonts are not automatically registered in the helper. Subgraph titles are
repainted above edge paths against their own cluster fill to preserve legibility;
original semantic labels remain in the SVG.
