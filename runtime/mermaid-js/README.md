# Node/Chrome Mermaid.js worker (non-macOS fallback)

On macOS, `codex/mermaid-js-integration` uses the embedded system WebKit helper
in `../mermaid-webkit/README.md`. This directory supplies its build-time assets.
Other platforms retain the Node/Chrome worker described below, using Mermaid 12.2.0 in a persistent
background Chrome worker. Chromium supplies the PNG pixels displayed by Iced;
the original SVG is retained for export. DOT and math keep their native paths.

For a fresh checkout, install Node.js >=22.12 and Google Chrome, then run:

```sh
cd runtime/mermaid-js
npm ci
npm run prepare:icons
```

Run `target/release/rmdv` from the repository. The default runtime path points
to this source checkout; this is a local trial, not a self-contained installer.
The dependencies and AWS icon cache are already prepared on the trial machine.

Optional environment variables:

- `RMDV_NODE`: absolute Node executable path.
- `RMDV_CHROME`: absolute Chrome/Chromium executable path.
- `RMDV_MERMAID_RUNTIME`: directory containing worker.mjs and installed packages.

A loopback-only server serves local Mermaid modules and the checksum-pinned
AWS pack. Browser requests outside that local origin are blocked. The worker
is serialized, reused between diagrams, and reset after a protocol failure or
45-second timeout. Missing dependencies produce a diagram error.

The official example's `logos` pack is also registered locally. Existing
Markdown can use `logos:aws-route53`, `logos:aws-cloudfront`, `logos:aws-s3`,
`logos:aws-elb`, `logos:aws-ec2`, `logos:aws-rds`, and `logos:aws-elasticache`.
Unknown `logos:` names produce a clear error instead of a placeholder.
After updating dependencies, restart rmdv to reload the persistent worker.

AWS flowcharts and architecture diagrams support canonical names such as
`aws:lambda` and source-derived aliases such as `aws:arch-aws-lambda`.
Unknown names produce an error. See THIRD_PARTY_NOTICES.md and
AWS-ICONS-LICENSE.txt for the asset attribution and separate license.

Packaging Node/Chromium, startup cost, cross-platform behavior and native
visual acceptance remain follow-up work. The owner will manually verify the
release trial; do not treat compilation as visual acceptance.
