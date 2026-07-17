use crate::ipc::{socket, Request, Response};
use anyhow::{anyhow, Result};
use interprocess::local_socket::{
    tokio::{prelude::*, Stream},
    GenericFilePath, ToFsName,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Attempt a single round-trip. Returns `Ok(Some(response))` if an instance is
/// listening, `Ok(None)` if no instance is running (caller should become the
/// instance), `Err` on protocol/io errors after a successful connect.
pub async fn try_send(req: &Request) -> Result<Option<Response>> {
    let path = socket::default_path();
    let name = match path_to_name(&path) {
        Ok(n) => n,
        Err(e) => return Err(anyhow!("invalid socket path {}: {e}", path.display())),
    };

    let stream = match Stream::connect(name).await {
        Ok(s) => s,
        Err(e) if is_no_listener(&e) => return Ok(None),
        Err(e) => return Err(anyhow!("connect failed: {e}")),
    };

    use tokio::io::split;
    let (recv, mut send) = split(stream);

    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    send.write_all(line.as_bytes()).await?;
    send.flush().await?;
    drop(send); // half-close so server's read_line returns EOF after our line

    let mut reader = BufReader::new(recv);
    let mut buf = String::new();
    reader.read_line(&mut buf).await?;
    if buf.is_empty() {
        return Err(anyhow!("ipc disconnect"));
    }
    let resp: Response = serde_json::from_str(buf.trim_end())?;
    Ok(Some(resp))
}

#[cfg(unix)]
fn path_to_name(p: &std::path::Path) -> std::io::Result<interprocess::local_socket::Name<'_>> {
    p.to_fs_name::<GenericFilePath>()
}

#[cfg(windows)]
fn path_to_name(p: &std::path::Path) -> std::io::Result<interprocess::local_socket::Name<'static>> {
    use interprocess::local_socket::{GenericNamespaced, ToNsName};
    // Build an *owned* name: `to_ns_name` on a `String` selects the owning impl
    // (`Cow::Owned`), so the returned `Name` carries its own buffer instead of
    // borrowing this local, avoiding E0515 (returning a value that borrows a
    // dropped local).
    let owned = p
        .to_string_lossy()
        .trim_start_matches(r"\\.\pipe\")
        .to_owned();
    owned.to_ns_name::<GenericNamespaced>()
}

fn is_no_listener(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::NotFound
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::AddrNotAvailable
    )
}
