//! Importers for foreign theme formats. Each function takes a source path
//! and returns rendered rmdv TOML, ready to drop into the themes directory.
//!
//! - Base16: 24-color YAML (`base00..base0F`, optional metadata).
//! - VS Code: JSON with `colors` and `tokenColors` (TextMate scopes).

use crate::theme::{palette_for, Palette, ThemePreset};
use crate::theme_load::{parse_color, write_theme_toml};
use iced::Color;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug)]
pub struct Imported {
    pub slug: String,
    pub name: String,
    pub toml: String,
}

pub fn import_auto(path: &Path) -> Result<Imported, String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "yaml" | "yml" => import_base16(path),
        "json" => import_vscode(path),
        other => Err(format!("unsupported extension: {other}")),
    }
}

// ---------- Base16 ----------

#[derive(Debug, Deserialize)]
struct Base16Yaml {
    #[serde(alias = "scheme", alias = "name")]
    name: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(flatten)]
    rest: std::collections::BTreeMap<String, serde_yaml::Value>,
}

pub fn import_base16(path: &Path) -> Result<Imported, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let fallback = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Base16 Theme");
    import_base16_str(&raw, fallback)
}

pub fn import_base16_str(raw: &str, fallback_name: &str) -> Result<Imported, String> {
    let parsed: Base16Yaml = serde_yaml::from_str(raw).map_err(|e| e.to_string())?;

    // Resolve base00..base0F (accept either flat or `palette:` nested form).
    let mut bases: [Option<Color>; 16] = Default::default();
    let probe = |k: &str| -> Option<Color> {
        let v = parsed.rest.get(k).or_else(|| {
            parsed.rest.get("palette").and_then(|p| match p {
                serde_yaml::Value::Mapping(m) => m.get(serde_yaml::Value::String(k.to_string())),
                _ => None,
            })
        })?;
        let s = v.as_str()?;
        let hex = if s.starts_with('#') {
            s.to_string()
        } else {
            format!("#{s}")
        };
        parse_color(&hex).ok()
    };
    for i in 0..16 {
        let k = format!("base{:02X}", i);
        bases[i] = probe(&k).or_else(|| probe(&k.to_ascii_lowercase()));
    }
    for (i, b) in bases.iter().enumerate() {
        if b.is_none() {
            return Err(format!("missing base{:02X}", i));
        }
    }
    let b = bases.map(|c| c.unwrap());

    // Base16 spec: base00=bg, base01=lighter bg, base02=selection, base03=comment,
    // base04=dark fg, base05=fg, base06=light fg, base07=lightest,
    // base08=red, base09=orange, base0A=yellow, base0B=green,
    // base0C=cyan, base0D=blue, base0E=magenta, base0F=brown
    let dark = luma(b[0]) < 0.5;
    let pal = Palette {
        bg: b[0],
        surface: b[1],
        surface_alt: b[2],
        sidebar: b[0],
        fg: b[5],
        muted: b[4],
        subtle: b[3],
        accent: b[0xD],
        accent_fg: b[0],
        code_bg: b[1],
        code_border: with_alpha(b[3], 0.4),
        rule: with_alpha(b[3], 0.4),
        selection: with_alpha(b[0xD], 0.28),
        match_bg: with_alpha(b[0xA], 0.45),
        match_current_bg: with_alpha(b[9], 0.85),
        scroller: with_alpha(b[3], 0.0),
        scroller_hover: with_alpha(b[3], 0.55),
        indent_guide: with_alpha(b[2], 0.7),
        tree_selected_bg: with_alpha(b[0xD], 0.14),
        tree_selected_border: b[0xD],
        syntax: crate::theme::SyntaxPalette {
            keyword: b[0xE],
            type_: b[0xA],
            function: b[0xD],
            string: b[0xB],
            number: b[9],
            comment: b[3],
            operator: b[0xC],
            constant: b[9],
            variable: b[8],
            punctuation: b[5],
        },
    };

    let name = parsed
        .name
        .clone()
        .unwrap_or_else(|| fallback_name.to_string());
    let slug = crate::theme_load::slugify(&name);
    let mut toml = String::new();
    toml.push_str(&format!("# Imported from Base16 scheme: {}\n", name));
    if let Some(a) = parsed.author {
        toml.push_str(&format!("# Author: {}\n", a));
    }
    toml.push_str(&write_theme_toml(&name, dark, None, &pal, None));
    Ok(Imported { slug, name, toml })
}

// ---------- VS Code ----------

#[derive(Debug, Deserialize)]
struct VsCodeTheme {
    name: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    colors: std::collections::BTreeMap<String, String>,
    #[serde(default, rename = "tokenColors")]
    token_colors: Vec<TokenColor>,
}

#[derive(Debug, Deserialize)]
struct TokenColor {
    #[serde(default)]
    scope: TokenScope,
    #[serde(default)]
    settings: TokenSettings,
}

#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
enum TokenScope {
    #[default]
    None,
    One(String),
    Many(Vec<String>),
}

impl TokenScope {
    fn iter(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        match self {
            TokenScope::None => Box::new(std::iter::empty()),
            TokenScope::One(s) => Box::new(s.split(',').map(|x| x.trim())),
            TokenScope::Many(v) => Box::new(v.iter().map(|s| s.as_str())),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct TokenSettings {
    foreground: Option<String>,
}

pub fn import_vscode(path: &Path) -> Result<Imported, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    // Strip // and /* */ comments (VS Code allows JSONC).
    let stripped = strip_jsonc(&raw);
    let theme: VsCodeTheme = serde_json::from_str(&stripped).map_err(|e| e.to_string())?;

    let dark = matches!(theme.kind.as_deref(), Some("dark")) || theme.kind.is_none();
    let base_preset = if dark {
        ThemePreset::OneDark
    } else {
        ThemePreset::OneLight
    };
    let mut pal = palette_for(base_preset);

    // UI colors
    let c = |k: &str| -> Option<Color> {
        theme.colors.get(k).and_then(|s| {
            let s = s.trim();
            let s = if s.starts_with('#') {
                s.to_string()
            } else {
                format!("#{s}")
            };
            parse_color(&s).ok()
        })
    };
    if let Some(v) = c("editor.background") {
        pal.bg = v;
        pal.sidebar = v;
        pal.code_bg = v;
    }
    if let Some(v) = c("editor.foreground") {
        pal.fg = v;
    }
    if let Some(v) = c("editorWidget.background").or_else(|| c("sideBar.background")) {
        pal.surface = v;
    }
    if let Some(v) = c("list.activeSelectionBackground") {
        pal.surface_alt = v;
    }
    if let Some(v) = c("sideBar.background") {
        pal.sidebar = v;
    }
    if let Some(v) = c("descriptionForeground").or_else(|| c("editorLineNumber.foreground")) {
        pal.muted = v;
    }
    if let Some(v) = c("editorLineNumber.foreground") {
        pal.subtle = v;
    }
    if let Some(v) = c("focusBorder").or_else(|| c("button.background")) {
        pal.accent = v;
    }
    if let Some(v) = c("editor.lineHighlightBackground") {
        pal.surface_alt = v;
    }
    if let Some(v) = c("editor.selectionBackground") {
        pal.selection = v;
    }
    if let Some(v) = c("editor.findMatchHighlightBackground") {
        pal.match_bg = v;
    }
    if let Some(v) = c("editor.findMatchBackground") {
        pal.match_current_bg = v;
    }
    if let Some(v) =
        c("editorIndentGuide.background1").or_else(|| c("editorIndentGuide.background"))
    {
        pal.indent_guide = v;
    }
    if let Some(v) = c("editorRuler.foreground") {
        pal.rule = v;
    }

    // Token scopes -> syntax
    let scope_lookup = |needles: &[&str]| -> Option<Color> {
        for tc in &theme.token_colors {
            for sc in tc.scope.iter() {
                if needles
                    .iter()
                    .any(|n| sc == *n || sc.starts_with(&format!("{n}.")))
                {
                    if let Some(fg) = &tc.settings.foreground {
                        let s = if fg.starts_with('#') {
                            fg.to_string()
                        } else {
                            format!("#{fg}")
                        };
                        if let Ok(c) = parse_color(&s) {
                            return Some(c);
                        }
                    }
                }
            }
        }
        None
    };
    if let Some(v) = scope_lookup(&["keyword", "storage", "keyword.control"]) {
        pal.syntax.keyword = v;
    }
    if let Some(v) = scope_lookup(&["entity.name.type", "support.type", "storage.type"]) {
        pal.syntax.type_ = v;
    }
    if let Some(v) = scope_lookup(&["entity.name.function", "support.function", "meta.function"]) {
        pal.syntax.function = v;
    }
    if let Some(v) = scope_lookup(&["string"]) {
        pal.syntax.string = v;
    }
    if let Some(v) = scope_lookup(&["constant.numeric"]) {
        pal.syntax.number = v;
    }
    if let Some(v) = scope_lookup(&["comment"]) {
        pal.syntax.comment = v;
    }
    if let Some(v) = scope_lookup(&["keyword.operator", "punctuation.operator"]) {
        pal.syntax.operator = v;
    }
    if let Some(v) = scope_lookup(&["constant", "constant.language"]) {
        pal.syntax.constant = v;
    }
    if let Some(v) = scope_lookup(&["variable", "variable.other"]) {
        pal.syntax.variable = v;
    }
    if let Some(v) = scope_lookup(&["punctuation"]) {
        pal.syntax.punctuation = v;
    }

    let name = theme.name.clone().unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("VS Code Theme")
            .to_string()
    });
    let slug = crate::theme_load::slugify(&name);
    let mut toml = String::new();
    toml.push_str(&format!("# Imported from VS Code theme: {}\n", name));
    toml.push_str(&write_theme_toml(&name, dark, None, &pal, None));
    Ok(Imported { slug, name, toml })
}

fn strip_jsonc(s: &str) -> String {
    // Minimal JSONC stripper: line comments + block comments. Naive about
    // comments inside strings, but VS Code themes don't put `//` in values.
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut in_str = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            out.push(b as char);
            if b == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if b == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            in_str = true;
            out.push('"');
            i += 1;
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'/' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
                continue;
            }
        }
        out.push(b as char);
        i += 1;
    }
    out
}

fn luma(c: Color) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::from_rgba(c.r, c.g, c.b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base16_minimal_yaml() {
        let yaml = (0..16)
            .map(|i| {
                format!(
                    "base{:02X}: \"{:02x}{:02x}{:02x}\"",
                    i,
                    i * 16,
                    i * 16,
                    i * 16
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let yaml = format!("scheme: Test\n{yaml}\n");
        let tmp = std::env::temp_dir().join(format!("rmdv-b16-{}.yaml", std::process::id()));
        std::fs::write(&tmp, yaml).unwrap();
        let imp = import_base16(&tmp).unwrap();
        assert_eq!(imp.name, "Test");
        assert_eq!(imp.slug, "test");
        assert!(imp.toml.contains("[ui]"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn jsonc_strip_preserves_strings() {
        let s = strip_jsonc("{ \"a\": \"//keep\" /* drop */, // gone\n \"b\": 1 }");
        assert!(s.contains("\"a\": \"//keep\""));
        assert!(!s.contains("/*"));
        assert!(!s.contains("// gone"));
    }
}
