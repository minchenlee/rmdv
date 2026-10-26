//! T2 — renderer-core integration tests.

use rmdv::diagram::{render_blocking, DiagramKind, MAX_SOURCE_BYTES};
use rmdv::theme::Palette;

fn palette() -> Palette {
    Palette::ONE_DARK
}

#[test]
fn render_simple_mermaid_returns_svg() {
    let svg = render_blocking(
        DiagramKind::Mermaid,
        "graph LR\n  A --> B",
        &palette(),
        "system-ui",
    )
    .expect("mermaid should render");
    let head = &svg[..200.min(svg.len())];
    assert!(head.contains("<svg"), "no <svg in head: {head}");
}

#[test]
fn render_simple_dot_returns_svg() {
    let svg = render_blocking(
        DiagramKind::Dot,
        "digraph G { a -> b }",
        &palette(),
        "system-ui",
    )
    .expect("dot should render");
    let head = &svg[..200.min(svg.len())];
    assert!(head.contains("<svg"), "no <svg in head: {head}");
}

#[test]
fn render_rejects_oversized_source() {
    let big = "x".repeat(MAX_SOURCE_BYTES + 100);
    let err = render_blocking(DiagramKind::Mermaid, &big, &palette(), "system-ui")
        .expect_err("oversized source must be rejected");
    assert!(err.contains("too large"), "got: {err}");
}

#[test]
fn render_mermaid_invalid_returns_err() {
    let err = render_blocking(
        DiagramKind::Mermaid,
        "this is not mermaid",
        &palette(),
        "system-ui",
    )
    .expect_err("invalid mermaid must be Err");
    assert!(!err.is_empty());
}

#[test]
fn inject_skipped_when_user_has_init() {
    // If the user already provided %%{init, our theme block must not be prepended.
    // We can't easily inspect the prepared string from outside, but we can verify
    // behaviorally: a user-provided init parses identically with or without our
    // injection. We rely on render success here as a proxy — and check no panic.
    let src = "%%{init: { 'theme': 'base' }}%%\ngraph LR\n  A --> B";
    let svg = render_blocking(DiagramKind::Mermaid, src, &palette(), "system-ui")
        .expect("user-init mermaid should render");
    assert!(svg.contains("<svg"));
}

#[tokio::test]
async fn mermaid_families_render_through_the_app_raster_pipeline() {
    use rmdv::diagram::render_blocking_async;

    let cases = [
        ("flowchart", "flowchart LR\n A[開始] -->|確認| B{完成？}\n B --> C[結束]"),
        ("sequence", "sequenceDiagram\n Alice->>Bob: Hello\n Bob-->>Alice: Done"),
        ("class", "classDiagram\n Animal <|-- Duck\n Animal : +int age"),
        ("state", "stateDiagram-v2\n [*] --> Ready\n Ready --> Done\n Done --> [*]"),
        ("mindmap", "mindmap\n root((Project))\n  Plan\n  Build"),
        ("user-init", "%%{init: {\"theme\":\"base\",\"themeVariables\":{\"primaryColor\":\"#123456\"}}}%%\nflowchart LR\n A[Custom] --> B[Theme]"),
    ];
    for palette in [Palette::ONE_LIGHT, Palette::ONE_DARK] {
        for (name, source) in cases {
            let output = render_blocking_async(
                DiagramKind::Mermaid,
                source.to_owned(),
                palette,
                "sans-serif".to_owned(),
            )
            .await
            .unwrap_or_else(|error| panic!("{name}: {error}"));
            let svg = std::str::from_utf8(&output.svg).unwrap();
            assert!(!svg.contains("<foreignObject"), "HTML labels in {name}");
            assert!(svg.contains("<text"), "missing SVG labels in {name}");
            if name == "flowchart" {
                assert!(svg.contains("開始"), "missing CJK label");
            }
            if name == "user-init" {
                assert!(svg.contains("#123456"), "lost user theme override");
            }
            assert!(output.w > 0 && output.h > 0, "empty raster for {name}");
            assert_eq!(output.rgba.len(), output.w as usize * output.h as usize * 4);
            assert!(output.rgba.chunks_exact(4).any(|p| p[3] != 0));
        }
    }
}

#[test]
fn mermaid_svg_uses_the_application_palette() {
    let source = "flowchart LR\n A[Start] --> B[Done]";
    let light = render_blocking(
        DiagramKind::Mermaid,
        source,
        &Palette::ONE_LIGHT,
        "sans-serif",
    )
    .unwrap();
    let dark = render_blocking(
        DiagramKind::Mermaid,
        source,
        &Palette::ONE_DARK,
        "sans-serif",
    )
    .unwrap();
    assert_ne!(light, dark);
    for (svg, palette) in [(&light, Palette::ONE_LIGHT), (&dark, Palette::ONE_DARK)] {
        let color = palette.surface;
        let hex = format!(
            "#{:02x}{:02x}{:02x}",
            (color.r * 255.0).round() as u8,
            (color.g * 255.0).round() as u8,
            (color.b * 255.0).round() as u8
        );
        assert!(svg.contains(&hex), "missing palette fill {hex}");
    }
    // White at 8% over #21252b; black at 10.8% over white. These must
    // remain subtle borders, rather than becoming #ffffff / #000000.
    assert!(
        dark.contains("stroke:#33363c"),
        "dark border lost its alpha blend"
    );
    assert!(
        light.contains("stroke:#e3e3e3"),
        "light border lost its alpha blend"
    );
}

#[test]
fn cup_handle_math_renders_via_pipeline() {
    use rmdv::ast::DiagramKind;
    use rmdv::diagram::render_blocking;
    use rmdv::theme::Palette;
    let srcs = [
        r"P_{buy}=P_{handle\_max}\times(1+\delta),\qquad \delta\approx0.1\%-0.5\%",
        r"SL=\min\left\{P_{handle\_min}-1\times ATR_{20},\, P_{handle\_min}(1-\epsilon)\right\},\quad \epsilon\approx3\%",
        r"f^*=\frac{p\cdot R - (1-p)}{R},\quad R=\frac{TP}{SL}-1",
    ];
    let pal = Palette::ONE_DARK;
    for s in srcs {
        match render_blocking(DiagramKind::Math, s, &pal, "JetBrains Mono") {
            Ok(svg) => assert!(
                svg.contains("<path") || svg.contains("<rect"),
                "no glyphs for {s}"
            ),
            Err(e) => panic!("render_blocking Math ERR for {s:?}: {e}"),
        }
    }
}
