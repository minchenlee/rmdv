//! Export the application's Mermaid SVG and raster output for visual inspection.
//! cargo run --example merman_smoke -- /tmp/rmdv-merman

use rmdv::diagram::{render_blocking_async, DiagramKind};
use rmdv::theme::Palette;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args()
        .nth(1)
        .ok_or("usage: merman_smoke <output-directory>")?;
    let directory = std::path::Path::new(&directory);
    std::fs::create_dir_all(directory)?;
    let source = "flowchart LR\n A[開始] -->|讀取文件| B{包含圖表？}\n B -->|是| C[Merman → SVG]\n B -->|否| D[顯示文字]";
    for (name, palette) in [("light", Palette::ONE_LIGHT), ("dark", Palette::ONE_DARK)] {
        let output = render_blocking_async(
            DiagramKind::Mermaid,
            source.to_owned(),
            palette,
            "sans-serif".to_owned(),
        )
        .await?;
        std::fs::write(directory.join(format!("{name}.svg")), &output.svg)?;
        image::save_buffer(
            directory.join(format!("{name}.png")),
            &output.rgba,
            output.w,
            output.h,
            image::ColorType::Rgba8,
        )?;
        println!("{name}: {} × {} pixels", output.w, output.h);
    }
    Ok(())
}
