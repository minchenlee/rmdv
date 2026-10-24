# Third-party components

- Mermaid.js 12.2.0: https://github.com/mermaid-js/mermaid, MIT. The npm package
  includes its license.
- Pele 1.9.0: https://github.com/obsidianmd/pele, copyright kepano, MIT. The npm
  package includes its license.
- AWS icon artwork: Amazon Web Services, provided by AWS Labs' **AWS Icons for
  PlantUML** project: https://github.com/awslabs/aws-icons-for-plantuml.
  Source revision: `e26e2c05daf8b6bc4c764669fc2be04c314ccb8c`.
  File: `dist/aws-icons-mermaid.json`.
  SHA-256: `36bff63d2b9f5f94021543a0f0e3b2ed845062a1953eb9038dbf23651fd47111`.
  The source project's license is Creative Commons Attribution-NoDerivs 2.0;
  its full text is retained in `AWS-ICONS-LICENSE.txt`. The downloaded JSON
  retains its original `info` attribution. Icon shapes/colors are not edited;
  the adapter only maps lookup names and wraps original SVG bodies for display.
  AWS reference: https://aws.amazon.com/architecture/icons/.

`prepare-icons.mjs` fetches only the pinned data file and verifies its checksum.
The cached icon file is not relicensed under the application's MIT license.
