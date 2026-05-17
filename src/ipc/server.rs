use crate::ipc::{socket, Request, Response};
use anyhow::{anyhow, Result};
use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use interprocess::local_socket::{
    tokio::{prelude::*, Listener, Stream},
    GenericFilePath, ListenerOptions, ToFsName,
};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Message handed to the Iced update loop.
pub type Pending = (Request, oneshot::Sender<Response>);

/// Bind the listener, recovering from a stale socket.
pub fn acquire() -> Result<Listener> {
    let path = socket::default_path();
    // First try to connect — if something answers, the caller is not the instance.
    if can_connect_blocking(&path) {
        return Err(anyhow!("instance already running"));
    }
    // Stale or absent — best-effort unlink (unix only; Windows pipes don't persist).
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(&path);
    }
    let name = path_to_name(&path)?;
    let opts = ListenerOptions::new().name(name);
    let listener = opts
        .create_tokio()
        .map_err(|e| anyhow!("bind {}: {e}", path.display()))?;
    Ok(listener)
}

#[cfg(unix)]
fn can_connect_blocking(path: &Path) -> bool {
    use std::os::unix::net::UnixStream;
    UnixStream::connect(path).is_ok()
}

#[cfg(windows)]
fn can_connect_blocking(path: &Path) -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .is_ok()
}

/// Run the listener loop, forwarding requests through `tx` and writing replies
/// back to the connecting client. Serialises clients (one at a time).
pub async fn run(listener: Listener, mut tx: mpsc::Sender<Pending>) {
    loop {
        let conn = match listener.accept().await {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Err(_e) = handle_one(conn, &mut tx).await {
            // best-effort: drop on protocol error, keep listening
        }
    }
}

async fn handle_one(stream: Stream, tx: &mut mpsc::Sender<Pending>) -> Result<()> {
    let (recv, mut send) = tokio::io::split(stream);
    let mut reader = BufReader::new(recv);
    let mut buf = String::new();
    reader.read_line(&mut buf).await?;
    if buf.is_empty() {
        return Err(anyhow!("empty request"));
    }
    let req: Request = serde_json::from_str(buf.trim_end())
        .map_err(|e| anyhow!("bad json: {e}"))?;
    let id = req.id;
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send((req, reply_tx)).await?;
    let resp = reply_rx
        .await
        .unwrap_or_else(|_| Response::err(id, "instance shutdown"));
    let mut line = serde_json::to_string(&resp)?;
    line.push('\n');
    send.write_all(line.as_bytes()).await?;
    send.flush().await?;
    Ok(())
}

#[cfg(unix)]
fn path_to_name(p: &Path) -> Result<interprocess::local_socket::Name<'_>> {
    p.to_fs_name::<GenericFilePath>()
        .map_err(|e| anyhow!("name: {e}"))
}

#[cfg(windows)]
fn path_to_name(p: &Path) -> Result<interprocess::local_socket::Name<'_>> {
    use interprocess::local_socket::{GenericNamespaced, ToNsName};
    let s = p.to_string_lossy();
    let trimmed = s.trim_start_matches(r"\\.\pipe\");
    trimmed
        .to_ns_name::<GenericNamespaced>()
        .map_err(|e| anyhow!("name: {e}"))
}
