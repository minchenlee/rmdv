use crate::ipc::{socket, Request, Response};
use anyhow::{anyhow, Result};
use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use interprocess::local_socket::{
    tokio::{prelude::*, Listener, Stream},
    GenericFilePath, ListenerOptions, ToFsName,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// A connected client must deliver its request line within this window, so a
/// silent connection cannot hold the serialised accept loop.
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound on one request line; requests are small JSON objects.
pub const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
/// How long a request may queue behind others before it is refused undispatched.
const DISPATCH_QUEUE_TIMEOUT: Duration = Duration::from_secs(25);
/// How long the app may take to answer. Screenshots reply only after the
/// capture is written, which normally takes well under a second.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
// Read + queue + reply budgets must finish inside the client's 65 s round
// trip, so a request never runs after its client has already given up.
const _: () = assert!(
    REQUEST_READ_TIMEOUT.as_secs() + DISPATCH_QUEUE_TIMEOUT.as_secs() + REPLY_TIMEOUT.as_secs()
        < crate::ipc::client::RESPONSE_TIMEOUT.as_secs()
);

/// Message handed to the Iced update loop.
pub type Pending = (Request, oneshot::Sender<Response>);

/// The primary stable endpoint and, when available, the legacy TMPDIR
/// endpoint. Keeping both alive lets an older client reach a newer instance
/// while new clients can cross shell/tmux environment boundaries.
pub struct ListenerSet {
    listeners: Vec<Listener>,
}

/// Bind the listener, recovering from a stale socket.
pub fn acquire() -> Result<ListenerSet> {
    acquire_paths(&socket::candidate_paths(), !socket::has_override())
}

/// Bind every usable endpoint in `paths`, in order. The first path is the
/// stable endpoint; when `prepare_stable_parent` is set its parent directory is
/// created user-private. Public for integration tests.
pub fn acquire_paths(paths: &[PathBuf], prepare_stable_parent: bool) -> Result<ListenerSet> {
    // First try to connect — if something answers, the caller is not the instance.
    if paths.iter().any(|path| can_connect_blocking(path)) {
        return Err(anyhow!("instance already running"));
    }

    let mut listeners = Vec::new();
    let mut last_error = None;
    for (index, path) in paths.iter().enumerate() {
        #[cfg(unix)]
        if index == 0 && prepare_stable_parent {
            if let Err(error) = prepare_stable_socket_parent(path) {
                last_error = Some(anyhow!(
                    "prepare IPC socket directory {}: {error}",
                    path.display()
                ));
                continue;
            }
        }
        #[cfg(not(unix))]
        let _ = (index, prepare_stable_parent);

        match bind_endpoint(path) {
            Ok(listener) => listeners.push(listener),
            Err(error) => {
                // Before this process owns any endpoint, a bind failure may
                // mean a competing starter won after the initial probe; do not
                // bind only the compatibility alias and become a second
                // instance. Once an endpoint is ours, probing would reach our
                // own listener, so a failed alias is simply skipped.
                if listeners.is_empty()
                    && paths
                        .iter()
                        .any(|candidate| can_connect_blocking(candidate))
                {
                    return Err(anyhow!("instance already running"));
                }
                last_error = Some(error);
            }
        }
    }

    if listeners.is_empty() {
        Err(last_error.unwrap_or_else(|| anyhow!("no usable IPC socket path")))
    } else {
        Ok(ListenerSet { listeners })
    }
}

impl ListenerSet {
    /// Number of bound endpoints.
    pub fn len(&self) -> usize {
        self.listeners.len()
    }

    pub fn is_empty(&self) -> bool {
        self.listeners.is_empty()
    }
}

/// Bind one endpoint without displacing a live listener: an existing socket
/// file is removed only after `AddrInUse` and a failed connect prove it stale.
fn bind_endpoint(path: &Path) -> Result<Listener> {
    let bind = || -> Result<Listener> {
        let name = path_to_name(path)
            .map_err(|error| anyhow!("invalid socket path {}: {error}", path.display()))?;
        ListenerOptions::new()
            .name(name)
            .create_tokio()
            .map_err(|error| anyhow!(BindError(path.to_path_buf(), error)))
    };
    match bind() {
        Err(error) if is_addr_in_use(&error) && !can_connect_blocking(path) => {
            #[cfg(unix)]
            {
                let _ = std::fs::remove_file(path);
            }
            bind()
        }
        other => other,
    }
}

#[derive(Debug)]
struct BindError(PathBuf, std::io::Error);

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bind {}: {}", self.0.display(), self.1)
    }
}

impl std::error::Error for BindError {}

fn is_addr_in_use(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<BindError>()
        .is_some_and(|BindError(_, io)| io.kind() == std::io::ErrorKind::AddrInUse)
}

#[cfg(unix)]
fn prepare_stable_socket_parent(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent == Path::new("/tmp") {
        return Ok(());
    }

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
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
/// back to the connecting client. Requests are read concurrently but dispatched
/// one at a time across both endpoint aliases.
pub async fn run(listeners: ListenerSet, tx: mpsc::Sender<Pending>) {
    let gate = std::sync::Arc::new(tokio::sync::Mutex::new(()));
    let mut tasks = Vec::with_capacity(listeners.listeners.len());
    for listener in listeners.listeners {
        let gate = std::sync::Arc::clone(&gate);
        let tx = tx.clone();
        tasks.push(tokio::spawn(async move {
            loop {
                let conn = match listener.accept().await {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let gate = std::sync::Arc::clone(&gate);
                let tx = tx.clone();
                // Each connection reads its request on its own task, so a
                // slow or silent client never blocks the accept loop. Only
                // the dispatch is serialised.
                tokio::spawn(async move {
                    // best-effort: drop on protocol error, keep listening
                    let _ = handle_one(conn, tx, &gate).await;
                });
            }
        }));
    }

    // Each task is intentionally long-lived. Awaiting them keeps the listener
    // set owned by the subscription for the lifetime of the app.
    for task in tasks {
        let _ = task.await;
    }
}

async fn handle_one(
    stream: Stream,
    mut tx: mpsc::Sender<Pending>,
    gate: &tokio::sync::Mutex<()>,
) -> Result<()> {
    let (recv, mut send) = tokio::io::split(stream);
    let req = tokio::time::timeout(REQUEST_READ_TIMEOUT, read_request(recv))
        .await
        .map_err(|_| anyhow!("request read timed out"))??;
    let id = req.id;

    let Ok(_guard) = tokio::time::timeout(DISPATCH_QUEUE_TIMEOUT, gate.lock()).await else {
        return write_response(&mut send, &Response::err(id, "instance busy, try again")).await;
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send((req, reply_tx)).await?;
    let resp = match tokio::time::timeout(REPLY_TIMEOUT, reply_rx).await {
        Ok(reply) => reply.unwrap_or_else(|_| Response::err(id, "instance shutdown")),
        Err(_) => Response::err(id, "instance did not reply in time"),
    };
    write_response(&mut send, &resp).await
}

async fn write_response<W: tokio::io::AsyncWrite + Unpin>(
    send: &mut W,
    resp: &Response,
) -> Result<()> {
    let mut line = serde_json::to_string(resp)?;
    line.push('\n');
    send.write_all(line.as_bytes()).await?;
    send.flush().await?;
    Ok(())
}

/// Read one newline-terminated JSON request of at most [`MAX_REQUEST_BYTES`].
pub async fn read_request<R: tokio::io::AsyncRead + Unpin>(recv: R) -> Result<Request> {
    let mut reader = BufReader::new(recv.take(MAX_REQUEST_BYTES + 1));
    let mut buf = String::new();
    reader.read_line(&mut buf).await?;
    if buf.is_empty() {
        return Err(anyhow!("empty request"));
    }
    if buf.len() as u64 > MAX_REQUEST_BYTES {
        return Err(anyhow!("request exceeds {MAX_REQUEST_BYTES} bytes"));
    }
    serde_json::from_str(buf.trim_end()).map_err(|e| anyhow!("bad json: {e}"))
}

#[cfg(unix)]
fn path_to_name(p: &Path) -> Result<interprocess::local_socket::Name<'_>> {
    p.to_fs_name::<GenericFilePath>()
        .map_err(|e| anyhow!("name: {e}"))
}

#[cfg(windows)]
fn path_to_name(p: &Path) -> Result<interprocess::local_socket::Name<'static>> {
    use interprocess::local_socket::{GenericNamespaced, ToNsName};
    // Build an *owned* name: `to_ns_name` on a `String` selects the owning impl
    // (`Cow::Owned`), so the returned `Name` carries its own buffer instead of
    // borrowing this local, avoiding E0515 (returning a value that borrows a
    // dropped local).
    let owned = p
        .to_string_lossy()
        .trim_start_matches(r"\\.\pipe\")
        .to_owned();
    owned
        .to_ns_name::<GenericNamespaced>()
        .map_err(|e| anyhow!("name: {e}"))
}
