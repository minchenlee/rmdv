// rmdv.mclee.dev — footer ASCII wordmark. A Ghostty-style homage: a plasma noise
// field flowing through a fixed ghost silhouette, rendered into a <pre>.
//
// Perf contract (this site targets 100/100 PageSpeed):
//   • loaded with `defer` — off the critical path, never render-blocking
//   • no network, no fonts, no extra requests; ~4 KB of JS, that's the whole cost
//   • 30 fps accumulator throttle (never an uncapped rAF)
//   • prefers-reduced-motion → one static frame, ticker never starts
//   • paused when the tab is hidden, the window is blurred, or the art scrolls
//     out of view (IntersectionObserver) — idles at ~0% CPU
//   • the <pre> reserves its height in CSS, so animating it causes zero CLS
//   • lives after the install section and does not touch the LCP element
(function () {
  'use strict';

  const art = document.getElementById('footer-wordmark');
  if (!art) return;

  // ── 30 fps accumulator ticker (mirrors Ghostty's own throttle) ──────────────
  class Ticker {
    constructor(cb, fps) {
      this.cb = cb;
      this.frameTime = 1000 / fps;
      this.last = -1;
      this.raf = null;
      this.tick = this.tick.bind(this);
    }
    start() { if (this.raf == null) this.raf = requestAnimationFrame(this.tick); }
    pause() { if (this.raf != null) { cancelAnimationFrame(this.raf); this.raf = null; this.last = -1; } }
    tick(t) {
      if (this.last < 0) this.last = t;
      let d = t - this.last;
      // Clamp catch-up so a long pause can't fire a burst of frames on resume.
      if (d > 250) { this.last = t; d = this.frameTime; }
      while (d >= this.frameTime) { this.cb(); d -= this.frameTime; this.last += this.frameTime; }
      this.raf = requestAnimationFrame(this.tick);
    }
  }

  // ── "rmdv" wordmark mask (Alien Block, baked at build time) ─────────────────
  // The silhouette is the word "rmdv" set in the Alien Block display face. The
  // bitmap was rendered offscreen once and frozen here, so NO font is downloaded
  // at runtime — the plasma simply flows through these heavy glyph shapes exactly
  // as it did the ghost. Each string is one grid row; '1' = inside a letter.
  const GLYPHS = [
    '000000000000000000000000000000000000000000000000000000011111111111111110000000000000000000000000',
    '000000000000000000000000000000000000000000000000000000111111111111111111000000000000000000000000',
    '000000000000000000000000000000000000000000000000000000111111111111111111000000000000000000000000',
    '000000000000000000000000000000000000000000000000000000111111111111111111000000000000000000000000',
    '000000000000000000000000000000000000000000000000000000111111111111111111000000000000000000000000',
    '000000000000000000000000000000000000000000000000000000111111111111111111000000000000000000000000',
    '000011111111111111111110011111111000111111110000000000111111111111111111011111111111111111111110',
    '000111111111111111111111111111111110111111111100000011111111111111111111111111111111111111111111',
    '001111111111111111111111111111111110111111111110000111111111111111111111111111111111111111111111',
    '011111111111111111111111111111111111111111111110001111111111111111111111111111111111111111111111',
    '011111111111111111111111111111111111111111111111011111111111111111111111111111111111111111111111',
    '111111111111111111111111111111111111111111111111011111111111111111111111111111111111111111111111',
    '111111111111111111000000111111111111111111111111011111111111111111111111111111111111111111111111',
    '111111111111111111000000111111111111111111111111111111111111111111111111111111111111111111111110',
    '111111111111111111000000111111111111111111111111111111111111111111111111111111111111111111111110',
    '111111111111111111000000111111111111111111111111111111111111111111111111111111111111111111111110',
    '111111111111111111000000111111111111111111111111011111111111111111111111111111111111111111111110',
    '111111111111111111000000111111111111111111111111011111111111111111111111111111111111111111111100',
    '111111111111111111000000111111111111111111111111011111111111111111111111111111111111111111111100',
    '111111111111111111000000111111111111111111111111001111111111111111111111111111111111111111111000',
    '111111111111111111000000111111111111111111111111000111111111111111111111111111111111111111110000',
    '111111111111111111000000111111111111111111111111000011111111111111111111111111111111111111100000',
    '111111111111111111000000111111111111111111111111000001111111111111111111111111111111111110000000',
    '011111111111111110000000011111111111111111111110000000001111111111111110011111111111100000000000',
  ];
  const GW = GLYPHS[0].length;            // 96
  const GH = GLYPHS.length;               // 23
  const PAD_Y = 4;                        // blank rows above & below the word
  const COLS = GW;                        // 96
  const ROWS = GH + PAD_Y * 2;            // 31
  const RAMP = ' .`·~:-=+oxzXO%$#@';
  const RAMP_LEN = RAMP.length;
  const ASPECT = 0.52;                    // on-screen col:row width ratio

  const mask = new Uint8Array(COLS * ROWS);
  for (let gr = 0; gr < GH; gr++) {
    const row = GLYPHS[gr];
    const r = gr + PAD_Y;
    for (let c = 0; c < GW; c++) if (row.charCodeAt(c) === 49 /* '1' */) mask[r * COLS + c] = 1;
  }

  // ── value-noise plasma (integer hash, 2-octave fBm) ─────────────────────────
  function hash21(x, y) {
    let h = (x * 374761393 + y * 668265263) | 0;
    h = (h ^ (h >>> 13)) | 0;
    h = Math.imul(h, 1274126177) | 0;
    h = (h ^ (h >>> 16)) | 0;
    return (h >>> 0) / 0xFFFFFFFF;
  }
  function valueNoise(x, y) {
    const ix = Math.floor(x) | 0, iy = Math.floor(y) | 0;
    const fx = x - ix, fy = y - iy;
    const ux = fx * fx * (3 - 2 * fx), uy = fy * fy * (3 - 2 * fy);
    const a = hash21(ix, iy), b = hash21(ix + 1, iy);
    const c = hash21(ix, iy + 1), d = hash21(ix + 1, iy + 1);
    return a + (b - a) * ux + (c - a) * uy + (d - b - c + a) * ux * uy;
  }
  function fbm(x, y) {
    return valueNoise(x, y) * 0.6 + valueNoise(x * 2.1, y * 2.1) * 0.3 + valueNoise(x * 4.3, y * 4.3) * 0.1;
  }

  // ── render ──────────────────────────────────────────────────────────────────
  // Colors come from CSS classes (.gd / .ga / .gb in style.css), NOT inline
  // style attributes — the site's CSP forbids inline styles. The classes read
  // --muted / --accent / --accent-text, so they track the theme automatically.
  let tick = 0, timeA = 0, timeB = 0;

  function spanFor(tier, text) {
    if (tier === 0) return text;
    const cls = tier === 3 ? 'gb' : tier === 2 ? 'ga' : 'gd';
    return '<span class="' + cls + '">' + text + '</span>';
  }

  function renderFrame() {
    tick++;
    timeA += 0.036;   // 2× the original flow speed (was 0.018)
    timeB += 0.022;   // 2× the original flow speed (was 0.011)
    const scaleA = 0.11, scaleB = 0.085;
    let html = '';
    for (let r = 0; r < ROWS; r++) {
      if (r > 0) html += '\n';
      let runColor = -1, runText = '';
      for (let c = 0; c < COLS; c++) {
        const inside = mask[r * COLS + c] === 1;
        let tier, ch;
        if (!inside) {
          tier = 0; ch = ' ';
        } else {
          const nA = fbm(c * scaleA + timeA * 0.7, r * scaleA + timeA * 0.5);
          const nB = fbm(c * scaleB - timeB * 0.6, r * scaleB + timeB * 0.4);
          const noise = nA * 0.62 + nB * 0.38;
          ch = RAMP[Math.max(1, Math.min(RAMP_LEN - 1, (noise * (RAMP_LEN - 1) + 0.5) | 0))];
          tier = noise > 0.78 ? 3 : noise > 0.55 ? 2 : 1;
        }
        if (tier === runColor) { runText += ch; }
        else { if (runText) html += spanFor(runColor, runText); runColor = tier; runText = ch; }
      }
      if (runText) html += spanFor(runColor, runText);
    }
    art.innerHTML = html;
  }

  // ── gating: reduced-motion / visibility / blur / offscreen ──────────────────
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

  // Boot is deferred to browser idle so the first paint never lands inside the
  // page-load window — keeps Total Blocking Time clean. The art sits in the
  // footer; nothing above it waits on the animation.
  function boot() {
    if (reduced) {
      renderFrame();
    } else {
      const ticker = new Ticker(renderFrame, 30);
      let visible = document.visibilityState === 'visible';
      let inView = false;
      const update = () => { (visible && inView) ? ticker.start() : ticker.pause(); };

      document.addEventListener('visibilitychange', () => { visible = document.visibilityState === 'visible'; update(); });
      window.addEventListener('blur', () => { visible = false; update(); });
      window.addEventListener('focus', () => { visible = true; update(); });

      if ('IntersectionObserver' in window) {
        new IntersectionObserver((es) => { inView = es[0].isIntersecting; update(); }, { threshold: 0.02 }).observe(art);
      } else {
        inView = true;
      }
      renderFrame();
      update();
    }
    // Theme tracking is automatic: the .gd/.ga/.gb classes resolve --muted /
    // --accent / --accent-text, which the site's theme toggle already swaps.
  }

  if ('requestIdleCallback' in window) {
    requestIdleCallback(boot, { timeout: 2000 });
  } else {
    setTimeout(boot, 200);
  }
})();
