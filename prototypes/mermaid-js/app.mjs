import { loadAwsIcons } from './icons.mjs';
import { extraSamples } from './extra-samples.mjs';

const $ = (id) => document.getElementById(id);
let manifest, mermaid, pele, aws, serial = 0, latest = 0;
let queue = Promise.resolve();
let theme = 'light';
const outputs = { official: '', pele: '' };

function over(color, background) {
  const alpha = Math.max(0, Math.min(1, color[3]));
  return color.slice(0, 3).map((channel, i) => channel * alpha + background[i] * (1 - alpha)).concat(1);
}
function hex(color) {
  return '#' + color.slice(0, 3).map((n) => Math.round(Math.max(0, Math.min(1, n)) * 255).toString(16).padStart(2, '0')).join('');
}
function themeVariables(p) {
  const fill = over(p.surface, p.bg);
  return { background: hex(p.bg), primaryColor: hex(fill), primaryTextColor: hex(over(p.fg, fill)),
    primaryBorderColor: hex(over(p.border, fill)), lineColor: hex(over(p.muted, p.bg)),
    secondaryColor: hex(over(p.surfaceAlt, p.bg)), tertiaryColor: hex(over(p.accent, p.bg)),
    fontFamily: 'Helvetica, PingFang TC, sans-serif' };
}
function peleVariables(state) {
  if (state.palette === 'rmdv') {
    const p = manifest.palettes[state.theme], fill = over(p.surface, p.bg);
    return { bg: hex(p.bg), fg: hex(over(p.fg, fill)), muted: hex(over(p.muted, p.bg)),
      line: hex(over(p.muted, p.bg)), surface: hex(fill), 'surface-alt': hex(over(p.surfaceAlt, p.bg)),
      border: hex(over(p.border, fill)), accent: hex(over(p.accent, p.bg)) };
  }
  return state.theme === 'light'
    ? { bg: '#ffffff', fg: '#1f1f1f', muted: '#6e6e6e', line: '#1f1f1f', surface: '#f3f3f3', 'surface-alt': '#e6e6e6', border: '#8a8a8a', accent: '#5b7bd5' }
    : { bg: '#1f2024', fg: '#dedfe3', muted: '#a4a6ad', line: '#dedfe3', surface: '#282b30', 'surface-alt': '#32363c', border: '#737984', accent: '#a5b7ef' };
}
function updateDownload() { $('download').disabled = !outputs[$('export-engine').value]; }
function clearPanel(engine) {
  outputs[engine] = '';
  $(engine).replaceChildren();
  $(`${engine}-error`).hidden = true;
}
function showError(engine, error) {
  clearPanel(engine);
  $(`${engine}-error`).textContent = String(error.message || error).slice(0, 4000);
  $(`${engine}-error`).hidden = false;
}
function updateReference() {
  const sample = manifest.samples[Number($('sample').value)];
  const native = sample.native?.[theme];
  const edited = $('source').value !== sample.source;
  $('native-note').textContent = !native ? '尚未建立這個範例的 Rust 圖示參考'
    : edited ? '原始範例參考・未套用編輯後的內容' : 'rmdv 配色・相同原始碼的固定參考';
  $('native-viewport').style.background = hex(manifest.palettes[theme].bg);
  $('native-error').hidden = Boolean(native && !native.error);
  $('native-error').textContent = native?.error || (!native ? '這個新範例尚未建立 Rust 圖示參考。' : '');
  $('native').hidden = !native?.file;
  if (native?.file) $('native').src = native.file;
  applyZoom();
}
function applyZoom() {
  const value = $('zoom').value;
  for (const engine of ['official', 'pele']) {
    const svg = $(engine).querySelector('svg');
    if (!svg) continue;
    const width = svg.viewBox.baseVal.width || Number.parseFloat(svg.getAttribute('width')) || 800;
    svg.style.maxWidth = value === 'fit' ? '100%' : 'none';
    svg.style.width = value === 'fit' ? '100%' : `${width * Number(value)}px`;
    $(engine).style.width = value === 'fit' ? '100%' : `${width * Number(value)}px`;
  }
  const native = manifest?.samples[Number($('sample').value)].native?.[theme];
  if (native?.width) {
    $('native').style.maxWidth = value === 'fit' ? '100%' : 'none';
    $('native').style.width = value === 'fit' ? '' : `${native.width / 2 * Number(value)}px`;
  }
}
function renderPele(state) {
  const result = pele.render(state.source, {
    icons: aws.resolve, idPrefix: `pele-diagram-${++serial}`, fontFamily: 'Helvetica, PingFang TC, sans-serif',
    autoDirection: false, responsive: false, limit: 65536, outputLimit: 4 * 1024 * 1024,
    maxEdges: 500, links: false, images: false,
  });
  if (state.revision !== latest) return;
  $('pele').innerHTML = result.svg;
  const variables = peleVariables(state), svg = $('pele').querySelector('svg');
  for (const [key, value] of Object.entries(variables)) svg.style.setProperty(`--pele-${key}`, value);
  $('pele-viewport').style.background = variables.bg;
  // Freeze CSS variables into the exported SVG, instead of relying on page CSS.
  outputs.pele = new XMLSerializer().serializeToString(svg);
  $('pele-note').textContent = /themeVariables/.test(state.source)
    ? 'Pele 使用 CSS 配色，不套用 Mermaid 的 themeVariables'
    : `${state.palette === 'rmdv' ? 'rmdv 配色' : 'Pele 配色'}・Pele 獨立排版`;
}
async function renderOfficial(state) {
  let staging;
  try {
    const hasInit = state.source.trimStart().startsWith('%%{init');
    const custom = state.palette === 'rmdv' && !hasInit;
    mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', suppressErrorRendering: true,
      maxTextSize: 65536, maxEdges: 500, layout: state.layout,
      theme: custom ? 'base' : state.theme === 'dark' ? 'dark' : 'default',
      fontFamily: 'Helvetica, PingFang TC, sans-serif',
      ...(custom ? { themeVariables: themeVariables(manifest.palettes[state.theme]) } : {}) });
    await mermaid.parse(state.source);
    if (state.revision !== latest) return;
    staging = document.createElement('div');
    staging.style.cssText = 'position:fixed;left:-100000px;top:0;width:1000px;visibility:hidden';
    document.body.append(staging);
    const result = await mermaid.render(`official-diagram-${++serial}`, state.source, staging);
    if (state.revision !== latest) return;
    if (result.svg.length > 4 * 1024 * 1024) throw new Error('SVG 超過 4 MiB，請簡化圖表。');
    outputs.official = result.svg;
    $('official').innerHTML = result.svg;
    $('official-viewport').style.background = custom ? hex(manifest.palettes[state.theme].bg)
      : state.theme === 'dark' ? '#1f2024' : '#ffffff';
    $('official-note').textContent = hasInit ? '原始碼內的 init 設定優先'
      : `${custom ? 'rmdv 配色' : 'Mermaid 配色'}・${state.layout === 'elk' ? 'ELK' : 'Dagre'} 排版`;
  } catch (error) {
    if (state.revision !== latest) return;
    const detail = String(error.message || error);
    const elkMindmapIssue = state.layout === 'elk' && /^mindmap\b/m.test(state.source) && detail.includes("reading 're'");
    showError('official', elkMindmapIssue ? `官方版 ELK 在這個心智圖發生錯誤，請切換 Dagre 再試。\n${detail}` : detail);
  } finally { staging?.remove(); }
}
function render() {
  updateReference();
  const state = { revision: ++latest, source: $('source').value, theme,
    palette: $('palette').value, layout: $('layout').value };
  for (const engine of ['official', 'pele']) clearPanel(engine);
  $('status').textContent = '正在繪製…';
  updateDownload();
  queue = queue.catch(() => {}).then(async () => {
    if (state.revision !== latest) return;
    try {
      if (new TextEncoder().encode(state.source).length > 65536) throw new Error('原始碼超過 64 KiB，請縮短後再試。');
      if (!state.source.trim()) throw new Error('請輸入 Mermaid 原始碼。');
      aws.validate(state.source);
      await document.fonts.ready;
      if (state.revision !== latest) return;
      try { renderPele(state); } catch (error) { showError('pele', error); }
      await renderOfficial(state);
    } catch (error) {
      if (state.revision !== latest) return;
      for (const engine of ['official', 'pele']) showError(engine, error);
    }
    if (state.revision !== latest) return;
    const successes = Object.values(outputs).filter(Boolean).length;
    $('status').textContent = successes === 2 ? '已繪製' : successes === 1 ? '部分繪製成功，請查看個別結果' : '無法繪製，請查看錯誤訊息';
    applyZoom(); updateDownload();
  });
  return queue;
}
function loadSample() {
  $('source').value = manifest.samples[Number($('sample').value)].source;
  return render();
}
function updateCatalog() {
  const term = $('icon-search').value.toLowerCase();
  const matches = aws.names.filter((name) => name.includes(term)).slice(0, 10);
  $('icon-names').replaceChildren(...matches.map((name) => {
    const li = document.createElement('li'); li.textContent = `aws:${name}`; return li;
  }));
}
try {
  const response = await fetch('/reference/manifest.json');
  if (!response.ok) throw new Error('找不到比較範例。請先依 README 產生參考圖片。');
  manifest = await response.json();
  [ { default: mermaid }, pele, aws ] = await Promise.all([
    import('./node_modules/mermaid/dist/mermaid.esm.min.mjs'),
    import('./node_modules/pele/dist/pele.js'), loadAwsIcons(),
  ]);
  mermaid.registerIconPacks([{ name: 'aws', icons: aws.pack }]);
  $('icon-count').textContent = `${Object.keys(aws.pack.icons).length} 個 AWS Labs 圖示，另提供 arch-* 名稱別名。`;
  updateCatalog();
  for (const sample of extraSamples) manifest.samples.push({ ...sample, id: manifest.samples.length });
  for (const sample of manifest.samples) $('sample').add(new Option(sample.title, sample.id));
  for (const id of ['sample', 'source', 'render', 'reset']) $(id).disabled = false;
  $('sample').addEventListener('change', loadSample);
  $('reset').addEventListener('click', loadSample);
  $('render').addEventListener('click', render);
  $('source').addEventListener('input', () => {
    ++latest;
    outputs.official = ''; outputs.pele = '';
    updateDownload(); updateReference();
    $('status').textContent = '原始碼已修改，按「重新繪製」更新';
  });
  $('source').addEventListener('keydown', (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') { event.preventDefault(); render(); }
  });
  for (const id of ['palette', 'layout']) $(id).addEventListener('change', render);
  $('theme').addEventListener('click', () => {
    theme = theme === 'light' ? 'dark' : 'light';
    document.documentElement.dataset.theme = theme;
    $('theme').textContent = theme === 'dark' ? '切換淺色' : '切換深色'; render();
  });
  $('zoom').addEventListener('change', applyZoom);
  $('export-engine').addEventListener('change', updateDownload);
  $('icon-search').addEventListener('input', updateCatalog);
  $('download').addEventListener('click', () => {
    const engine = $('export-engine').value, svg = outputs[engine];
    if (!svg) return;
    const url = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml;charset=utf-8' }));
    const anchor = document.createElement('a');
    anchor.href = url; anchor.download = `${engine}-${$('sample').value}-${theme}.svg`; anchor.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  });
  await loadSample();
} catch (error) {
  $('status').textContent = '無法載入比較工具';
  for (const engine of ['official', 'pele']) showError(engine, error);
}
