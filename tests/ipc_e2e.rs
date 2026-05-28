use std::process::Command;

#[test]
fn list_sections_subprocess_emits_json_array() {
    let exe = env!("CARGO_BIN_EXE_mdv");
    let out = Command::new(exe)
        .args(["list-sections", "tests/fixtures/sections.md"])
        .output()
        .expect("spawn mdv");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let arr = v.as_array().expect("array");
    assert!(arr.iter().any(|s| s["path"] == "Foo/Install/Setup"));
    assert!(arr.iter().any(|s| s["path"] == "Foo/Usage/Setup"));
}

#[test]
fn list_sections_pretty_flag() {
    let exe = env!("CARGO_BIN_EXE_mdv");
    let out = Command::new(exe)
        .args(["--pretty", "list-sections", "tests/fixtures/sections.md"])
        .output()
        .expect("spawn mdv");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('\n'), "pretty output should be multiline");
}
