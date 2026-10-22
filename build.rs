//! Embeds every YAML scheme under `assets/themes/base16/` into the binary.
//! Runtime converts each via `theme_import::import_base16_str` so build.rs
//! stays free of project deps beyond the std fs walk.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Read at run time, not with `env!`: worktrees share one target dir, and a
    // compiled build script would otherwise keep a deleted worktree's path and
    // embed no themes.
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let dir: PathBuf = PathBuf::from(manifest_dir)
        .join("assets")
        .join("themes")
        .join("base16");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut entries: Vec<(String, String)> = Vec::new();
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext != "yaml" && ext != "yml" {
                continue;
            }
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("scheme")
                .to_string();
            let body = fs::read_to_string(&p).expect("read theme yaml");
            entries.push((stem, body));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR");
    let out = PathBuf::from(out_dir).join("bundled_themes.rs");
    let mut src = String::new();
    src.push_str("pub const BUNDLED_BASE16: &[(&str, &str)] = &[\n");
    for (stem, body) in &entries {
        src.push_str("    (\"");
        src.push_str(stem);
        src.push_str("\", r####\"");
        src.push_str(body);
        src.push_str("\"####),\n");
    }
    src.push_str("];\n");
    fs::write(out, src).expect("write generated");
}
