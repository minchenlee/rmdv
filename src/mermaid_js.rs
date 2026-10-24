//! Shared official Mermaid.js worker. System WebKit on macOS preserves HTML labels and
//! browser font shaping; resvg cannot rasterize Mermaid's foreignObject labels.
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(not(target_os = "macos"))]
use std::process::Command;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{mpsc, LazyLock, Mutex};
use std::time::Duration;

use base64::Engine;
use serde::Deserialize;

use crate::diagram::{RenderOutput, MAX_SOURCE_BYTES, MAX_SVG_BYTES};

static WORKER: LazyLock<Mutex<Option<Worker>>> = LazyLock::new(|| {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(15));
        if let Ok(mut guard) = WORKER.try_lock() {
            if guard
                .as_ref()
                .is_some_and(|worker| worker.last_used.elapsed() >= Duration::from_secs(60))
            {
                *guard = None;
            }
        }
    });
    Mutex::new(None)
});
const RESPONSE_LIMIT: u64 = 52 * 1024 * 1024;

#[derive(Deserialize)]
struct Response {
    svg: Option<String>,
    png: Option<String>,
    error: Option<String>,
}

struct Worker {
    child: Child,
    input: ChildStdin,
    replies: mpsc::Receiver<Result<String, String>>,
    last_used: std::time::Instant,
    #[cfg(target_os = "macos")]
    directory: std::path::PathBuf,
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        #[cfg(target_os = "macos")]
        crate::mermaid_webkit::cleanup(&self.directory);
    }
}
impl Worker {
    fn start() -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        let (mut command, directory) = {
            let runtime = crate::mermaid_webkit::prepare()?;
            (runtime.command, runtime.directory)
        };
        #[cfg(not(target_os = "macos"))]
        let mut command = {
            let directory = std::env::var_os("RMDV_MERMAID_RUNTIME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/mermaid-js")
                });
            let mut command =
                Command::new(std::env::var_os("RMDV_NODE").unwrap_or_else(|| "node".into()));
            command.arg(directory.join("worker.mjs"));
            command
        };
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Could not start Mermaid rendering worker: {e}"))?;
        let input = child.stdin.take().ok_or("worker stdin unavailable")?;
        let output = child.stdout.take().ok_or("worker stdout unavailable")?;
        let (send, replies) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut line = String::new();
                let result = reader
                    .by_ref()
                    .take(RESPONSE_LIMIT + 1)
                    .read_line(&mut line);
                let reply = match result {
                    Ok(0) => Err("Mermaid.js worker exited; check the rendering runtime".into()),
                    Ok(_) if line.len() as u64 > RESPONSE_LIMIT => {
                        Err("Mermaid.js response too large".into())
                    }
                    Ok(_) => Ok(line),
                    Err(e) => Err(e.to_string()),
                };
                let failed = reply.is_err();
                if send.send(reply).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            replies,
            last_used: std::time::Instant::now(),
            #[cfg(target_os = "macos")]
            directory,
        })
    }
}

fn request(source: &str, config: &serde_json::Value, raster: bool) -> Result<Response, String> {
    if source.len() > MAX_SOURCE_BYTES + 4096 {
        return Err("diagram too large".into());
    }
    let mut guard = WORKER
        .lock()
        .map_err(|_| "Mermaid.js worker lock poisoned")?;
    if guard.is_none() {
        *guard = Some(Worker::start()?);
    }
    let worker = guard.as_mut().unwrap();
    let result = (|| {
        let payload = serde_json::json!({ "source": source, "config": config, "raster": raster });
        writeln!(worker.input, "{payload}").map_err(|e| e.to_string())?;
        worker.input.flush().map_err(|e| e.to_string())?;
        let line = worker
            .replies
            .recv_timeout(Duration::from_secs(45))
            .map_err(|_| "Mermaid.js worker timed out or disconnected".to_string())??;
        serde_json::from_str::<Response>(&line)
            .map_err(|e| format!("Invalid Mermaid.js response: {e}"))
    })();
    let response = match result {
        Ok(response) => response,
        Err(error) => {
            *guard = None;
            return Err(error);
        }
    };
    if let Some(worker) = guard.as_mut() {
        worker.last_used = std::time::Instant::now();
    }
    if let Some(error) = response.error {
        return Err(error);
    }
    if response
        .svg
        .as_ref()
        .map_or(true, |s| s.len() > MAX_SVG_BYTES)
    {
        return Err("Mermaid.js SVG missing or too large".into());
    }
    Ok(response)
}

pub(crate) fn svg(source: &str, config: &serde_json::Value) -> Result<String, String> {
    request(source, config, false)?
        .svg
        .ok_or_else(|| "Mermaid.js SVG missing".into())
}

pub(crate) fn render(source: &str, config: &serde_json::Value) -> Result<RenderOutput, String> {
    let response = request(source, config, true)?;
    let png = base64::engine::general_purpose::STANDARD
        .decode(response.png.ok_or("Mermaid.js raster missing")?)
        .map_err(|e| e.to_string())?;
    let mut decoder =
        image::ImageReader::with_format(std::io::Cursor::new(png), image::ImageFormat::Png);
    decoder.limits(image::Limits::default());
    let rgba = decoder.decode().map_err(|e| e.to_string())?.into_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 || w > 1600 || h > 8192 || u64::from(w) * u64::from(h) > 8_000_000 {
        return Err("Mermaid.js raster exceeds size limit".into());
    }
    Ok(RenderOutput {
        svg: response.svg.unwrap().into_bytes(),
        rgba: rgba.into_raw(),
        w,
        h,
    })
}
