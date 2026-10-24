import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { render } from 'pele';
import { createAwsIcons } from './icons.mjs';
import { extraSamples } from './extra-samples.mjs';

const buffer = await readFile(new URL('icon-cache/aws-icons.json', import.meta.url));
assert.equal(createHash('sha256').update(buffer).digest('hex'), '36bff63d2b9f5f94021543a0f0e3b2ed845062a1953eb9038dbf23651fd47111');
const aws = createAwsIcons(JSON.parse(buffer));
assert.equal(Object.keys(aws.pack.icons).length, 871);
assert.equal(aws.resolve('aws:missing-icon'), undefined);
assert.throws(() => aws.validate('service a(aws:missing-icon)[Unknown]'), /找不到 AWS 圖示/);
for (const [alias, canonical] of [
  ['arch-aws-lambda', 'lambda'],
  ['arch-amazon-api-gateway', 'api-gateway'],
  ['arch-amazon-dynamodb', 'dynamodb'],
]) assert.equal(aws.resolve(`aws:${alias}`), aws.resolve(`aws:${canonical}`));

const manifest = JSON.parse(await readFile(new URL('reference/manifest.json', import.meta.url)));
let rendered = 0;
for (const sample of [...manifest.samples, ...extraSamples]) {
  const run = () => render(sample.source, { icons: aws.resolve, idPrefix: `verify-${rendered}`,
    autoDirection: false, maxEdges: 500, limit: 65536, outputLimit: 4 * 1024 * 1024 });
  if (sample.title === 'Invalid source') { assert.throws(run); continue; }
  aws.validate(sample.source);
  const result = run();
  assert.ok(result.width > 0 && result.height > 0 && result.svg.includes('<svg'));
  if (sample.title.startsWith('AWS')) {
    for (const name of ['Arch_AWS-Lambda', 'Arch_Amazon-API-Gateway', 'Arch_Amazon-DynamoDB']) {
      assert.ok(result.svg.includes(name), `${sample.title}: missing actual ${name} SVG`);
    }
    assert.ok(result.svg.includes('#ED7100'), 'AWS multicolor artwork was lost');
  }
  rendered++;
}
console.log(JSON.stringify({ iconPack: 871, aliases: Object.keys(aws.pack.aliases).length,
  validPeleSamples: rendered, invalidSampleRejected: true, unknownIconRejected: true }, null, 2));
