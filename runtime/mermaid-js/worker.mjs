// Private JSON-lines worker. stdout belongs exclusively to the Rust protocol.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { createInterface } from 'node:readline';
import { chromium } from 'playwright';
const root = new URL('./', import.meta.url);
let browser;
const server = createServer(async (req, res) => {
  try {
    const name = new URL(req.url, 'http://localhost').pathname;
    if (name === '/') {
      res.setHeader('Content-Type', 'text/html');
      res.end('<!doctype html><meta charset="utf-8"><style>html,body{margin:0;background:transparent}#diagram{display:inline-block}svg{display:block}</style><div id="diagram"></div><script type="module" src="/page.mjs"></script>');
      return;
    }
    if (!name.startsWith('/node_modules/mermaid/dist/') && !['/page.mjs', '/icons.mjs', '/icon-cache/aws-icons.json', '/node_modules/@iconify-json/logos/icons.json'].includes(name)) throw Error('Unavailable asset');
    const path = new URL('.' + name, root);
    if (!path.href.startsWith(root.href)) throw Error('Invalid path');
    res.setHeader('Content-Type', name.endsWith('.json') ? 'application/json' : 'text/javascript');
    res.end(await readFile(path));
  } catch { res.writeHead(404).end(); }
});
async function close() {
  await browser?.close().catch(() => {});
  await new Promise(resolve => server.close(resolve));
}
process.once('SIGTERM', () => { close().finally(() => process.exit(0)); });
try {
  const icons = await readFile(new URL('icon-cache/aws-icons.json', root));
  if (createHash('sha256').update(icons).digest('hex') !== '36bff63d2b9f5f94021543a0f0e3b2ed845062a1953eb9038dbf23651fd47111') throw Error('AWS icon checksum mismatch; run npm run prepare:icons');
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const origin = `http://127.0.0.1:${server.address().port}`;
  browser = await chromium.launch({ headless: true, ...(process.env.RMDV_CHROME ? { executablePath: process.env.RMDV_CHROME } : { channel: 'chrome' }) });
  const context = await browser.newContext({ viewport: { width: 1600, height: 1200 }, deviceScaleFactor: 1, serviceWorkers: 'block', acceptDownloads: false });
  await context.route('**/*', route => route.request().url().startsWith(origin + '/') ? route.continue() : route.abort());
  const page = await context.newPage();
  page.setDefaultTimeout(20000);
  await page.goto(origin);
  await page.waitForFunction(() => typeof window.renderDiagram === 'function');
  for await (const line of createInterface({ input: process.stdin, crlfDelay: Infinity })) {
    try {
      const request = JSON.parse(line);
      const result = await page.evaluate(request => window.renderDiagram(request), request);
      const png = request.raster ? (await page.locator('#diagram > svg').screenshot({ omitBackground: true, timeout: 20000 })).toString('base64') : undefined;
      process.stdout.write(JSON.stringify({ ...result, png }) + '\n');
    } catch (error) { process.stdout.write(JSON.stringify({ error: String(error.message).slice(0, 4000) }) + '\n'); }
  }
} catch (error) {
  process.stdout.write(JSON.stringify({ error: `Mermaid.js runtime: ${error.message}. Run npm ci and npm run prepare:icons in runtime/mermaid-js; install Chrome or set RMDV_CHROME.` }) + '\n');
} finally { await close(); }
