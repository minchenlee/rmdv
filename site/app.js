// rmdv.mclee.dev — keyboard layer. Mirrors the app's bindings.
(function () {
  'use strict';

  const mac = /Mac|iP(hone|ad|od)/.test(navigator.platform);
  const $ = (s) => document.querySelector(s);
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

  // Non-mac: show Ctrl instead of ⌘ on every keycap marked data-mod.
  if (!mac) {
    document.querySelectorAll('kbd[data-mod]').forEach((k) => {
      if (k.textContent === '⌘') k.textContent = 'Ctrl';
    });
  }

  // ── theme ──────────────────────────────────────────────
  function setTheme(t) {
    document.documentElement.dataset.theme = t;
    localStorage.setItem('rmdv-theme', t);
    const shot = $('#shot-img');
    if (shot) shot.src = shot.dataset[t === 'dark' ? 'dark' : 'light'];
  }
  function toggleTheme() {
    const next = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
    // View Transition crossfade where supported — a snap, not a lerp.
    if (document.startViewTransition && !reduced) {
      document.startViewTransition(() => setTheme(next));
    } else {
      setTheme(next);
    }
  }

  // ── keycap echo: pressing a real key depresses its keycap ──
  function echo(ch) {
    document.querySelectorAll('kbd[data-key]').forEach((k) => {
      if (k.dataset.key !== ch) return;
      k.classList.add('hit');
      setTimeout(() => k.classList.remove('hit'), 160);
    });
  }

  // ── scroll reveal — once, fast, subtle ─────────────────
  const revealed = document.querySelectorAll('.reveal');
  if ('IntersectionObserver' in window && !reduced) {
    const io = new IntersectionObserver((entries) => {
      entries.forEach((e) => {
        if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); }
      });
    }, { rootMargin: '0px 0px -8% 0px' });
    revealed.forEach((el) => io.observe(el));
  } else {
    revealed.forEach((el) => el.classList.add('in'));
  }

  // ── scrollspy: highlight the section under the cursor ──
  const navLinks = [...document.querySelectorAll('.top-nav a[href^="#"]')];
  const spied = navLinks
    .map((a) => document.getElementById(a.hash.slice(1)))
    .filter(Boolean);
  if ('IntersectionObserver' in window && spied.length) {
    const spy = new IntersectionObserver((entries) => {
      entries.forEach((e) => {
        if (!e.isIntersecting) return;
        navLinks.forEach((a) => a.classList.toggle('active', a.hash === '#' + e.target.id));
      });
    }, { rootMargin: '-30% 0px -60% 0px' });
    spied.forEach((s) => spy.observe(s));
  }

  // Nav clicks glide; j/k stays instant (CSS scroll-behavior is auto).
  navLinks.forEach((a) =>
    a.addEventListener('click', (e) => {
      const el = document.getElementById(a.hash.slice(1));
      if (!el) return;
      e.preventDefault();
      el.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'start' });
      history.replaceState(null, '', a.hash);
    })
  );

  // ── resolve real download URLs from the latest release ─
  // Progressive enhancement: buttons already link to the releases page, so
  // this is deferred until the visitor scrolls to Install or interacts. The
  // GitHub API rate-limits unauthenticated callers (a 403 the browser logs as
  // a console error); firing lazily keeps it off the initial page load.
  let releasesResolved = false;
  function resolveReleases() {
    if (releasesResolved) return;
    releasesResolved = true;
    fetch('https://api.github.com/repos/minchenlee/rmdv/releases/latest',
      typeof AbortSignal.timeout === 'function' ? { signal: AbortSignal.timeout(5000) } : {})
      .then((r) => (r.ok ? r.json() : null))
      .then((rel) => {
        if (!rel || !rel.assets) return;
        document.querySelectorAll('[data-asset]').forEach((a) => {
          const suffix = a.dataset.asset;
          const hit = rel.assets.find((as) => as.name.endsWith(suffix));
          if (hit) a.href = hit.browser_download_url;
        });
        // Primary button stays OS-agnostic: it just scrolls to the Install
        // section (href="#install"). Only stamp the version into its label.
        const primary = $('#dl-primary');
        if (rel.tag_name) primary.textContent = 'Download ' + rel.tag_name;
      })
      .catch(() => {});
  }

  const installSection = document.getElementById('install');
  if ('IntersectionObserver' in window && installSection) {
    const ro = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) { resolveReleases(); ro.disconnect(); }
    });
    ro.observe(installSection);
  } else {
    resolveReleases();
  }
  ['pointerdown', 'keydown', 'focusin'].forEach((ev) =>
    window.addEventListener(ev, resolveReleases, { once: true, passive: true })
  );

  // ── overlays ───────────────────────────────────────────
  const palette = $('#palette');
  const cheat = $('#cheat');
  const input = $('#palette-input');
  const list = $('#palette-list');
  let lastFocus = null;

  const status = $('#sr-status');

  function openOverlay(el) {
    closeOverlays();
    lastFocus = document.activeElement;
    el.hidden = false;
    if (el === palette) {
      input.value = '';
      render('');
      input.focus();
    } else {
      el.querySelector('.panel').focus({ preventScroll: true });
      status.textContent = 'Keyboard shortcuts dialog opened';
    }
  }
  function closeOverlays() {
    let was = !palette.hidden || !cheat.hidden;
    palette.hidden = true;
    cheat.hidden = true;
    if (was) {
      status.textContent = '';
      if (lastFocus) lastFocus.focus({ preventScroll: true });
    }
    return was;
  }
  [palette, cheat].forEach((ov) =>
    ov.addEventListener('mousedown', (e) => { if (e.target === ov) closeOverlays(); })
  );

  // ── command palette ────────────────────────────────────
  const sections = [...document.querySelectorAll('section[id], footer')].map((s) => ({
    label: 'Go to: ' + (s.querySelector('h2')?.textContent.replace('##', '').trim() || 'Footer'),
    hint: '',
    run: () => s.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'start' }),
  }));
  const commands = [
    { label: 'Toggle theme', hint: 't', run: toggleTheme },
    { label: 'Go to top', hint: 'g', run: () => window.scrollTo({ top: 0 }) },
    ...sections,
    { label: 'Download latest release', hint: '', run: () => { location.href = $('#dl-primary').href; } },
    { label: 'Open GitHub repository', hint: '', run: () => { location.href = 'https://github.com/minchenlee/rmdv'; } },
    { label: 'View releases / changelog', hint: '', run: () => { location.href = 'https://github.com/minchenlee/rmdv/releases'; } },
    { label: 'Copy: cargo build --release', hint: '', run: () => navigator.clipboard?.writeText('git clone https://github.com/minchenlee/rmdv && cd rmdv && cargo build --release') },
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
      li.id = 'cmd-' + i;
      li.setAttribute('role', 'option');
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
    status.textContent = filtered.length + (filtered.length === 1 ? ' command' : ' commands');
    paint();
  }
  function paint() {
    [...list.children].forEach((li, i) => {
      li.classList.toggle('sel', i === sel);
      li.setAttribute('aria-selected', i === sel ? 'true' : 'false');
    });
    const cur = list.children[sel];
    input.setAttribute('aria-activedescendant', cur ? cur.id : '');
    cur?.scrollIntoView({ block: 'nearest' });
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
    const mod = mac ? e.metaKey : e.ctrlKey;
    // ⌘⇧P — the app's own palette binding. The site teaches only real keys.
    const paletteKey = mod && e.shiftKey && e.key.toLowerCase() === 'p';
    if (paletteKey) {
      e.preventDefault();
      palette.hidden ? openOverlay(palette) : closeOverlays();
      return;
    }

    if (e.key === 'Escape') { if (closeOverlays()) e.preventDefault(); return; }

    // Everything below: plain keys only — never steal typing from the input.
    if (!palette.hidden || !cheat.hidden) return;
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName)) return;

    switch (e.key) {
      case 'j': window.scrollBy({ top: SCROLL }); echo('j'); break;
      case 'k': window.scrollBy({ top: -SCROLL }); echo('k'); break;
      case 'g': window.scrollTo({ top: 0 }); echo('g'); break;
      case 'G': window.scrollTo({ top: document.body.scrollHeight }); echo('G'); break;
      case ' ':
        window.scrollBy({ top: (e.shiftKey ? -1 : 1) * innerHeight * 0.85 });
        e.preventDefault();
        break;
      case 't': toggleTheme(); echo('t'); break;
      case '?': openOverlay(cheat); echo('?'); break;
      default: return;
    }
  });

  const paletteHint = $('#palette-hint');
  if (paletteHint) paletteHint.addEventListener('click', () => openOverlay(palette));

  // ── feature carousels — snap track + buttons + dots ────
  document.querySelectorAll('[data-carousel]').forEach((root) => {
    const track = root.querySelector('[data-track]');
    const slides = Array.from(track.children);
    const prev = root.querySelector('[data-prev]');
    const next = root.querySelector('[data-next]');
    const dotsWrap = root.querySelector('[data-dots]');
    if (!track || slides.length < 2) return;

    let index = 0;

    const dots = slides.map((_, i) => {
      const d = document.createElement('button');
      d.type = 'button';
      // Dots live in an aria-hidden container (prev/next already expose nav to
      // assistive tech); keep them out of the tab order too.
      d.tabIndex = -1;
      d.setAttribute('aria-label', 'Show feature ' + (i + 1));
      d.addEventListener('click', () => go(i));
      dotsWrap.appendChild(d);
      return d;
    });

    function go(i) {
      index = Math.max(0, Math.min(slides.length - 1, i));
      track.scrollTo({ left: slides[index].offsetLeft, behavior: reduced ? 'auto' : 'smooth' });
      sync();
    }

    function sync() {
      prev.disabled = index === 0;
      next.disabled = index === slides.length - 1;
      dots.forEach((d, i) => d.setAttribute('aria-current', i === index ? 'true' : 'false'));
    }

    prev.addEventListener('click', () => go(index - 1));
    next.addEventListener('click', () => go(index + 1));

    // keep dots/buttons in sync when the user swipes the track directly
    let raf = 0;
    track.addEventListener('scroll', () => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        const mid = track.scrollLeft + track.clientWidth / 2;
        let nearest = 0, best = Infinity;
        slides.forEach((s, i) => {
          const c = s.offsetLeft + s.offsetWidth / 2;
          const dist = Math.abs(c - mid);
          if (dist < best) { best = dist; nearest = i; }
        });
        if (nearest !== index) { index = nearest; sync(); }
      });
    });

    sync();
  });

  // ── image lightbox — click a slide shot to view full-size ──
  const lightbox = $('#lightbox');
  if (lightbox) {
    const lbImg = $('#lightbox-img');
    const stage = $('#lightbox-stage');
    const closeBtn = $('#lightbox-close');
    let lastFocus = null;

    function openLightbox(src, alt) {
      lastFocus = document.activeElement;
      lbImg.src = src;
      lbImg.alt = alt || '';
      stage.classList.remove('zoomed');
      lightbox.hidden = false;
      requestAnimationFrame(() => lightbox.classList.add('in'));
      closeBtn.focus();
    }

    function closeLightbox() {
      lightbox.classList.remove('in');
      const finish = () => {
        lightbox.hidden = true;
        lbImg.src = '';
        stage.classList.remove('zoomed');
        if (lastFocus && lastFocus.focus) lastFocus.focus();
      };
      // Wait for the fade-out, then hide. Fixed timeout (not transitionend)
      // so an interrupted/absent transition can't leave it stuck open.
      if (reduced) finish();
      else setTimeout(finish, 180);
    }

    document.querySelectorAll('.slide-shot img').forEach((img) => {
      img.addEventListener('click', () => openLightbox(img.currentSrc || img.src, img.alt));
    });

    // Click the image toggles native-size zoom; the stage scrolls when zoomed.
    lbImg.addEventListener('click', (e) => {
      e.stopPropagation();
      stage.classList.toggle('zoomed');
    });
    // Click anywhere off the image (backdrop / stage padding) closes.
    stage.addEventListener('click', closeLightbox);
    closeBtn.addEventListener('click', closeLightbox);
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && !lightbox.hidden) { e.preventDefault(); closeLightbox(); }
    });
  }
})();
