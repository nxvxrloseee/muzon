//! NDJSON compatibility with the Tauri socket. Each bounded client worker owns
//! its reader and writer; replies and events cannot interleave within a frame.
use muzon_core::{
    controller::{ControllerClient, Snapshot},
    data::lrc_source,
};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    net::Shutdown,
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
const MAX_CLIENTS: usize = 32;
const MAX_REQUEST: usize = 64 * 1024;
pub struct ControlServer {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    _lock: File,
}
impl ControlServer {
    pub fn start(client: ControllerClient, path: PathBuf) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut lock_path = path.as_os_str().to_os_string();
        lock_path.push(".lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(Path::new(&lock_path))?;
        lock.try_lock()?;
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if !meta.file_type().is_socket() {
                anyhow::bail!("refusing to replace a non-socket: {}", path.display());
            }
            match UnixStream::connect(&path) {
                Ok(_) => anyhow::bail!("another player owns {}", path.display()),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                    ) =>
                {
                    std::fs::remove_file(&path)?
                }
                Err(e) => return Err(e.into()),
            }
        }
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let meta = std::fs::symlink_metadata(&path)?;
        let identity = (meta.dev(), meta.ino());
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        eprintln!("[muzon control] {}", path.display());
        let thread = thread::Builder::new()
            .name("muzon-control".into())
            .spawn(move || {
                let mut clients: Vec<JoinHandle<()>> = Vec::new();
                while !stopped.load(Ordering::Acquire) {
                    clients.retain(|t| !t.is_finished());
                    match listener.accept() {
                        Ok((stream, _)) if clients.len() < MAX_CLIENTS => {
                            let client = client.clone();
                            let stop = stopped.clone();
                            clients
                                .push(thread::spawn(move || handle_client(client, stream, stop)));
                        }
                        Ok((stream, _)) => {
                            let _ = stream.shutdown(Shutdown::Both);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(20))
                        }
                        Err(_) => break,
                    }
                }
                stopped.store(true, Ordering::Release);
                for client in clients {
                    let _ = client.join();
                }
                drop(listener);
                if std::fs::symlink_metadata(&path).is_ok_and(|m| (m.dev(), m.ino()) == identity) {
                    let _ = std::fs::remove_file(path);
                }
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
            _lock: lock,
        })
    }
    pub fn request_shutdown(&self) {
        self.stop.store(true, Ordering::Release);
    }
    pub fn shutdown(&mut self) {
        self.request_shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for ControlServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn write_frame(stream: &mut UnixStream, value: Value) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(&value)?;
    if bytes.len() > 1024 * 1024 {
        return Err(std::io::Error::other("response too large"));
    }
    bytes.push(b'\n');
    stream.write_all(&bytes)
}
fn handle_client(client: ControllerClient, mut stream: UnixStream, stop: Arc<AtomicBool>) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut topics = Vec::<String>::new();
    let mut prev = client.snapshot();
    while !stop.load(Ordering::Acquire) {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => input.extend_from_slice(&chunk[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
        if input.len() > MAX_REQUEST {
            break;
        }
        while let Some(end) = input.iter().position(|&b| b == b'\n') {
            if stop.load(Ordering::Acquire) {
                return;
            }
            let line: Vec<u8> = input.drain(..=end).collect();
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let request = serde_json::from_slice::<Value>(&line);
            let frame = match request {
                Err(e) => json!({"error":format!("bad request: {e}")}),
                Ok(request) => {
                    let id = request.get("id").cloned().unwrap_or(Value::Null);
                    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
                    let params = request.get("params").cloned().unwrap_or(Value::Null);
                    let result = if method == "subscribe" {
                        topics = params
                            .get("topics")
                            .and_then(Value::as_array)
                            .map(|a| {
                                a.iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_owned)
                                    .collect()
                            })
                            .unwrap_or_else(|| vec!["*".into()]);
                        Ok(Value::Bool(true))
                    } else {
                        dispatch(&client, method, &params, &stop)
                    };
                    match result {
                        Ok(value) => json!({"id":id,"result":value}),
                        Err(error) => json!({"id":id,"error":error}),
                    }
                }
            };
            if write_frame(&mut stream, frame).is_err() {
                return;
            }
        }
        let snapshot = client.snapshot();
        for (topic, data) in changes(&prev, &snapshot) {
            if topics.iter().any(|t| t == "*" || t == topic)
                && write_frame(&mut stream, json!({"topic":topic,"data":data})).is_err()
            {
                return;
            }
        }
        prev = snapshot;
    }
}
pub fn status(s: &Snapshot) -> Value {
    let track = s
        .current_track()
        .filter(|_| s.current_path.is_some())
        .map(|track| {
            let mut current = track.clone();
            current.tempo = s.track_tempo(track);
            current
        });
    json!({"sleep":{"remainingSecs":s.sleep.remaining_secs,"endOfTrack":s.sleep.end_of_track},"track":track,"isPlaying":s.is_playing,"positionSecs":s.position_secs,"durationSecs":if s.current_path.is_some() {s.display_duration()} else {0.0}})
}

pub fn queue(s: &Snapshot, before: usize, after: usize) -> Value {
    let playing = s.cursor.and_then(|i| s.order.iter().position(|&q| q == i));
    let n = s.order.len();
    let (from, to) = playing
        .map(|p| {
            (
                p.saturating_sub(before),
                p.saturating_add(after).saturating_add(1).min(n),
            )
        })
        .unwrap_or((0, after.min(n)));
    let items:Vec<Value>=(from..to).take(2048).filter_map(|position| {
        let &index=s.order.get(position)?;let t=s.queue.get(index)?;
        Some(json!({"index":index,"position":position,"playing":playing==Some(position),"path":t.path,"title":t.title,"artist":t.artist,"durationSecs":t.duration_secs,"trackId":t.id}))
    }).collect();
    json!({"length":s.queue.len(),"playingPosition":playing,"shuffle":s.shuffle,"repeat":match s.repeat {muzon_queue::RepeatMode::Off=>"off",muzon_queue::RepeatMode::One=>"one",muzon_queue::RepeatMode::All=>"all"},"items":items})
}
fn amount(p: &Value, key: &str, default: usize) -> usize {
    p.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(default)
}
fn dispatch(
    client: &ControllerClient,
    method: &str,
    p: &Value,
    stop: &AtomicBool,
) -> Result<Value, String> {
    match method {
        "status" => Ok(status(&client.snapshot())),
        "queue" => Ok(queue(
            &client.snapshot(),
            amount(p, "before", 3),
            amount(p, "after", 30),
        )),
        "queue.play" => {
            let i = p
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|i| usize::try_from(i).ok())
                .ok_or("params.index is required")?;
            if i >= client.snapshot().queue.len() {
                return Err(format!("no queue item {i}"));
            }
            client.play_queue_index(i);
            client.barrier()?;
            Ok(Value::Bool(true))
        }
        "favorite.toggle" => {
            let id = match p.get("trackId") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_i64()
                        .and_then(|n| i32::try_from(n).ok())
                        .ok_or("invalid trackId")?,
                ),
            };
            let (id, on) = client.toggle_favorite(id)?;
            Ok(json!({"trackId":id,"isFavorite":on}))
        }
        "lyrics" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| client.snapshot().current_path.clone())
                .ok_or("nothing is playing")?;
            let result = super::read_job(move || {
                let lyrics = lrc_source::load_for_track(Path::new(&path));
                json!({"path":path,"lyrics":lyrics})
            })?;
            wait_read(result, stop, Duration::from_secs(2))
        }
        "toggle" => {
            client.toggle_play();
            Ok(Value::Bool(client.barrier()?.is_playing))
        }
        "next" => {
            client.next();
            client.barrier()?;
            Ok(Value::Bool(true))
        }
        "previous" => {
            client.previous();
            client.barrier()?;
            Ok(Value::Bool(true))
        }
        _ => Err(format!("unknown method: {method}")),
    }
}
fn changes(prev: &Snapshot, s: &Snapshot) -> Vec<(&'static str, Value)> {
    let mut events = Vec::new();
    if prev.current_path != s.current_path || prev.cursor != s.cursor {
        events.push(("track", status(s)));
    }
    if prev.is_playing != s.is_playing || prev.seek_revision != s.seek_revision {
        events.push(("playback", status(s)));
    }
    if !Arc::ptr_eq(&prev.queue, &s.queue)
        || !Arc::ptr_eq(&prev.order, &s.order)
        || prev.cursor != s.cursor
        || prev.shuffle != s.shuffle
        || prev.repeat != s.repeat
    {
        events.push(("queue", queue(s, 3, 30)));
    }
    if !Arc::ptr_eq(&prev.library, &s.library) {
        let old: std::collections::HashMap<i32, bool> =
            prev.library.iter().map(|t| (t.id, t.is_favorite)).collect();
        for t in s.library.iter() {
            if old.get(&t.id).is_some_and(|&v| v != t.is_favorite) {
                events.push((
                    "favorite",
                    json!({"trackId":t.id,"isFavorite":t.is_favorite}),
                ));
            }
        }
    }
    events
}

fn wait_read<T>(
    rx: std::sync::mpsc::Receiver<T>,
    stop: &AtomicBool,
    limit: Duration,
) -> Result<T, String> {
    let deadline = std::time::Instant::now() + limit;
    loop {
        if stop.load(Ordering::Acquire) {
            return Err("desktop integration stopped".into());
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err("lyrics read timed out".into());
        }
        match rx.recv_timeout(remaining.min(Duration::from_millis(20))) {
            Ok(value) => return Ok(value),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
}
