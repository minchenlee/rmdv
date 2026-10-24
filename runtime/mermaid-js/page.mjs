import mermaid from '/node_modules/mermaid/dist/mermaid.esm.min.mjs';
import { loadAwsIcons } from '/icons.mjs';
const aws = await loadAwsIcons();
const logosResponse = await fetch('/node_modules/@iconify-json/logos/icons.json');
if (!logosResponse.ok) throw Error('Logos 圖示包尚未安裝，請在 runtime/mermaid-js 執行 npm ci。');
const logos = await logosResponse.json();
mermaid.registerIconPacks([{ name: 'aws', icons: aws.pack }, { name: 'logos', icons: logos }]);
let serial = 0;
window.renderDiagram = async ({ source, raster, config = {} }) => {
  if (new TextEncoder().encode(source).length > 68 * 1024) throw Error('diagram too large');
  aws.validate(source);
  for (const match of source.matchAll(/\blogos:([a-z0-9-]+)/g)) {
    if (!Object.hasOwn(logos.icons, match[1]) && !Object.hasOwn(logos.aliases || {}, match[1])) {
      throw Error(`找不到 Logos 圖示：${match[0]}。`);
    }
  }
  // Configure defaults from the current app palette on every request. Mermaid
  // still applies explicit source init/frontmatter/style overrides afterwards.
  const variables = config.themeVariables || {};
  // Iced's bundled fonts are not automatically registered in the WebKit helper.
  // Use the same explicit fallback stack during measurement and final painting.
  const fontFamily = `${config.fontFamily || 'Helvetica'}, Helvetica, "PingFang TC", sans-serif`;
  document.body.style.fontFamily = fontFamily;
  await document.fonts.load(`16px ${fontFamily}`);
  await document.fonts.ready;
  document.documentElement.style.backgroundColor = 'transparent';
  document.body.style.backgroundColor = 'transparent';
  document.documentElement.style.color = variables.textColor || '';
  document.documentElement.style.colorScheme = variables.darkMode ? 'dark' : 'light';
  const options = { ...config, startOnLoad: false, securityLevel: 'strict', suppressErrorRendering: true,
    maxTextSize: 68 * 1024, maxEdges: 500, layout: config.layout || 'dagre', theme: config.theme || 'base',
    fontFamily, themeVariables: { ...variables, fontFamily },
    flowchart: {
      nodeSpacing: 60, rankSpacing: 70,
      subGraphTitleMargin: { top: 12, bottom: 24 },
      ...config.flowchart,
    } };
  mermaid.initialize(options);
  const diagramType = mermaid.detectType(source);
  const isFlowchart = diagramType === 'flowchart' || diagramType === 'flowchart-v2';
  if (isFlowchart && !config.layout) mermaid.initialize({ ...options, layout: 'elk' });
  const host = document.getElementById('diagram');
  host.replaceChildren();
  await mermaid.parse(source);
  let { svg } = await mermaid.render(`rmdv-${++serial}`, source);
  if (new TextEncoder().encode(svg).length > 4 * 1024 * 1024) throw Error('rendered diagram too large');
  host.innerHTML = svg;
  await document.fonts.ready;
  const element = host.firstElementChild;
  // The app/modal supplies the backdrop. Keep only diagram elements opaque,
  // and retain the transparent root in exported SVG as well as the PNG.
  element.style.setProperty('background-color', 'transparent', 'important');
  if (isFlowchart) protectClusterTitles(element);
  svg = element.outerHTML;
  const box = element.viewBox.baseVal;
  if (!(box.width > 0 && box.height > 0)) throw Error('svg has zero size');
  const scale = Math.min(2, 1600 / box.width, 8192 / box.height, Math.sqrt(8_000_000 / (box.width * box.height)));
  if (raster) {
    element.style.maxWidth = 'none';
    element.style.width = `${Math.max(1, Math.floor(box.width * scale))}px`;
    element.style.height = `${Math.max(1, Math.floor(box.height * scale))}px`;
  }
  return { svg };
};


// Mermaid paints edge paths after cluster groups. Repaint each title over a
// small backing of its own cluster color, so incoming lines cannot cross text.
// Keep the original group/label for SVG semantics and hide the decorative copy
// from accessibility and pointer hit-testing.
function protectClusterTitles(svg) {
  const namespace = 'http://www.w3.org/2000/svg';
  const rootMatrix = svg.getCTM();
  if (!rootMatrix) return;
  const foreground = document.createElementNS(namespace, 'g');
  foreground.setAttribute('aria-hidden', 'true');
  foreground.style.pointerEvents = 'none';
  for (const cluster of svg.querySelectorAll('g.cluster')) {
    const label = cluster.querySelector('.cluster-label');
    const rect = cluster.querySelector('rect');
    const matrix = label?.getCTM();
    if (!label || !rect || !matrix) continue;
    const bounds = label.getBBox();
    if (!(bounds.width > 0 && bounds.height > 0)) continue;
    const group = document.createElementNS(namespace, 'g');
    const transform = rootMatrix.inverse().multiply(matrix);
    group.setAttribute('transform', `matrix(${transform.a} ${transform.b} ${transform.c} ${transform.d} ${transform.e} ${transform.f})`);
    const style = getComputedStyle(label);
    group.style.color = style.color;
    group.style.fontFamily = style.fontFamily;
    group.style.fontSize = style.fontSize;
    const backing = document.createElementNS(namespace, 'rect');
    for (const [name, value] of Object.entries({ x: bounds.x - 3, y: bounds.y - 1,
      width: bounds.width + 6, height: bounds.height + 2 })) backing.setAttribute(name, String(value));
    backing.setAttribute('fill', getComputedStyle(rect).fill);
    const copy = label.cloneNode(true);
    copy.removeAttribute('transform');
    copy.removeAttribute('id');
    for (const node of copy.querySelectorAll('[id]')) node.removeAttribute('id');
    group.append(backing, copy);
    foreground.append(group);
  }
  svg.append(foreground);
}
