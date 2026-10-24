# MDV-050 — Official Mermaid.js visual comparison

## Outcome

The owner rejected the Rust renderer's visual output and approved a standalone
official Mermaid.js comparison before choosing a desktop integration strategy.
Work starts from clean `5e1b769` on `codex/mermaid-js-prototype`. The Iced
application and production rendering dependencies remain unchanged.

## Scope and acceptance

- Pin official Mermaid 12.2.0 from npm; serve the complete distribution locally.
- Use the same sources from `tests/fixtures/renderer_trial.md` for official
  browser SVG and the pinned Rust renderer's actual SVG/RGBA output.
- Support source editing, sample selection, light/dark themes, official/rmdv
  palette comparison, layout selection, zoom, SVG export, and bounded errors.
- Label the native preview as a fixed reference; never imply edited source
  has been re-rendered by Rust.
- Verify the five diagram families, custom init, invalid syntax and recovery,
  theme/layout controls, CJK labels, mobile overflow, and local-only assets in
  Chrome. Inspect screenshots. Native WebView integration is out of scope.
- Document how to start/stop/reproduce; open the comparison for owner review.

## Visual plan

The subject is diagram quality: place the actual diagram above all decoration.
Use white (#ffffff) preview paper, cool canvas (#f3f5f7), ink (#202b38), muted
text (#647184), a restrained blue control accent (#465ba6), and dividers
(#dce1e8). Use the system/PingFang TC sans-serif stack for UI and diagrams,
and the system monospace stack only for editable source.

Layout: a compact header and controls above a source sidebar and two aligned
preview columns. Collapse to one column on narrow windows. No marketing hero,
card grid, ornamental labels, external fonts, or animation; diagrams own the
space. Review against the brief: all controls change the comparison, and the
page uses actual fixture content rather than sample product copy.

## Authority and verification

Local prototype only; no publish, push, merge, or production engine change.
GitHub API still reports upstream rmdv v0.10.1 on 2026-10-24. Node 24.10.0,
npm 11.6.0, and local Chrome are available. The repo guides' legacy checkout
path is stale; this checkout is `/Users/harry_chung/Developer/Personal/rmdv`.

## Verification (2026-10-24)

- `cargo run --locked --example renderer_compare -- prototypes/mermaid-js/reference`:
  exported seven same-source references with both native palettes.
- Official npm package 12.2.0 is pinned; Playwright 1.64.1 is a development-only
  dependency using the installed Chrome 155.0.8059.85, not the user's profile.
- `npm test`: 26 checks passed across seven cases and both themes. Covered
  editing, modified-reference labeling, zoom, SVG download, invalid/oversized
  source and recovery, quick sample changes, and narrow viewport overflow.
  No page errors, failed HTTP asset responses, or external requests occurred.
- An ELK mindmap exception was reproduced separately with the same source;
  Dagre succeeds. The UI reports this error instead of silently switching
  engines; default selection is Dagre for a usable comparison.
- Light/dark desktop screenshots were visually inspected. Mobile geometry
  and page overflow passed. Chrome mobile captures showed repeated strips
  despite a single document header in the DOM; these captures are not counted
  as proof of mobile visual quality. Manual mobile acceptance remains pending.
- `cargo fmt --all --check`, JavaScript syntax checks, and `git diff --check`:
  passed. No Rust product code changed, so full Rust tests/release were not
  repeated for this standalone browser prototype.
- Local server runs on 127.0.0.1:4317. The app-browser comparison was opened,
  visually confirmed, and retained as a deliverable. No publication, push, production engine replacement, or native WebView
  integration occurred. Owner visual acceptance remains pending.


## Owner extension: Pele and AWS icons (2026-10-24)

Add Pele 1.9.0 as a live preview beside official Mermaid.js, retaining the
native reference in a collapsible panel. Keep layout controls specific to
Mermaid; Pele owns its own layout and CSS-variable theme. Mermaid
`themeVariables` must not be presented as supported Pele theme settings.

Use AWS Labs' original multicolor Iconify JSON at fixed revision
`e26e2c05daf8b6bc4c764669fc2be04c314ccb8c` (871 icons), with SHA-256
`36bff63d2b9f5f94021543a0f0e3b2ed845062a1953eb9038dbf23651fd47111`.
Serve the checksum-verified cache locally and retain source/license attribution.
Mermaid registers the pack as `aws`; Pele receives a trusted icon resolver.
Expose canonical AWS Labs names plus source-derived `arch-*` aliases. This
is not a promise of complete Mermaid Chart icon-name compatibility.

Acceptance: both live engines render AWS flowchart and architecture icons
with actual vector artwork, including in exported SVG, in both themes.
Unknown icons must report an error rather than silently using a placeholder.
Keep source editing, errors, themes, zoom, and fixed native reference behavior.
The production Iced app remains outside this prototype's scope.

### Pele/AWS extension verification

- Browser-free `node verify-icons.mjs` passed: original icon checksum,
  871 icons, 279 source-derived aliases, 8 valid Pele samples, invalid source
  rejection, unknown icon rejection, and actual SVG artwork in both AWS cases.
- App-browser verification covered all nine samples in both light/dark
  appearance: eight render in both engines; the intentionally invalid case
  reports errors in both. AWS samples contain the actual Lambda orange
  artwork in each engine, not a placeholder or just a text label.
- Verified rmdv palette values in Pele's root CSS variables while preserving
  AWS artwork colors. The UI states that Pele ignores Mermaid themeVariables.
- Verified unknown AWS names report explicit errors in both previews. ELK
  mindmap errors are isolated: Pele remains visible/exportable, while the
  failing Mermaid export is disabled. Dagre and AWS samples recover normally.
- Downloaded a Pele AWS architecture SVG through the UI and parsed the file:
  12,182 bytes, all three service vectors, and frozen palette variables.
  Reproducible copy: `prototypes/mermaid-js/artifacts/pele-aws.svg` (ignored).
- Fixed author CSS overriding the native image's HTML hidden attribute.
  Rechecked after reload: a new AWS sample has display:none on the old image
  and explicitly reports that no native AWS reference has been generated.
- The source pack does not include the supplied Chart documentation's Alexa
  alias; do not claim complete Mermaid Chart AWS-name compatibility. Available
  canonical names and aliases are searchable in the prototype.
- The new browser harness was updated for both engines and AWS reproduction;
  this extension's browser evidence is the app-browser exercise above, not a
  new execution of the earlier 26-check headless run. JS syntax and diff checks
  pass; production Rust code was not changed.

Future desktop acceptance must include local AWS icon loading, both AWS
samples, missing-icon errors, and actual artwork in exported SVGs. Choosing
Pele alone is not equivalent to shipping those capabilities in Iced.


## Owner-authorized desktop integration (2026-10-24)

The owner selected official Mermaid.js and requested a separate integration
branch: `codex/mermaid-js-integration`. This authorization extends the earlier
prototype-only scope. Existing prototype changes remain in the checkout.

The Mermaid path now invokes a persistent Node/Chrome worker under
`runtime/mermaid-js`, using local Mermaid 12.2.0 and the shared AWS icon pack.
Chromium rasterizes HTML labels into PNG; Rust decodes RGBA into the existing
Iced image/cache path while retaining the original SVG. DOT and math remain
native. Requests retain existing application cache identity behavior, source
and SVG limits, plus a bounded raster and worker timeout. External browser
requests are blocked; assets are served on an ephemeral loopback port.

The narrow renderer suite passed 9 tests, including five diagram families,
both palettes, custom init, AWS artwork in actual pixels and SVG, unknown
icons and recovery. The owner then requested no additional verification and
a release build for manual testing. Full-suite and native-window acceptance
are not claimed. Runtime dependencies and icon data are prepared locally.

This is a source-checkout trial requiring local Node and Chrome. A portable
installer, cross-platform runtime setup and performance acceptance remain
pending; compilation does not satisfy those requirements.


### Owner native trial feedback

The owner confirmed the Mermaid.js desktop trial works. Enlarged diagrams now
use a 95%-opaque theme-background overlay backing so transparent pixels do not reveal
interfering document content. Ordinary image overlays retain their existing
theme-based backing. Release build requested for continued owner manual review;
no additional automated tests are run for this small visual adjustment.

The theme-matched overlay release build completed successfully. The owner
requested a local commit of the integration and preview changes. No push or
publication is authorized by this commit request.


### Logos icon pack compatibility

Owner requested support for the official Mermaid example's Iconify `logos`
pack after an existing note used `logos:aws-*`. Register the actual local
package alongside `aws`; do not rewrite notes or substitute different icons.
Serve its JSON through the worker's existing loopback allowlist, retain the
npm lockfile and package attribution, and report unknown `logos:` names.
The owner continues to handle native visual verification manually.


### Centered preview modal

Owner superseded the full-window translucent preview: use the existing modal
frame with 40px outer margins, a maximum 1100×760 panel, an opaque theme
background, preview title and close button. Preserve the existing image viewer
for wheel zoom and drag pan, Escape dismissal and outside-click dismissal.
Use the application's theme typography, LG frame radius, border and DIALOG
shadow. The diagram remains the main content; only the title and close control
occupy the header. No new palette or decorative UI is introduced.

This matches the owner's request for a bounded popup and eliminates document
bleed-through within the panel. Native visual review remains with the owner.


Owner refinement: paint the opaque preview fill directly on a 16px rounded
panel so no square child background covers its corners. Give this preview a
45%-black translucent outside scrim, leaving the document visible but dimmed.
Keep the close button and outside click; blank clicks inside the panel are
consumed without dismissing it. Other modal styles remain unchanged.


### Diagram interaction clarification

The owner clarified that resizing and dragging apply to the rendered diagram,
not to the modal. Remove the mistaken modal geometry/drag implementation.
Keep the popup centered at its fixed bounded size, with rounded opaque theme
surface and translucent scrim. Its existing image viewer handles wheel zoom
and drag pan on enlarged content; the header now makes those controls visible.
The owner manually verifies the release build.


### Trackpad interaction

Owner requested native two-finger pinch instead of treating trackpad scrolling
as zoom. Forward the existing macOS magnification bridge to the preview only
while it is open; preserve mindmap routing when no overlay is active. Adapt
Iced's MIT-licensed image viewer locally so native pinch and mouse wheel use
the same cursor-anchored scale state. Trackpad pixel scrolling pans; mouse
wheel line scrolling zooms. Consume cumulative pinch deltas only while the
pointer is over the diagram, and initialize newly mounted viewer state without
replaying old gestures. Keep the modal fixed. Header now says Pinch to zoom.

Upstream viewer copyright and license are retained alongside the adaptation.
Release build only; owner will verify actual macOS gestures manually.

Submitted for upstream review in Draft [PR #80](https://github.com/minchenlee/rmdv/pull/80).
The owner explicitly authorized this push and PR. It is not merged; native
acceptance and runtime packaging/CI follow-up remain as described above.
