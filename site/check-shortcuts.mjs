import fs from 'node:fs';

const appSource = fs.readFileSync(new URL('../src/app.rs', import.meta.url), 'utf8');
const html = fs.readFileSync(new URL('index.html', import.meta.url), 'utf8');
const browserLayer = fs.readFileSync(new URL('app.js', import.meta.url), 'utf8');

const contracts = [
  {
    id: 'open-file',
    action: 'Find File in Workspace',
    html: ['<kbd data-mod>⌘</kbd>', '<kbd>P</kbd>'],
    native: [
      '"p" if cmd => return Message::OpenFileFinder',
      '("⌘P", "Find File in Workspace")',
    ],
  },
  {
    id: 'search-all',
    action: 'Search All Files',
    html: ['<kbd data-mod>⌘</kbd>', '<kbd>⇧</kbd>', '<kbd>F</kbd>'],
    native: [
      '"f" | "F" if cmd && mods.shift() => return Message::OpenVaultSearch',
      '("⌘⇧F", "Search All Files")',
    ],
  },
  {
    id: 'document-mindmap',
    action: 'Toggle Mindmap',
    html: ['<kbd data-mod>⌘</kbd>', '<kbd>M</kbd>'],
    native: [
      '"m" if cmd => return Message::ToggleMindmap',
      '("⌘M", "Toggle Mindmap")',
    ],
  },
  {
    id: 'full-mindmap',
    action: 'Toggle Full Mindmap Mode',
    html: ['<kbd data-mod>⌘</kbd>', '<kbd>⇧</kbd>', '<kbd>M</kbd>'],
    native: [
      'Physical::Code(Code::KeyM)',
      '("Toggle Full Mindmap Mode  ⌘⇧M", Message::ToggleFullMindmap)',
    ],
  },
  {
    id: 'toggle-zen',
    action: 'Toggle Zen Edit',
    html: ['<kbd data-mod>⌘</kbd>', '<kbd>E</kbd>'],
    native: [
      '"e" if cmd => return Message::ToggleViewMode',
      '("⌘E", "Toggle Zen Edit")',
    ],
  },
  {
    id: 'fold-headings',
    action: 'Fold to Level',
    html: ['<kbd data-mod>⌘</kbd>', '<kbd>K</kbd>', 'class="key-then">then</span>', '<kbd>0–6</kbd>'],
    native: [
      '"k" if cmd && !editing => return Message::FoldChordStart',
      'kbd("Fold to Level (then 0–6)", "⌘K")',
    ],
  },
  {
    id: 'scroll-document',
    action: 'Scroll Document',
    html: ['<kbd data-key="j">j</kbd>', '<kbd data-key="k">k</kbd>', '<kbd data-key="g">g</kbd>', '<kbd data-key="G">G</kbd>'],
    native: [
      '"j" => Some(Message::ScrollBy(40.0))',
      '"k" => Some(Message::ScrollBy(-40.0))',
      '"g" => Some(Message::ScrollToTop)',
      '"G" => Some(Message::ScrollToBottom)',
    ],
  },
];

const failures = [];

for (const contract of contracts) {
  const rowPattern = new RegExp(
    `<div class="command-row"[^>]*data-app-shortcut="${contract.id}"[^>]*>([\\s\\S]*?)</div>`,
  );
  const row = html.match(rowPattern)?.[1];
  if (!row) {
    failures.push(`${contract.id}: website row is missing`);
    continue;
  }
  if (!row.includes(`<span>${contract.action}</span>`)) {
    failures.push(`${contract.id}: website action must be "${contract.action}"`);
  }
  for (const fragment of contract.html) {
    if (!row.includes(fragment)) failures.push(`${contract.id}: website row is missing ${fragment}`);
  }
  for (const fragment of contract.native) {
    if (!appSource.includes(fragment)) failures.push(`${contract.id}: native binding is missing ${fragment}`);
  }
}

const websiteIds = [...html.matchAll(/class="command-row"[^>]*data-app-shortcut="([^"]+)"/g)]
  .map((match) => match[1]);
const contractIds = contracts.map(({ id }) => id);
for (const id of websiteIds) {
  if (!contractIds.includes(id)) failures.push(`${id}: website row has no native contract`);
}
for (const id of contractIds) {
  if (!websiteIds.includes(id)) failures.push(`${id}: native contract has no website row`);
}

if (!appSource.includes('let cmd = mods.command() || mods.control();')) {
  failures.push('native modifier handling no longer supports Command or Control');
}
if (!browserLayer.includes('const mod = mac ? e.metaKey : e.ctrlKey;')) {
  failures.push('website modifier handling no longer mirrors Command or Control');
}
if (!browserLayer.includes('if (e.defaultPrevented) return;')) {
  failures.push('website global shortcuts do not respect handled events from shadow controls');
}
if (browserLayer.includes('sticker-forge')) {
  failures.push('website still includes the interactive Sticker Forge control');
}
for (const fragment of ['appShortcutPreviews', 'openAppExperience', '.command-preview']) {
  if (browserLayer.includes(fragment)) failures.push(`website app-shortcut interaction returned: ${fragment}`);
}
for (const fragment of ['data-preview=', 'data-native-view=', 'id="app-demo"']) {
  if (html.includes(fragment)) failures.push(`website app-shortcut preview markup returned: ${fragment}`);
}
if (!html.includes('The seven app shortcuts above are shown for reference and are not captured by this website.')) {
  failures.push('static app-shortcut boundary is not clearly labeled');
}
if (!html.includes('Keyboard shortcuts for this website')) {
  failures.push('website-only shortcut sheet is not clearly labeled');
}
if (!html.includes('<kbd data-mod>⌘</kbd><kbd data-key="/">/</kbd> for website shortcuts')) {
  failures.push('website ⌘/ shortcut-sheet hint is missing');
}
if (!browserLayer.includes("const shortcutsKey = mod && !e.shiftKey && e.key === '/';")) {
  failures.push('website ⌘/ shortcut-sheet binding is missing');
}
if (browserLayer.includes("case '?':")) {
  failures.push('legacy ? shortcut-sheet binding returned');
}
if (!html.includes('<span>Run a page command</span><kbd data-key="p">p</kbd>')) {
  failures.push('website p command-palette hint is missing');
}
if (!browserLayer.includes("e.key === 'p' && !typing")) {
  failures.push('website p command-palette binding is missing');
}
if (browserLayer.includes('mod && e.shiftKey && e.key.toLowerCase()')) {
  failures.push('unreliable website ⌘⇧P command-palette binding returned');
}
for (const fragment of [
  '"p" if cmd && mods.shift() => return Message::OpenCommandPalette',
  '("⌘⇧P", "Command Palette")',
  '"/" if cmd => return Message::ToggleShortcuts',
  '("⌘/", "Show Shortcuts")',
]) {
  if (!appSource.includes(fragment)) failures.push(`native auxiliary binding is missing ${fragment}`);
}
for (const feature of [
  'Fuzzy file finder (Cmd+P)',
  'Mindmap view for any document including JSON and YAML (Cmd+M)',
  'Full Mindmap workspace navigation for folders and files (Cmd+Shift+M)',
  'Vault-wide search with Zed-style full-page results (Cmd+Shift+F)',
  'In-document search (Cmd+F)',
  'Edit mode (Cmd+E)',
  'Heading fold (Cmd+K 0-6)',
]) {
  if (!html.includes(feature)) failures.push(`structured feature shortcut is missing ${feature}`);
}

if (failures.length) {
  console.error('Shortcut contract: FAIL');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exitCode = 1;
} else {
  console.log(`Shortcut contract: PASS (${contracts.length} website rows match native bindings)`);
}
