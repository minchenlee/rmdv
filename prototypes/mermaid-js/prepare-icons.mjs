import { createHash } from 'node:crypto';
import { mkdir, writeFile } from 'node:fs/promises';

const revision = 'e26e2c05daf8b6bc4c764669fc2be04c314ccb8c';
const base = `https://raw.githubusercontent.com/awslabs/aws-icons-for-plantuml/${revision}/`;
const expected = '36bff63d2b9f5f94021543a0f0e3b2ed845062a1953eb9038dbf23651fd47111';
const response = await fetch(base + 'dist/aws-icons-mermaid.json');
if (!response.ok) throw new Error(`AWS icon download failed: ${response.status}`);
const data = Buffer.from(await response.arrayBuffer());
if (createHash('sha256').update(data).digest('hex') !== expected) throw new Error('AWS icon checksum mismatch');
const pack = JSON.parse(data);
for (const icon of Object.values(pack.icons)) {
  if (/<(?:script|foreignObject)\b|\bon\w+\s*=|javascript:|(?:href\s*=\s*["'])https?:/i.test(icon.body)) {
    throw new Error('Unexpected active or remote content in AWS icon data');
  }
}
const directory = new URL('icon-cache/', import.meta.url);
await mkdir(directory, { recursive: true });
await writeFile(new URL('aws-icons.json', directory), data);
console.log(`Prepared ${Object.keys(pack.icons).length} local AWS icons at ${revision}`);
