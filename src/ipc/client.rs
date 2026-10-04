use crate::ipc::{socket, Request, Response};
use anyhow::{anyhow, Result};
use interprocess::local_socket::{
    tokio::{prelude::*, Stream},
    GenericFilePath, ToFsName,
};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Per-endpoint connect budget. A local socket either answers at once or is
/// not usable.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Round-trip budget once connected. The server's read, queue and reply
/// budgets add up to less than this, so its own error reply arrives first.
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(65);

/// Attempt a single round-trip. Returns `Ok(Some(response))` if an instance is
/// listening, `Ok(None)` if no instance is running (caller should become the
/// instance), `Err` on protocol/io errors after a successful connect.
pub async fn try_send(req: &Request) -> Result<Option<Response>> {
    // A failing candidate (e.g. a permission error on the stable path) must
    // not hide an instance reachable through a later alias.
    let mut first_error = None;
    let mut saw_no_listener = false;
    for path in socket::candidate_paths() {
        let name = match path_to_name(&path) {
            Ok(n) => n,
            Err(_) => continue,
        };

        let stream = match tokio::time::timeout(CONNECT_TIMEOUT, Stream::connect(name)).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) if is_no_listener(&e) => {
                saw_no_listener = true;
                continue;
            }
            Ok(Err(e)) => {
                first_error.get_or_insert_with(|| anyhow!("connect {}: {e}", path.display()));
                continue;
            }
            Err(_) => {
                first_error.get_or_insert_with(|| anyhow!("connect {}: timed out", path.display()));
                continue;
            }
        };

        return tokio::time::timeout(RESPONSE_TIMEOUT, round_trip(stream, req))
            .await
            .map_err(|_| anyhow!("no reply from rmdv within {}s", RESPONSE_TIMEOUT.as_secs()))?
            .map(Some);
    }

    // Missing, redirected, or unusual terminal environment information must
    // not prevent the normal caller from becoming the first instance.
    match first_error {
        Some(error) if !saw_no_listener => Err(error),
        _ => Ok(None),
    }
}

async fn round_trip(stream: Stream, req: &Request) -> Result<Response> {
    let (recv, mut send) = tokio::io::split(stream);

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
    Ok(serde_json::from_str(buf.trim_end())?)
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
