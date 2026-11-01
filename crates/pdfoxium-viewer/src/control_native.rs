// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Opt-in local JSON-RPC transport. All application work stays on the UI thread.
use super::Request;
use eframe::egui;
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
};
use web_time::{Duration, Instant};

pub struct Server {
    pub requests: mpsc::Receiver<Request>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    path: PathBuf,
}
impl Server {
    pub fn start(path: &Path, ctx: egui::Context) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let token: String = rand::random::<[u8; 32]>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let endpoint = json!({"version":1,"port":listener.local_addr()?.port(),"token":token});
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        if let Err(error) = writeln!(file, "{endpoint}").and_then(|_| file.sync_all()) {
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        let (tx, requests) = mpsc::sync_channel(64);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let active = Arc::new(AtomicUsize::new(0));
        let worker = thread::Builder::new()
            .name("viewer-control".into())
            .spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if active.load(Ordering::Relaxed) >= 8 {
                                continue;
                            }
                            active.fetch_add(1, Ordering::Relaxed);
                            let active = active.clone();
                            let tx = tx.clone();
                            let token = token.clone();
                            let ctx = ctx.clone();
                            thread::spawn(move || {
                                let _ = connection(stream, &token, tx, ctx);
                                active.fetch_sub(1, Ordering::Relaxed);
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(20))
                        }
                        Err(_) => thread::sleep(Duration::from_millis(100)),
                    }
                }
            });
        match worker {
            Ok(worker) => Ok(Self {
                requests,
                stop,
                thread: Some(worker),
                path: path.to_owned(),
            }),
            Err(error) => {
                let _ = std::fs::remove_file(path);
                Err(error)
            }
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}
fn connection(
    mut stream: TcpStream,
    token: &str,
    tx: mpsc::SyncSender<Request>,
    ctx: egui::Context,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut line = Vec::new();
    // One request per connection; bounded even for a client without a newline.
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut limited = std::io::Read::take(&mut reader, 1024 * 1024 + 1);
    limited.read_until(b'\n', &mut line)?;
    let result = parse(&line, token);
    let (id, result) = match result {
        Err(error) => (Value::Null, Err(error)),
        Ok((id, method, params)) => {
            let (reply, rx) = mpsc::channel();
            let request = Request {
                method,
                params,
                reply,
                since: Instant::now(),
            };
            let result = match tx.try_send(request) {
                Ok(()) => {
                    ctx.request_repaint();
                    rx.recv_timeout(Duration::from_secs(35))
                        .unwrap_or_else(|_| Err("app did not answer before the deadline".into()))
                }
                Err(_) => Err("control queue full or app closed".into()),
            };
            (id, result)
        }
    };
    let response = match result {
        Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
        Err(message) => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":message}}),
    };
    // Send a complete JSON line rather than many formatting fragments while
    // the peer is waiting for the newline that terminates the response.
    let mut encoded = serde_json::to_vec(&response).map_err(std::io::Error::other)?;
    encoded.push(b'\n');
    stream.write_all(&encoded)
}
fn parse(line: &[u8], token: &str) -> Result<(Value, String, Value), String> {
    if line.len() > 1024 * 1024 {
        return Err("request exceeds 1 MiB".into());
    }
    let value: Value = serde_json::from_slice(line).map_err(|e| e.to_string())?;
    if value["token"].as_str() != Some(token) {
        return Err("authentication failed".into());
    }
    if value["jsonrpc"] != "2.0" {
        return Err("jsonrpc must be 2.0".into());
    }
    let method = value["method"].as_str().ok_or("missing method")?.to_owned();
    let params = value.get("params").cloned().unwrap_or(json!({}));
    if !params.is_object() {
        return Err("params must be an object".into());
    }
    Ok((value["id"].clone(), method, params))
}

/// CLI client: the screenshot itself is saved by the UI after GPU readback.
pub fn call(path: &Path, method: String, params: Value) -> Result<Value, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("control endpoint must be a regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("control endpoint must be private (mode 0600)".into());
        }
    }
    let endpoint: Value = serde_json::from_reader(File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let port = endpoint["port"]
        .as_u64()
        .and_then(|v| u16::try_from(v).ok())
        .ok_or("invalid endpoint port")?;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(40)))
        .map_err(|e| e.to_string())?;
    let request =
        json!({"jsonrpc":"2.0","id":1,"token":endpoint["token"],"method":method,"params":params});
    let mut encoded = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
    encoded.push(b'\n');
    stream.write_all(&encoded).map_err(|e| e.to_string())?;
    let mut result = String::new();
    BufReader::new(stream)
        .read_line(&mut result)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&result).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_require_authentication_version_and_object_params() {
        assert!(parse(
            br#"{"jsonrpc":"2.0","method":"ui.quit","token":"wrong"}"#,
            "secret"
        )
        .is_err());
        assert!(parse(
            br#"{"jsonrpc":"1.0","method":"ui.quit","token":"secret"}"#,
            "secret"
        )
        .is_err());
        assert!(parse(
            br#"{"jsonrpc":"2.0","method":"ui.state","token":"secret","params":[]}"#,
            "secret"
        )
        .is_err());
        let (id, method, params) = parse(
            br#"{"jsonrpc":"2.0","id":7,"method":"ui.state","token":"secret"}"#,
            "secret",
        )
        .unwrap();
        assert_eq!(
            (id, method, params),
            (json!(7), "ui.state".into(), json!({}))
        );
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    #[test]
    fn endpoint_is_private_exclusive_and_removed_on_shutdown() {
        let path =
            std::env::temp_dir().join(format!("pdfoxium-control-{}.json", rand::random::<u64>()));
        let server = Server::start(&path, egui::Context::default()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let original = std::fs::read(&path).unwrap();
        assert!(Server::start(&path, egui::Context::default()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let endpoint: Value = serde_json::from_slice(&original).unwrap();
        let stream =
            TcpStream::connect(("127.0.0.1", endpoint["port"].as_u64().unwrap() as u16)).unwrap();
        let mut write = stream.try_clone().unwrap();
        writeln!(
            write,
            "{{\"jsonrpc\":\"2.0\",\"token\":\"wrong\",\"method\":\"ui.quit\"}}"
        )
        .unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        assert!(line.contains("authentication failed"));
        assert!(server.requests.try_recv().is_err());
        drop(server);
        assert!(!path.exists());
    }
}
