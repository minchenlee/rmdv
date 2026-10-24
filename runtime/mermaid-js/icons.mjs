export async function loadAwsIcons() {
  const response = await fetch('/icon-cache/aws-icons.json');
  if (!response.ok) throw new Error('AWS 圖示尚未準備完成，請先執行 npm run prepare:icons。');
  return createAwsIcons(await response.json());
}

export function createAwsIcons(pack) {
  const aliases = {};
  for (const [name, icon] of Object.entries(pack.icons)) {
    // Preserve canonical AWS Labs names and provide the Arch_* names used in
    // the supplied Mermaid Chart documentation when the source identifies them.
    const architectureName = icon.body.match(/\bArch_([A-Za-z0-9_-]+?)_(?:16|32|48|64)\b/)?.[1];
    if (architectureName) aliases[`arch-${architectureName.toLowerCase().replaceAll('_', '-')}`] = { parent: name };
  }
  pack.aliases = { ...pack.aliases, ...aliases };
  const names = [...Object.keys(pack.icons), ...Object.keys(pack.aliases)].sort();
  function getIcon(name) {
    if (!name.startsWith('aws:')) return undefined;
    const key = name.slice(4);
    return pack.icons[key] || pack.icons[pack.aliases[key]?.parent];
  }
  return {
    pack, names,
    validate(source) {
      for (const match of source.matchAll(/\baws:([a-z0-9-]+)/g)) {
        if (!getIcon(match[0])) throw new Error(`找不到 AWS 圖示：${match[0]}。請使用下方「AWS 圖示名稱」提供的名稱。`);
      }
    },
    resolve(name) {
      const icon = getIcon(name);
      if (!icon) return undefined;
      // Markup comes only from the checksum-pinned AWS Labs pack, never from
      // diagram text. Keep the original multicolor SVG artwork unmodified.
      return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${icon.width || pack.width} ${icon.height || pack.height}">${icon.body}</svg>`;
    },
  };
}
