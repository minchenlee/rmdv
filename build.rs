//! Embeds every YAML scheme under `assets/themes/base16/` into the binary.
//! Runtime converts each via `theme_import::import_base16_str` so build.rs
//! stays free of project deps beyond the std fs walk.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    build_webkit_runtime();
    let dir: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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

fn build_webkit_runtime() {
    println!("cargo:rerun-if-changed=runtime/mermaid-webkit/worker.swift");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let runtime = root.join("runtime/mermaid-js");
    let files = [
        (
            "mermaid.min.js",
            runtime.join("node_modules/mermaid/dist/mermaid.min.js"),
        ),
        ("aws-icons.json", runtime.join("icon-cache/aws-icons.json")),
        (
            "logos-icons.json",
            runtime.join("node_modules/@iconify-json/logos/icons.json"),
        ),
    ];
    for (name, source) in files {
        println!("cargo:rerun-if-changed={}", source.display());
        assert!(source.is_file(), "Missing Mermaid build asset: {}. Run npm ci and npm run prepare:icons in runtime/mermaid-js", source.display());
        let output = std::process::Command::new("gzip")
            .args(["-n", "-9", "-c"])
            .arg(&source)
            .output()
            .expect("gzip Mermaid assets");
        assert!(output.status.success(), "gzip failed: {}", source.display());
        fs::write(out.join(format!("{name}.gz")), output.stdout).expect("write compressed asset");
    }
    let checksum = std::process::Command::new("shasum")
        .args(["-a", "256"])
        .arg(runtime.join("icon-cache/aws-icons.json"))
        .output()
        .expect("checksum AWS icons");
    assert!(
        checksum.status.success()
            && String::from_utf8_lossy(&checksum.stdout)
                .starts_with("36bff63d2b9f5f94021543a0f0e3b2ed845062a1953eb9038dbf23651fd47111"),
        "AWS icon checksum mismatch; run npm run prepare:icons"
    );
    for (name, source) in [
        (
            "MERMAID-LICENSE.txt",
            runtime.join("node_modules/mermaid/LICENSE"),
        ),
        (
            "THIRD_PARTY_NOTICES.md",
            runtime.join("THIRD_PARTY_NOTICES.md"),
        ),
        (
            "AWS-ICONS-LICENSE.txt",
            runtime.join("AWS-ICONS-LICENSE.txt"),
        ),
    ] {
        println!("cargo:rerun-if-changed={}", source.display());
        fs::copy(source, out.join(name)).expect("copy runtime license");
    }
    println!("cargo:rerun-if-changed=runtime/mermaid-js/icons.mjs");
    println!("cargo:rerun-if-changed=runtime/mermaid-js/page.mjs");
    let icons = fs::read_to_string(runtime.join("icons.mjs"))
        .expect("read icon adapter")
        .replace("export ", "");
    let page = fs::read_to_string(runtime.join("page.mjs")).expect("read Mermaid page")
        .replace("import mermaid from '/node_modules/mermaid/dist/mermaid.esm.min.mjs';", "")
        .replace("import { loadAwsIcons } from '/icons.mjs';", "")
        .replace("const aws = await loadAwsIcons();", "const aws = createAwsIcons(JSON.parse(document.getElementById('aws-icons').textContent));")
        .replace("const logosResponse = await fetch('/node_modules/@iconify-json/logos/icons.json');", "")
        .replace("if (!logosResponse.ok) throw Error('Logos 圖示包尚未安裝，請在 runtime/mermaid-js 執行 npm ci。');", "")
        .replace("const logos = await logosResponse.json();", "const logos = JSON.parse(document.getElementById('logos-icons').textContent);")
        .replace("return { svg };", "return { svg, width: Math.max(1, Math.floor(box.width * scale)), height: Math.max(1, Math.floor(box.height * scale)) };");
    fs::write(out.join("native.js"), format!("(async () => {{\n{icons}\n{page}\nwindow.webkit.messageHandlers.ready.postMessage({{ready:true}});\n}})().catch(error => window.webkit.messageHandlers.ready.postMessage({{error:String(error.message || error)}}));")).expect("write WebKit bootstrap");
    let source = root.join("runtime/mermaid-webkit/worker.swift");
    let status = std::process::Command::new("xcrun")
        .args([
            "swiftc",
            "-O",
            "-whole-module-optimization",
            "-module-cache-path",
        ])
        .arg(out.join("swift-module-cache"))
        .arg(&source)
        .arg("-o")
        .arg(out.join("rmdv-mermaid-webkit"))
        .status()
        .expect("compile WebKit helper with Xcode command line tools");
    assert!(status.success(), "Swift WebKit helper compilation failed");
}
