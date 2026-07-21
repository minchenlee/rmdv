# Landing-site guidance

## Scope

Applies to `site/**`.

## Purpose

Owns the static rmdv landing site, its accessibility/SEO content, browser
enhancements, screenshots, fonts, and Cloudflare Workers static-asset payload.
It is separate from the Rust application and has no Cargo build boundary.

## Start here

- [`index.html`](index.html) — page structure, metadata, JSON-LD, and inline bootstrap
- [`app.js`](app.js) — keyboard layer, theme, command palette, and progressive release links
- [`style.css`](style.css) — layout, responsive behavior, and visual system
- [`_headers`](_headers) — CSP, security headers, and cache policy
- [`../wrangler.jsonc`](../wrangler.jsonc) — `./site` asset directory and custom domain
- [`../.github/workflows/deploy-site.yml`](../.github/workflows/deploy-site.yml) — manual-only deployment workflow

## Architecture and boundaries

- This is a static asset tree: `wrangler.jsonc` publishes `site/` directly;
  the Rust package does not build or bundle these files.
- `index.html` owns content and document semantics, `style.css` owns visual
  presentation, and `app.js`/`ghost.js` provide progressive browser behavior.
  Keep the page usable with the static HTML when JavaScript enhancement fails.
- `_headers` is part of the deployment contract. The Content-Security-Policy
  hashes cover the inline scripts in `index.html`; changing an inline script
  requires updating the matching hash deliberately.
- The deploy workflow is `workflow_dispatch` only. Do not imply that a local
  site edit is live until an explicit deployment and live check has happened.

## Commands

- There is no site build command; review static changes against the files above and `git diff --check`.
- `node site/check-shortcuts.mjs` — verify the static app-shortcut reference against the native bindings in `src/app.rs` and guard its non-interactive boundary.
- `wrangler deploy` — the deployment command configured by the repository's manual workflow; run only when publishing is explicitly in scope.

## Editing constraints

- Keep canonical URLs, metadata, JSON-LD, Open Graph/Twitter content, visible
  copy, `llms.txt`, `robots.txt`, and `sitemap.xml` consistent when product
  claims or navigation change.
- Preserve keyboard access, focus restoration, reduced-motion behavior,
  semantic controls, and the CSP/security headers when editing interactions.
- Keep screenshots, fonts, and other binary assets under `site/assets/`; do not
  replace them with remote dependencies without an explicit product decision.
- Deployment is separate from editing. Do not modify workflow triggers or
  claim a Cloudflare release as part of a local documentation or code change.

## Verification

1. Inspect the changed HTML, CSS, JS, metadata, and headers together.
2. Run `git diff --check`.
3. If deployment is requested, use the manual workflow/Wrangler path and verify the live domain separately.

## Status and handoff

- [Current status](../PROJECT_STATUS.md)
- [Deploy workflow](../.github/workflows/deploy-site.yml)
