//! In-app auto-update for mdv.
//!
//! Flow (mac + linux only):
//!  1. On launch, [`check`] queries the GitHub Releases `latest.json` manifest.
//!  2. If a newer version exists, mdv downloads the matching artifact in the
//!     background and verifies its SHA-256 against the manifest.
//!  3. A toast/banner invites the user to install. On confirm, [`apply`]
//!     self-replaces the running app bundle/binary and relaunches.
//!
//! Windows is intentionally excluded: it ships the NSIS installer instead.

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Manifest published alongside each GitHub release (`latest.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes_url: Option<String>,
    pub platforms: std::collections::HashMap<String, PlatformEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlatformEntry {
    pub url: String,
    pub sha256: String,
}

/// Result of a successful update check + download: the verified artifact is on
/// disk and ready for [`apply`].
#[derive(Debug, Clone)]
pub struct ReadyUpdate {
    pub version: String,
    pub notes_url: Option<String>,
    /// Path to the verified, downloaded artifact (`.app.tar.gz` on macOS,
    /// `.AppImage` on Linux).
    pub artifact: PathBuf,
    /// Expected SHA-256 of the artifact; re-checked from disk in [`apply`]
    /// since the staged file sits in a world-writable temp dir.
    pub sha256: String,
}

/// Manifest URL. Points at the `latest.json` asset on the newest GitHub
/// release. The `latest` redirect resolves to whatever tag is most recent.
const MANIFEST_URL: &str =
    "https://github.com/minchenlee/mdv/releases/latest/download/latest.json";

/// Artifact URLs from the manifest must live under this repo's release
/// downloads — a tampered manifest must not be able to point elsewhere.
const ARTIFACT_URL_PREFIX: &str = "https://github.com/minchenlee/mdv/releases/download/";

/// Refuse artifacts larger than this; release bundles are tens of MB.
const MAX_ARTIFACT_BYTES: u64 = 200 * 1024 * 1024;

/// The `platforms` key for the current OS + arch, matching the keys the
/// release workflow writes. Returns `None` on unsupported platforms (Windows),
/// which disables auto-update entirely.
fn platform_key() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("darwin-aarch64"),
        ("macos", "x86_64") => Some("darwin-x86_64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        _ => None,
    }
}

/// Compare two semver-ish strings (`x.y.z`). Returns true if `remote` is
/// strictly newer than `current`. Non-numeric/garbage segments sort as 0.
fn is_newer(remote: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.trim_start_matches('v')
            .split('.')
            .map(|s| s.trim().parse().unwrap_or(0))
            .collect()
    }
    let (r, c) = (parts(remote), parts(current));
    for i in 0..r.len().max(c.len()) {
        let rv = r.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if rv != cv {
            return rv > cv;
        }
    }
    false
}

/// Check for an update and, if one exists, download + verify it. Returns
/// `Ok(None)` when already up to date or on an unsupported platform. Network
/// and verification errors propagate as `Err` (callers should treat a failed
/// check as a silent no-op).
pub async fn check_and_download() -> Result<Option<ReadyUpdate>> {
    let Some(key) = platform_key() else {
        return Ok(None);
    };
    let current = env!("CARGO_PKG_VERSION");

    let client = reqwest::Client::builder()
        .user_agent(concat!("mdv/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .build()?;

    let manifest_bytes = client
        .get(MANIFEST_URL)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("fetch manifest")?
        .error_for_status()
        .context("manifest status")?
        .bytes()
        .await
        .context("read manifest body")?;
    let manifest: Manifest =
        serde_json::from_slice(&manifest_bytes).context("parse manifest")?;

    if !is_newer(&manifest.version, current) {
        return Ok(None);
    }
    let entry = manifest
        .platforms
        .get(key)
        .ok_or_else(|| anyhow!("no artifact for platform {key}"))?
        .clone();
    if !entry.url.starts_with(ARTIFACT_URL_PREFIX) {
        bail!("artifact url outside release downloads: {}", entry.url);
    }

    let resp = client
        .get(&entry.url)
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .context("download artifact")?
        .error_for_status()
        .context("artifact status")?;
    if resp.content_length().is_some_and(|len| len > MAX_ARTIFACT_BYTES) {
        bail!("artifact too large");
    }
    let bytes = resp.bytes().await.context("read artifact body")?;
    if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        bail!("artifact too large");
    }

    verify_sha256(&bytes, &entry.sha256)?;

    let artifact = staged_path(&entry.url, &manifest.version)?;
    tokio::fs::write(&artifact, &bytes)
        .await
        .context("write staged artifact")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // AppImage must be executable; tarballs don't care.
        if artifact.extension().and_then(|e| e.to_str()) == Some("AppImage") {
            let mut perm = std::fs::metadata(&artifact)?.permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&artifact, perm)?;
        }
    }

    Ok(Some(ReadyUpdate {
        version: manifest.version,
        notes_url: manifest.notes_url,
        artifact,
        sha256: entry.sha256,
    }))
}

fn verify_sha256(bytes: &[u8], expected_hex: &str) -> Result<()> {
    let actual = sha256_hex(bytes);
    if !actual.eq_ignore_ascii_case(expected_hex.trim()) {
        bail!("sha256 mismatch: expected {expected_hex}, got {actual}");
    }
    Ok(())
}

/// Minimal SHA-256 (FIPS 180-4). Avoids pulling a crypto crate for a single
/// integrity check.
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ ((!v[4]) & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

/// Where to stage a downloaded artifact. Uses the OS temp dir + a versioned
/// filename so re-downloads overwrite cleanly.
fn staged_path(url: &str, version: &str) -> Result<PathBuf> {
    // Both pieces come from the remote manifest — strip anything that could
    // escape the temp dir or smuggle path separators.
    fn sanitize(s: &str) -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            .collect()
    }
    let name = url
        .rsplit('/')
        .next()
        .map(sanitize)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "mdv-update".into());
    Ok(std::env::temp_dir().join(format!("mdv-{}-{name}", sanitize(version))))
}

/// Apply a downloaded update in place, then relaunch the new build.
///
/// macOS: extract the `.app.tar.gz` and `ditto`/`mv` it over the currently
/// running `.app` bundle, then `open` the replacement.
///
/// Linux: copy the verified `.AppImage` over the current executable path and
/// re-exec it.
///
/// Returns `Ok(())` only on success; on failure the running app is left
/// untouched and the caller surfaces the error.
pub fn apply(ready: &ReadyUpdate) -> Result<()> {
    // Re-verify from disk: the staged file lives in a world-writable temp dir
    // and could have been swapped since the in-memory check at download time.
    let bytes = std::fs::read(&ready.artifact).context("read staged artifact")?;
    verify_sha256(&bytes, &ready.sha256)?;
    drop(bytes);

    match std::env::consts::OS {
        "macos" => apply_macos(&ready.artifact),
        "linux" => apply_linux(&ready.artifact),
        other => bail!("self-update unsupported on {other}"),
    }
}

/// Locate the `.app` bundle that contains the running executable, e.g.
/// `/Applications/mdv.app` from `/Applications/mdv.app/Contents/MacOS/mdv`.
fn current_app_bundle() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("current_exe")?;
    // …/mdv.app/Contents/MacOS/mdv → ascend to the `.app`.
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .ok_or_else(|| anyhow!("not running from a .app bundle"))?;
    Ok(bundle.to_path_buf())
}

fn apply_macos(tarball: &Path) -> Result<()> {
    let target = current_app_bundle()?;
    let staging = std::env::temp_dir().join("mdv-update-extract");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).context("create extract dir")?;

    // Extract the .app.tar.gz into staging.
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(tarball)
        .arg("-C")
        .arg(&staging)
        .status()
        .context("spawn tar")?;
    if !status.success() {
        bail!("tar extraction failed");
    }
    let new_app = std::fs::read_dir(&staging)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .ok_or_else(|| anyhow!("no .app in update archive"))?;

    // Replace atomically-ish: move old aside, ditto new into place. ditto
    // preserves the signed bundle structure + xattrs so Gatekeeper accepts it.
    let backup = target.with_extension("app.old");
    let _ = std::fs::remove_dir_all(&backup);
    std::fs::rename(&target, &backup).context("move current bundle aside")?;
    let status = std::process::Command::new("ditto")
        .arg(&new_app)
        .arg(&target)
        .status()
        .context("spawn ditto")?;
    if !status.success() {
        // Roll back.
        let _ = std::fs::remove_dir_all(&target);
        let _ = std::fs::rename(&backup, &target);
        bail!("ditto copy failed; rolled back");
    }
    let _ = std::fs::remove_dir_all(&backup);

    // Relaunch the freshly installed bundle, then exit this process.
    std::process::Command::new("open")
        .arg(&target)
        .spawn()
        .context("relaunch new bundle")?;
    std::process::exit(0);
}

fn apply_linux(appimage: &Path) -> Result<()> {
    let exe = std::env::current_exe().context("current_exe")?;
    // Replace the running executable's file. On Linux the inode of a running
    // binary stays valid after the path is overwritten, so a rename works.
    let tmp = exe.with_extension("new");
    std::fs::copy(appimage, &tmp).context("copy new AppImage")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&tmp)?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&tmp, perm)?;
    }
    std::fs::rename(&tmp, &exe).context("swap executable")?;

    // Re-exec the new binary in place of this process.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&exe).exec();
        bail!("re-exec failed: {err}");
    }
    #[cfg(not(unix))]
    bail!("linux apply on non-unix");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn version_compare() {
        assert!(is_newer("0.3.0", "0.2.0"));
        assert!(is_newer("v0.2.1", "0.2.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }
}
