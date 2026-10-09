//! Fonts bundled into the binary, registered with Iced at startup.
//!
//! cosmic-text only accepts a face whose weight equals the requested weight
//! (monospace faces excepted), and fontdb reads a variable font as a single
//! face at its default weight. So `Inter-Variable.ttf` serves weight 400 only;
//! the static instances below (cut from it at wght 500/600/700, subset to
//! Latin, Greek, Cyrillic, punctuation, arrows and math) serve the others.
//! Without them a bold or medium "Inter" run falls through to JetBrains Mono.
//! Inter is licensed under the SIL OFL 1.1; see `assets/fonts/Inter-OFL.txt`.

pub const BUNDLED: [&[u8]; 6] = [
    include_bytes!("assets/fonts/Inter-Variable.ttf"),
    include_bytes!("assets/fonts/Inter-Medium.ttf"),
    include_bytes!("assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("assets/fonts/Inter-Bold.ttf"),
    include_bytes!("assets/fonts/JetBrainsMono-Regular.otf"),
    include_bytes!("assets/fonts/lucide.ttf"),
];

#[cfg(test)]
mod tests {
    use super::BUNDLED;
    use iced::advanced::graphics::text::cosmic_text::{
        fontdb, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight,
    };
    use std::sync::Arc;

    /// Shape `text` the way Iced does (family "Inter" at `weight`, only the
    /// bundled fonts loaded) and return the family of each glyph's face.
    fn shaped_families(text: &str, weight: Weight) -> Vec<String> {
        let sources = BUNDLED
            .iter()
            .map(|bytes| fontdb::Source::Binary(Arc::new(*bytes)));
        let mut fonts = FontSystem::new_with_fonts(sources);
        let mut buffer = Buffer::new(&mut fonts, Metrics::new(16.0, 20.0));
        buffer.set_text(
            &mut fonts,
            text,
            &Attrs::new().family(Family::Name("Inter")).weight(weight),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut fonts, false);
        let mut out = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let face = fonts.db().face(glyph.font_id).expect("face");
                out.push(face.families[0].0.clone());
            }
        }
        out
    }

    #[test]
    fn inter_weights_resolve_to_inter_not_the_monospace_fallback() {
        for weight in [
            Weight::NORMAL,
            Weight::MEDIUM,
            Weight::SEMIBOLD,
            Weight::BOLD,
        ] {
            let families = shaped_families("Plain bold words", weight);
            assert!(!families.is_empty());
            assert!(
                families.iter().all(|f| f == "Inter"),
                "weight {weight:?} shaped with {families:?}"
            );
        }
    }
}
