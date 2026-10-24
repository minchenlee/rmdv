//! Export fixed native references for the standalone Mermaid.js comparison.
//! cargo run --locked --example renderer_compare -- prototypes/mermaid-js/reference

use rmdv::ast::{Block, DiagramKind, Inline};
use rmdv::diagram::render_blocking_async;
use rmdv::theme::Palette;
use serde_json::{json, Map, Value};

fn palette_json(palette: Palette) -> Value {
    let colors = [
        ("bg", palette.bg),
        ("surface", palette.surface),
        ("surfaceAlt", palette.surface_alt),
        ("fg", palette.fg),
        ("muted", palette.muted),
        ("border", palette.code_border),
        ("accent", palette.accent),
    ];
    Value::Object(
        colors
            .into_iter()
            .map(|(name, c)| (name.to_owned(), json!([c.r, c.g, c.b, c.a])))
            .collect(),
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args()
        .nth(1)
        .ok_or("usage: renderer_compare <output-directory>")?;
    let directory = std::path::Path::new(&directory);
    std::fs::create_dir_all(directory)?;
    let (blocks, _) = rmdv::parser::parse(include_str!("../tests/fixtures/renderer_trial.md"));
    let mut title = String::new();
    let mut samples = Vec::new();
    for (_, block) in blocks {
        match block {
            Block::Heading { inlines, .. } => {
                title = inlines
                    .iter()
                    .filter_map(|inline| match inline {
                        Inline::Text(text) | Inline::Code(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
            }
            Block::Diagram {
                kind: DiagramKind::Mermaid,
                source,
                ..
            } => {
                let id = samples.len();
                let mut native = Map::new();
                for (theme, palette) in [("light", Palette::ONE_LIGHT), ("dark", Palette::ONE_DARK)]
                {
                    let result = render_blocking_async(
                        DiagramKind::Mermaid,
                        source.clone(),
                        palette,
                        "sans-serif".to_owned(),
                    )
                    .await;
                    let entry = match result {
                        Ok(output) => {
                            let filename = format!("{id}-{theme}");
                            std::fs::write(directory.join(format!("{filename}.svg")), output.svg)?;
                            image::save_buffer(
                                directory.join(format!("{filename}.png")),
                                &output.rgba,
                                output.w,
                                output.h,
                                image::ColorType::Rgba8,
                            )?;
                            json!({"file": format!("/reference/{filename}.png"),
                                "width": output.w, "height": output.h})
                        }
                        Err(error) => json!({"error": error}),
                    };
                    native.insert(theme.to_owned(), entry);
                }
                samples.push(json!({"id": id, "title": title, "source": source, "native": native}));
            }
            _ => {}
        }
    }
    let manifest = json!({
        "nativeRenderer": "official Mermaid.js · 12.2.0 (Chromium worker)",
        "fixture": "tests/fixtures/renderer_trial.md",
        "palettes": {"light": palette_json(Palette::ONE_LIGHT), "dark": palette_json(Palette::ONE_DARK)},
        "samples": samples,
    });
    std::fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    println!(
        "Exported {} same-source native references to {}",
        samples.len(),
        directory.display()
    );
    Ok(())
}
