// mdv.mclee.dev — keyboard layer. Mirrors the app's bindings.
(function () {
  'use strict';

  const mac = /Mac|iP(hone|ad|od)/.test(navigator.platform);
  const $ = (s) => document.querySelector(s);

  // Non-mac: show Ctrl instead of ⌘ on every keycap marked data-mod.
  if (!mac) {
    document.querySelectorAll('kbd[data-mod]').forEach((k) => {
      if (k.textContent === '⌘') k.textContent = 'Ctrl';
    });
  }

  // ── theme ──────────────────────────────────────────────
  function setTheme(t) {
    document.documentElement.dataset.theme = t;
    localStorage.setItem('mdv-theme', t);
  }
  function toggleTheme() {
    setTheme(document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark');
  }

  // ── resolve real download URLs from the latest release ─
  // Progressive enhancement: buttons already link to the releases page.
  fetch('https://api.github.com/repos/minchenlee/mdv/releases/latest')
    .then((r) => (r.ok ? r.json() : null))
    .then((rel) => {
      if (!rel || !rel.assets) return;
      document.querySelectorAll('[data-asset]').forEach((a) => {
        const suffix = a.dataset.asset;
        const hit = rel.assets.find((as) => as.name.endsWith(suffix));
        if (hit) a.href = hit.browser_download_url;
      });
      // Primary button: pick the asset for this OS when we can tell.
      const primary = $('#dl-primary');
      const ua = navigator.userAgent;
      let suffix = null;
      if (mac) suffix = 'aarch64.dmg'; // most Macs now; Intel users get the page
      else if (/Linux/.test(ua) && !/Android/.test(ua)) suffix = 'x86_64.AppImage';
      const hit = suffix && rel.assets.find((as) => as.name.endsWith(suffix));
      if (hit) {
        primary.href = hit.browser_download_url;
        primary.textContent = 'Download ' + rel.tag_name + (mac ? ' for macOS' : ' for Linux');
      }
    })
    .catch(() => {});

  // ── overlays ───────────────────────────────────────────
  const palette = $('#palette');
  const cheat = $('#cheat');
  const input = $('#palette-input');
  const list = $('#palette-list');
  let lastFocus = null;

  function openOverlay(el) {
    closeOverlays();
    lastFocus = document.activeElement;
    el.hidden = false;
    if (el === palette) {
      input.value = '';
      render('');
      input.focus();
    }
  }
  function closeOverlays() {
    let was = !palette.hidden || !cheat.hidden;
    palette.hidden = true;
    cheat.hidden = true;
    if (was && lastFocus) lastFocus.focus({ preventScroll: true });
    return was;
  }
  [palette, cheat].forEach((ov) =>
    ov.addEventListener('mousedown', (e) => { if (e.target === ov) closeOverlays(); })
  );

  // ── command palette ────────────────────────────────────
  const sections = [...document.querySelectorAll('section[id], footer')].map((s) => ({
    label: 'Go to: ' + (s.querySelector('h2')?.textContent.replace('##', '').trim() || 'Footer'),
    hint: '',
    run: () => s.scrollIntoView({ block: 'start' }),
  }));
  const commands = [
    { label: 'Toggle theme', hint: 't', run: toggleTheme },
    { label: 'Go to top', hint: 'g', run: () => window.scrollTo({ top: 0 }) },
    ...sections,
    { label: 'Download latest release', hint: '', run: () => { location.href = $('#dl-primary').href; } },
    { label: 'Open GitHub repository', hint: '', run: () => { location.href = 'https://github.com/minchenlee/mdv'; } },
    { label: 'View releases / changelog', hint: '', run: () => { location.href = 'https://github.com/minchenlee/mdv/releases'; } },
    { label: 'Copy: cargo build --release', hint: '', run: () => navigator.clipboard?.writeText('git clone https://github.com/minchenlee/mdv && cd mdv && cargo build --release') },
    { label: 'Keyboard shortcuts', hint: '?', run: () => openOverlay(cheat) },
  ];

  let filtered = commands;
  let sel = 0;

  // Subsequence fuzzy match, the same feel as the app's ⌘P.
  function fuzzy(q, s) {
    q = q.toLowerCase(); s = s.toLowerCase();
    let i = 0;
    for (const ch of s) if (ch === q[i]) i++;
    return i === q.length;
  }

  function render(q) {
    filtered = q ? commands.filter((c) => fuzzy(q, c.label)) : commands;
    sel = Math.min(sel, Math.max(0, filtered.length - 1));
    list.innerHTML = '';
    if (!filtered.length) {
      const li = document.createElement('li');
      li.className = 'empty';
      li.textContent = 'No matching commands';
      list.appendChild(li);
      return;
    }
    filtered.forEach((c, i) => {
      const li = document.createElement('li');
      if (i === sel) li.className = 'sel';
      const name = document.createElement('span');
      name.textContent = c.label;
      li.appendChild(name);
      if (c.hint) {
        const k = document.createElement('span');
        k.className = 'pk';
        k.textContent = c.hint;
        li.appendChild(k);
      }
      li.addEventListener('mousemove', () => { sel = i; paint(); });
      li.addEventListener('click', () => { closeOverlays(); c.run(); });
      list.appendChild(li);
    });
    paint();
  }
  function paint() {
    [...list.children].forEach((li, i) => li.classList.toggle('sel', i === sel));
    list.children[sel]?.scrollIntoView({ block: 'nearest' });
  }

  input.addEventListener('input', () => { sel = 0; render(input.value); });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown' || (e.ctrlKey && e.key === 'n')) {
      sel = (sel + 1) % filtered.length; paint(); e.preventDefault();
    } else if (e.key === 'ArrowUp' || (e.ctrlKey && e.key === 'p')) {
      sel = (sel - 1 + filtered.length) % filtered.length; paint(); e.preventDefault();
    } else if (e.key === 'Enter') {
      const c = filtered[sel];
      if (c) { closeOverlays(); c.run(); }
      e.preventDefault();
    }
  });

  // ── global keys ────────────────────────────────────────
  const SCROLL = 90;
  document.addEventListener('keydown', (e) => {
    const modK = (mac ? e.metaKey : e.ctrlKey) && e.key.toLowerCase() === 'k';
    if (modK) { e.preventDefault(); palette.hidden ? openOverlay(palette) : closeOverlays(); return; }

    if (e.key === 'Escape') { if (closeOverlays()) e.preventDefault(); return; }

    // Everything below: plain keys only — never steal typing from the input.
    if (!palette.hidden || !cheat.hidden) return;
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName)) return;

    switch (e.key) {
      case 'j': window.scrollBy({ top: SCROLL }); break;
      case 'k': window.scrollBy({ top: -SCROLL }); break;
      case 'g': window.scrollTo({ top: 0 }); break;
      case 'G': window.scrollTo({ top: document.body.scrollHeight }); break;
      case ' ':
        window.scrollBy({ top: (e.shiftKey ? -1 : 1) * innerHeight * 0.85 });
        e.preventDefault();
        break;
      case 't': toggleTheme(); break;
      case '?': openOverlay(cheat); break;
      default: return;
    }
  });

  $('#palette-hint').addEventListener('click', () => openOverlay(palette));
})();
