//! Extract the embedded macOS WebKit helper and local Mermaid assets lazily.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) struct Runtime {
    pub command: Command,
    pub directory: PathBuf,
}

pub(crate) fn prepare() -> Result<Runtime, String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("rmdv-webkit-{}-{nonce}", std::process::id()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        for (name, data) in [
            (
                "mermaid.min.js",
                &include_bytes!(concat!(env!("OUT_DIR"), "/mermaid.min.js.gz"))[..],
            ),
            (
                "aws-icons.json",
                &include_bytes!(concat!(env!("OUT_DIR"), "/aws-icons.json.gz"))[..],
            ),
            (
                "logos-icons.json",
                &include_bytes!(concat!(env!("OUT_DIR"), "/logos-icons.json.gz"))[..],
            ),
        ] {
            let mut contents = Vec::new();
            flate2::read::GzDecoder::new(data)
                .take(16 * 1024 * 1024)
                .read_to_end(&mut contents)
                .map_err(|e| e.to_string())?;
            std::fs::write(directory.join(name), contents).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            directory.join("native.js"),
            include_bytes!(concat!(env!("OUT_DIR"), "/native.js")),
        )
        .map_err(|e| e.to_string())?;
        for (name, contents) in [
            (
                "MERMAID-LICENSE.txt",
                &include_bytes!(concat!(env!("OUT_DIR"), "/MERMAID-LICENSE.txt"))[..],
            ),
            (
                "THIRD_PARTY_NOTICES.md",
                &include_bytes!(concat!(env!("OUT_DIR"), "/THIRD_PARTY_NOTICES.md"))[..],
            ),
            (
                "AWS-ICONS-LICENSE.txt",
                &include_bytes!(concat!(env!("OUT_DIR"), "/AWS-ICONS-LICENSE.txt"))[..],
            ),
        ] {
            std::fs::write(directory.join(name), contents).map_err(|e| e.to_string())?;
        }
        let executable = directory.join("rmdv-mermaid-webkit");
        std::fs::write(
            &executable,
            include_bytes!(concat!(env!("OUT_DIR"), "/rmdv-mermaid-webkit")),
        )
        .map_err(|e| e.to_string())?;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let mut command = Command::new(&executable);
        command.arg(&directory);
        Ok(Runtime {
            command,
            directory: directory.clone(),
        })
    })();
    if result.is_err() {
        cleanup(&directory);
    }
    result
}

pub(crate) fn cleanup(directory: &Path) {
    let _ = std::fs::remove_dir_all(directory);
}
