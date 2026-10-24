import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { chromium } from 'playwright';
import { extraSamples } from './extra-samples.mjs';

const origin = 'http://127.0.0.1:4318';
const server = spawn(process.execPath, ['server.mjs'], {
  cwd: new URL('.', import.meta.url), env: { ...process.env, MERMAID_TRIAL_PORT: '4318' }, stdio: 'pipe',
});
let browser;
try {
  for (let attempt = 0; ; attempt++) {
    try { const response = await fetch(origin); if (response.ok) break; } catch {}
    if (attempt > 50) throw new Error('Comparison test server did not start');
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const manifest = JSON.parse(await readFile(new URL('reference/manifest.json', import.meta.url)));
  browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage({ viewport: { width: 1550, height: 1000 }, deviceScaleFactor: 1 });
  const pageErrors = [], externalRequests = [], failedRequests = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));
  page.on('request', (request) => {
    if (!request.url().startsWith(origin) && !/^(data|blob):/.test(request.url())) externalRequests.push(request.url());
  });
  page.on('response', (response) => { if (response.status() >= 400) failedRequests.push(`${response.status()} ${response.url()}`); });
  await page.goto(origin);
  async function settled(valid = true, peleValid = valid) {
    await page.waitForFunction(() => /^(已繪製|部分繪製|無法繪製|無法載入)/.test(document.getElementById('status').textContent), { timeout: 30000 });
    const status = await page.locator('#status').innerText();
    assert.equal(status, valid && peleValid ? '已繪製' : valid || peleValid ? '部分繪製成功，請查看個別結果' : '無法繪製，請查看錯誤訊息',
      `sample=${await page.locator('#sample').inputValue()}: ${await page.locator('#official-error').textContent()}`);
    assert.equal(await page.locator('#official > svg').count(), valid ? 1 : 0);
    assert.equal(await page.locator('#pele > svg').count(), peleValid ? 1 : 0);
    const exported = await page.locator('#export-engine').inputValue();
    assert.equal(await page.locator('#download').isEnabled(), exported === 'pele' ? peleValid : valid);
    assert.equal(await page.locator('#official-error').isVisible(), !valid);
    assert.equal(await page.locator('#pele-error').isVisible(), !peleValid);
  }
  await settled();
  const samples = [...manifest.samples, ...extraSamples.map((s, i) => ({ ...s, id: manifest.samples.length + i }))];
  let checks = 0;
  for (const theme of ['light', 'dark']) {
    if (theme === 'dark') {
      const selected = Number(await page.locator('#sample').inputValue());
      await page.locator('#theme').click(); await settled(!samples[selected].native?.dark.error);
    }
    for (const sample of samples) {
      await page.locator('#sample').selectOption(String(sample.id));
      await settled(!sample.native?.[theme].error);
      if (sample.native && !sample.native[theme].error) {
        await page.locator('#native').evaluate(async (image) => { await image.decode(); });
        assert.ok(await page.locator('#native').evaluate((image) => image.naturalWidth > 0));
      }
      if (sample.title.startsWith('AWS')) {
        for (const engine of ['official', 'pele']) {
          assert.ok(await page.locator(`#${engine} [fill="#ED7100"]`).count(), `${engine}: missing Lambda icon artwork`);
        }
        assert.equal(await page.locator('#native').evaluate((image) => getComputedStyle(image).display), 'none');
      }
      checks++;
    }
  }
  await page.locator('#sample').selectOption('0'); await settled();
  assert.ok((await page.locator('#official').innerText()).includes('開始'));
  await mkdir(new URL('artifacts/', import.meta.url), { recursive: true });
  await page.screenshot({ path: new URL('artifacts/dark.png', import.meta.url).pathname, fullPage: true });
  await page.locator('#palette').selectOption('rmdv'); await settled(); checks++;
  await page.locator('#layout').selectOption('elk'); await settled(); checks++;
  await page.locator('#sample').selectOption('4'); await settled(false, true);
  assert.ok((await page.locator('#official-error').innerText()).includes('切換 Dagre')); checks++;
  await page.locator('#layout').selectOption('dagre'); await settled(); checks++;
  await page.locator('#sample').selectOption('0'); await settled();
  await page.locator('#zoom').selectOption('2');
  assert.ok(await page.locator('#official-viewport').evaluate((element) => element.scrollWidth > element.clientWidth)); checks++;
  await page.locator('#zoom').selectOption('fit');
  await page.locator('#source').fill('flowchart TD\n 新增[新的中文節點] --> 完成[驗證成功]');
  assert.ok((await page.locator('#native-note').innerText()).includes('未套用'));
  assert.equal(await page.locator('#download').isEnabled(), false);
  await page.locator('#source').press('Control+Enter'); await settled();
  assert.ok((await page.locator('#official').innerText()).includes('驗證成功')); checks++;
  const downloadPromise = page.waitForEvent('download'); await page.locator('#download').click();
  const download = await downloadPromise;
  await download.saveAs(new URL('artifacts/export.svg', import.meta.url).pathname);
  assert.ok((await readFile(new URL('artifacts/export.svg', import.meta.url), 'utf8')).includes('<svg')); checks++;
  await page.locator('#sample').selectOption('7'); await settled();
  await page.locator('#export-engine').selectOption('pele');
  const peleDownloadPromise = page.waitForEvent('download'); await page.locator('#download').click();
  const peleDownload = await peleDownloadPromise;
  const pelePath = new URL('artifacts/pele-aws.svg', import.meta.url);
  await peleDownload.saveAs(pelePath.pathname);
  const peleExport = await readFile(pelePath, 'utf8');
  assert.ok(peleExport.includes('Arch_AWS-Lambda') && peleExport.includes('--pele-bg'));
  checks++;
  await page.locator('#source').fill('flowchart LR\n A@{ icon: "aws:missing-icon", label: "Unknown" }');
  await page.locator('#render').click(); await settled(false);
  assert.ok((await page.locator('#pele-error').innerText()).includes('找不到 AWS 圖示')); checks++;
  await page.locator('#reset').click(); await settled();
  await page.locator('#export-engine').selectOption('official');
  await page.locator('#source').fill('invalid diagram source'); await page.locator('#render').click(); await settled(false); checks++;
  await page.locator('#source').fill('x'.repeat(65537)); await page.locator('#render').click(); await settled(false);
  assert.ok((await page.locator('#official-error').innerText()).includes('64 KiB')); checks++;
  await page.locator('#reset').click(); await settled(); checks++;
  await page.locator('#theme').click(); await settled();
  await page.locator('#palette').selectOption('official'); await settled();
  await page.locator('#layout').selectOption('dagre'); await settled();
  await page.screenshot({ path: new URL('artifacts/light.png', import.meta.url).pathname, fullPage: true });
  // Rapid state changes must leave the latest diagram, not a stale completion.
  await page.locator('#sample').selectOption('1'); await page.locator('#sample').selectOption('0'); await settled();
  assert.ok((await page.locator('#official').innerText()).includes('開始')); checks++;
  await page.setViewportSize({ width: 390, height: 844 });
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)); checks++;
  assert.equal(await page.locator('header').count(), 1);
  await page.screenshot({ path: new URL('artifacts/mobile.png', import.meta.url).pathname });
  await page.locator('#official-viewport').scrollIntoViewIfNeeded();
  await page.screenshot({ path: new URL('artifacts/mobile-diagram.png', import.meta.url).pathname });
  assert.deepEqual(pageErrors, []);
  assert.deepEqual(externalRequests, []);
  assert.deepEqual(failedRequests, []);
  const result = { checks, cases: samples.length, themes: 2, browser: browser.version(),
    pageErrors, externalRequests, failedRequests };
  await writeFile(new URL('artifacts/verification.json', import.meta.url), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result, null, 2));
} finally {
  await browser?.close();
  server.kill();
}
