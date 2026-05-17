//! Custom theme files. Schema lives at `~/.config/mdv/themes/*.toml`
//! (XDG-respecting via `dirs::config_dir()`).
//!
//! ```toml
//! name = "My Theme"
//! dark = true
//! extends = "one-dark"          # optional, inherit from a built-in preset
//!
//! [ui]
//! bg = "#282c34"
//! accent = "#e5a06b"
//! # ... any Palette field, all optional when `extends` is set
//!
//! [syntax]
//! keyword = "#c678dd"
//! # ... any SyntaxPalette field
//!
//! [typography]                  # optional
//! body_size = 16.0
//! ```

use crate::theme::{palette_for, preset_by_slug, Palette, SyntaxPalette, ThemePreset, Typography};
use iced::Color;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CustomTheme {
    pub slug: String,
    pub name: String,
    pub dark: bool,
    pub palette: Palette,
    pub typography: Typography,
    pub path: PathBuf,
    pub bundled: bool,
}

#[derive(Debug, Deserialize)]
struct ThemeFile {
    name: Option<String>,
    dark: Option<bool>,
    extends: Option<String>,
    #[serde(default)]
    ui: UiSection,
    #[serde(default)]
    syntax: SyntaxSection,
    #[serde(default)]
    typography: TypographySection,
}

#[derive(Debug, Default, Deserialize)]
struct UiSection {
    bg: Option<String>,
    surface: Option<String>,
    surface_alt: Option<String>,
    sidebar: Option<String>,
    fg: Option<String>,
    muted: Option<String>,
    subtle: Option<String>,
    accent: Option<String>,
    accent_fg: Option<String>,
    code_bg: Option<String>,
    code_border: Option<String>,
    rule: Option<String>,
    selection: Option<String>,
    match_bg: Option<String>,
    match_current_bg: Option<String>,
    scroller: Option<String>,
    scroller_hover: Option<String>,
    indent_guide: Option<String>,
    tree_selected_bg: Option<String>,
    tree_selected_border: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SyntaxSection {
    keyword: Option<String>,
    #[serde(alias = "type_")]
    type_: Option<String>,
    function: Option<String>,
    string: Option<String>,
    number: Option<String>,
    comment: Option<String>,
    operator: Option<String>,
    constant: Option<String>,
    variable: Option<String>,
    punctuation: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct TypographySection {
    body_size: Option<f32>,
    line_height: Option<f32>,
    measure_ch: Option<u32>,
    h1_size: Option<f32>,
    h2_size: Option<f32>,
    h3_size: Option<f32>,
    h4_size: Option<f32>,
    h5_size: Option<f32>,
    h6_size: Option<f32>,
    code_size: Option<f32>,
}

/// Returns the canonical themes directory. Created lazily on first write — read
/// paths just probe existence.
pub fn themes_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mdv").join("themes"))
}

pub fn ensure_themes_dir() -> std::io::Result<PathBuf> {
    let dir = themes_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config dir"))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Scan the themes directory and return every successfully parsed theme.
/// Parse errors are reported via `errors` so callers can surface them in UI.
pub fn discover(errors: &mut Vec<String>) -> Vec<CustomTheme> {
    let Some(dir) = themes_dir() else {
        return Vec::new();
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        match load_file(&p) {
            Ok(t) => out.push(t),
            Err(e) => errors.push(format!("{}: {e}", p.display())),
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

pub fn load_file(path: &Path) -> Result<CustomTheme, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file: ThemeFile = toml::from_str(&raw).map_err(|e| e.to_string())?;
    let slug = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| slugify(s))
        .unwrap_or_else(|| "custom".to_string());
    let name = file.name.clone().unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Custom")
            .to_string()
    });

    let (base_palette, base_dark) = match file.extends.as_deref() {
        Some(slug) => {
            let preset =
                preset_by_slug(slug).ok_or_else(|| format!("unknown `extends` preset: {slug}"))?;
            (palette_for(preset), preset.is_dark())
        }
        None => (
            // Default to dark-style fg/bg sentinels; user is expected to
            // supply most fields when not extending.
            palette_for(ThemePreset::OneDark),
            true,
        ),
    };
    let dark = file.dark.unwrap_or(base_dark);
    let palette = apply_overrides(base_palette, &file.ui, &file.syntax)?;
    let typography = apply_typography(Typography::DEFAULT, &file.typography);

    Ok(CustomTheme {
        slug,
        name,
        dark,
        palette,
        typography,
        path: path.to_path_buf(),
        bundled: false,
    })
}

include!(concat!(env!("OUT_DIR"), "/bundled_themes.rs"));

/// Returns every theme baked into the binary. Cached for the process lifetime
/// since the contents are static.
pub fn bundled() -> &'static Vec<CustomTheme> {
    use std::sync::OnceLock;
    static CELL: OnceLock<Vec<CustomTheme>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut out = Vec::with_capacity(BUNDLED_BASE16.len());
        for (stem, body) in BUNDLED_BASE16 {
            match crate::theme_import::import_base16_str(body, stem) {
                Ok(imp) => match toml_to_custom(&imp.toml, stem) {
                    Ok(mut t) => {
                        t.bundled = true;
                        out.push(t);
                    }
                    Err(_) => {}
                },
                Err(_) => {}
            }
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out
    })
}

fn toml_to_custom(text: &str, stem: &str) -> Result<CustomTheme, String> {
    let file: ThemeFile = toml::from_str(text).map_err(|e| e.to_string())?;
    let name = file.name.clone().unwrap_or_else(|| stem.to_string());
    let slug = slugify(stem);
    let (base_palette, base_dark) = match file.extends.as_deref() {
        Some(slug) => {
            let preset =
                preset_by_slug(slug).ok_or_else(|| format!("unknown `extends` preset: {slug}"))?;
            (palette_for(preset), preset.is_dark())
        }
        None => (palette_for(ThemePreset::OneDark), true),
    };
    let dark = file.dark.unwrap_or(base_dark);
    let palette = apply_overrides(base_palette, &file.ui, &file.syntax)?;
    let typography = apply_typography(Typography::DEFAULT, &file.typography);
    Ok(CustomTheme {
        slug,
        name,
        dark,
        palette,
        typography,
        path: PathBuf::from(format!("<bundled:{stem}>")),
        bundled: true,
    })
}

fn apply_overrides(
    mut pal: Palette,
    ui: &UiSection,
    sx: &SyntaxSection,
) -> Result<Palette, String> {
    macro_rules! set_ui {
        ($field:ident) => {
            if let Some(s) = &ui.$field {
                pal.$field = parse_color(s)?;
            }
        };
    }
    set_ui!(bg);
    set_ui!(surface);
    set_ui!(surface_alt);
    set_ui!(sidebar);
    set_ui!(fg);
    set_ui!(muted);
    set_ui!(subtle);
    set_ui!(accent);
    set_ui!(accent_fg);
    set_ui!(code_bg);
    set_ui!(code_border);
    set_ui!(rule);
    set_ui!(selection);
    set_ui!(match_bg);
    set_ui!(match_current_bg);
    set_ui!(scroller);
    set_ui!(scroller_hover);
    set_ui!(indent_guide);
    set_ui!(tree_selected_bg);
    set_ui!(tree_selected_border);

    macro_rules! set_sx {
        ($field:ident) => {
            if let Some(s) = &sx.$field {
                pal.syntax.$field = parse_color(s)?;
            }
        };
    }
    set_sx!(keyword);
    set_sx!(type_);
    set_sx!(function);
    set_sx!(string);
    set_sx!(number);
    set_sx!(comment);
    set_sx!(operator);
    set_sx!(constant);
    set_sx!(variable);
    set_sx!(punctuation);
    Ok(pal)
}

fn apply_typography(mut t: Typography, src: &TypographySection) -> Typography {
    if let Some(v) = src.body_size {
        t.body_size = v;
    }
    if let Some(v) = src.line_height {
        t.line_height = v;
    }
    if let Some(v) = src.measure_ch {
        t.measure_ch = v;
    }
    if let Some(v) = src.h1_size {
        t.h1_size = v;
    }
    if let Some(v) = src.h2_size {
        t.h2_size = v;
    }
    if let Some(v) = src.h3_size {
        t.h3_size = v;
    }
    if let Some(v) = src.h4_size {
        t.h4_size = v;
    }
    if let Some(v) = src.h5_size {
        t.h5_size = v;
    }
    if let Some(v) = src.h6_size {
        t.h6_size = v;
    }
    if let Some(v) = src.code_size {
        t.code_size = v;
    }
    t
}

/// Accepts `#rgb`, `#rrggbb`, `#rrggbbaa`.
pub fn parse_color(s: &str) -> Result<Color, String> {
    let s = s.trim();
    let hex = s
        .strip_prefix('#')
        .ok_or_else(|| format!("color must start with #: {s}"))?;
    let (r, g, b, a) = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16);
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16);
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16);
            (r, g, b, Ok(255u8))
        }
        6 => (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
            Ok(255u8),
        ),
        8 => (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
            u8::from_str_radix(&hex[6..8], 16),
        ),
        _ => return Err(format!("expected #rgb, #rrggbb, or #rrggbbaa: {s}")),
    };
    let (r, g, b, a) = (
        r.map_err(|e| e.to_string())?,
        g.map_err(|e| e.to_string())?,
        b.map_err(|e| e.to_string())?,
        a.map_err(|e| e.to_string())?,
    );
    Ok(Color::from_rgba(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ))
}

pub fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "theme".to_string()
    } else {
        out
    }
}

/// Render a `Palette` + `Typography` to TOML, optionally extending a preset.
/// Used by importers so they don't drift from the runtime schema.
pub fn write_theme_toml(
    name: &str,
    dark: bool,
    extends: Option<&str>,
    pal: &Palette,
    typo: Option<&Typography>,
) -> String {
    let mut s = String::new();
    s.push_str(&format!("name = \"{}\"\n", name.replace('"', "\\\"")));
    s.push_str(&format!("dark = {}\n", dark));
    if let Some(e) = extends {
        s.push_str(&format!("extends = \"{}\"\n", e));
    }
    s.push('\n');
    s.push_str("[ui]\n");
    push_color(&mut s, "bg", pal.bg);
    push_color(&mut s, "surface", pal.surface);
    push_color(&mut s, "surface_alt", pal.surface_alt);
    push_color(&mut s, "sidebar", pal.sidebar);
    push_color(&mut s, "fg", pal.fg);
    push_color(&mut s, "muted", pal.muted);
    push_color(&mut s, "subtle", pal.subtle);
    push_color(&mut s, "accent", pal.accent);
    push_color(&mut s, "accent_fg", pal.accent_fg);
    push_color(&mut s, "code_bg", pal.code_bg);
    push_color(&mut s, "code_border", pal.code_border);
    push_color(&mut s, "rule", pal.rule);
    push_color(&mut s, "selection", pal.selection);
    push_color(&mut s, "match_bg", pal.match_bg);
    push_color(&mut s, "match_current_bg", pal.match_current_bg);
    push_color(&mut s, "scroller", pal.scroller);
    push_color(&mut s, "scroller_hover", pal.scroller_hover);
    push_color(&mut s, "indent_guide", pal.indent_guide);
    push_color(&mut s, "tree_selected_bg", pal.tree_selected_bg);
    push_color(&mut s, "tree_selected_border", pal.tree_selected_border);

    s.push('\n');
    s.push_str("[syntax]\n");
    let sx = &pal.syntax;
    push_color(&mut s, "keyword", sx.keyword);
    push_color(&mut s, "type_", sx.type_);
    push_color(&mut s, "function", sx.function);
    push_color(&mut s, "string", sx.string);
    push_color(&mut s, "number", sx.number);
    push_color(&mut s, "comment", sx.comment);
    push_color(&mut s, "operator", sx.operator);
    push_color(&mut s, "constant", sx.constant);
    push_color(&mut s, "variable", sx.variable);
    push_color(&mut s, "punctuation", sx.punctuation);

    if let Some(t) = typo {
        s.push('\n');
        s.push_str("[typography]\n");
        s.push_str(&format!("body_size = {}\n", t.body_size));
        s.push_str(&format!("line_height = {}\n", t.line_height));
        s.push_str(&format!("code_size = {}\n", t.code_size));
    }
    let _ = SyntaxPalette::ONE_DARK; // keep import live
    s
}

fn push_color(s: &mut String, key: &str, c: Color) {
    let r = (c.r * 255.0).round() as u8;
    let g = (c.g * 255.0).round() as u8;
    let b = (c.b * 255.0).round() as u8;
    let a = (c.a * 255.0).round() as u8;
    if a == 255 {
        s.push_str(&format!("{key} = \"#{r:02x}{g:02x}{b:02x}\"\n"));
    } else {
        s.push_str(&format!("{key} = \"#{r:02x}{g:02x}{b:02x}{a:02x}\"\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_forms() {
        assert!(parse_color("#fff").is_ok());
        assert!(parse_color("#ffffff").is_ok());
        assert!(parse_color("#ffffff80").is_ok());
        assert!(parse_color("ffffff").is_err());
        assert!(parse_color("#zzzzzz").is_err());
    }

    #[test]
    fn slug_basics() {
        assert_eq!(slugify("One Dark"), "one-dark");
        assert_eq!(slugify("Tokyo Night Storm!!!"), "tokyo-night-storm");
        assert_eq!(slugify("__weird__"), "weird");
    }

    #[test]
    fn round_trip_palette() {
        let pal = palette_for(ThemePreset::Dracula);
        let txt = write_theme_toml("Dracula Clone", true, Some("dracula"), &pal, None);
        let dir = std::env::temp_dir().join(format!("mdv-theme-test-{}.toml", std::process::id()));
        std::fs::write(&dir, &txt).unwrap();
        let loaded = load_file(&dir).unwrap();
        assert_eq!(loaded.name, "Dracula Clone");
        assert!(loaded.dark);
        let _ = std::fs::remove_file(&dir);
    }
}
