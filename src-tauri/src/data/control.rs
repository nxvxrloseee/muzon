//! A small control socket for desktop shells and scripts: what MPRIS can't say.
//!
//! MPRIS covers play/pause/seek and the current track's metadata, but not the
//! queue, favourites or lyrics - so a shell that wants a "like" button or the
//! words of the song has nothing to ask. This is that something:
//! newline-delimited JSON over `$XDG_RUNTIME_DIR/muzon/control.sock`.
//!
//! ```text
//! -> {"id":1,"method":"status"}
//! <- {"id":1,"result":{"track":{...},"isPlaying":true,"positionSecs":12.3,...}}
//! -> {"id":2,"method":"subscribe","params":{"topics":["track","queue"]}}
//! <- {"topic":"track","data":{...}}          (pushed whenever it happens)
//! ```
//!
//! Methods: `status`, `queue` (`before`/`after` around the playing track),
//! `queue.play` (`index` into the queue), `favorite.toggle` (`trackId`, default
//! the playing track), `lyrics` (`path`, default the playing track), `toggle`,
//! `next`, `previous`, `subscribe`.
//! Topics: `track`, `playback`, `queue`, `favorite`.
//!
//! Plain std threads and blocking sockets on purpose: a handful of local
//! clients sending a few small messages don't justify pulling tokio's `net`
//! feature into the build, and nothing here is on the audio path.

use crate::domain::library;
use crate::domain::session::Session;
use crate::state::AppState;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub fn socket_path() -> PathBuf {
    // An override for running a second, isolated instance (development, tests)
    if let Some(p) = std::env::var_os("MUZON_CONTROL_SOCKET") {
        return PathBuf::from(p);
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    runtime.join("muzon").join("control.sock")
}

struct Subscriber {
    stream: UnixStream,
    topics: Vec<String>,
}

/// Everyone who asked to be told about changes. Lives in `AppState` so the
/// polling thread and the Tauri commands can publish without knowing about
/// sockets.
///
/// Publishing only queues the event: a single writer thread does the socket
/// writes, in order. The playback polling thread publishes, and it must never
/// wait on a client that has stopped reading.
pub struct ControlHub {
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
    events: Mutex<Sender<String>>,
}

impl Default for ControlHub {
    fn default() -> Self {
        let subscribers: Arc<Mutex<Vec<Subscriber>>> = Arc::default();
        let (tx, rx) = channel::<String>();
        let subs = subscribers.clone();
        std::thread::spawn(move || {
            for line in rx {
                let topic = serde_json::from_str::<Value>(&line)
                    .ok()
                    .and_then(|v| v.get("topic").and_then(Value::as_str).map(String::from))
                    .unwrap_or_default();
                // A subscriber that has gone away (or stalls past the write
                // timeout) is dropped on the spot
                subs.lock().unwrap().retain_mut(|s| {
                    if !s.topics.iter().any(|t| *t == topic || t == "*") {
                        return true;
                    }
                    s.stream.write_all(line.as_bytes()).is_ok()
                });
            }
        });
        Self { subscribers, events: Mutex::new(tx) }
    }
}

impl ControlHub {
    pub fn publish(&self, topic: &str, data: Value) {
        if self.subscribers.lock().unwrap().is_empty() {
            return;
        }
        let line = format!("{}\n", json!({ "topic": topic, "data": data }));
        let _ = self.events.lock().unwrap().send(line);
    }

    fn add(&self, stream: UnixStream, topics: Vec<String>) {
        self.subscribers.lock().unwrap().push(Subscriber { stream, topics });
    }
}

/// Starts listening. A socket left behind by a crash is replaced; one that
/// still answers means another Muzon owns it, and this one stays quiet.
pub fn serve(app: AppHandle) {
    let path = socket_path();
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    if UnixStream::connect(&path).is_ok() {
        eprintln!("[muzon control] another instance already listens on {}", path.display());
        return;
    }
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[muzon control] can't listen on {}: {e}", path.display());
            return;
        }
    };
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));

    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let app = app.clone();
            std::thread::spawn(move || handle_client(app, stream));
        }
    });
}

fn handle_client(app: AppHandle, stream: UnixStream) {
    let Ok(reader_stream) = stream.try_clone() else {
        return;
    };
    let mut writer = stream;
    let _ = writer.set_write_timeout(Some(Duration::from_secs(2)));

    for line in BufReader::new(reader_stream).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let _ = writeln!(writer, "{}", json!({ "error": format!("bad request: {e}") }));
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);

        let reply = if method == "subscribe" {
            let topics: Vec<String> = params
                .get("topics")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
                .unwrap_or_else(|| vec!["*".into()]);
            match writer.try_clone() {
                Ok(events) => {
                    app.state::<AppState>().control.add(events, topics);
                    Ok(Value::Bool(true))
                }
                Err(e) => Err(e.to_string()),
            }
        } else {
            dispatch(&app, method, &params)
        };

        let response = match reply {
            Ok(result) => json!({ "id": id, "result": result }),
            Err(error) => json!({ "id": id, "error": error }),
        };
        if writeln!(writer, "{response}").is_err() {
            break;
        }
    }
}

fn dispatch(app: &AppHandle, method: &str, params: &Value) -> Result<Value, String> {
    let state = app.state::<AppState>();
    match method {
        "status" => Ok(status(app)),
        "queue" => {
            let before = params.get("before").and_then(Value::as_u64).unwrap_or(3) as usize;
            let after = params.get("after").and_then(Value::as_u64).unwrap_or(30) as usize;
            Ok(queue(app, before, after))
        }
        "queue.play" => {
            let index = params
                .get("index")
                .and_then(Value::as_u64)
                .ok_or("params.index is required")?;
            if index as usize >= state.session.snapshot().queue_paths.len() {
                return Err(format!("no queue item {index}"));
            }
            // The queue store owns order, shuffle and what "playing index N"
            // means for the history - hand it over the same way MPRIS Next does.
            app.emit("remote-play-queue-index", index).map_err(|e| e.to_string())?;
            Ok(Value::Bool(true))
        }
        "favorite.toggle" => {
            let track_id = match params.get("trackId").and_then(Value::as_i64) {
                Some(id) => id as i32,
                None => current_track(app).ok_or("nothing is playing")?.id,
            };
            let is_favorite = library::toggle_favorite(&state.db, track_id).map_err(|e| e.to_string())?;
            announce_favorite(app, track_id, is_favorite, true);
            Ok(json!({ "trackId": track_id, "isFavorite": is_favorite }))
        }
        "lyrics" => {
            let path = match params.get("path").and_then(Value::as_str) {
                Some(p) => p.to_string(),
                None => state.player.status().path.ok_or("nothing is playing")?,
            };
            let lyrics = crate::data::lrc_source::load_for_track(std::path::Path::new(&path));
            Ok(json!({ "path": path, "lyrics": lyrics }))
        }
        "toggle" => {
            let status = state.player.status();
            if status.is_playing {
                state.player.pause().map_err(|e| e.to_string())?;
            } else {
                state.player.play().map_err(|e| e.to_string())?;
            }
            Ok(Value::Bool(!status.is_playing))
        }
        "next" => app.emit("mpris-next", ()).map(|_| Value::Bool(true)).map_err(|e| e.to_string()),
        "previous" => app.emit("mpris-previous", ()).map(|_| Value::Bool(true)).map_err(|e| e.to_string()),
        _ => Err(format!("unknown method: {method}")),
    }
}

fn current_track(app: &AppHandle) -> Option<crate::domain::track::Track> {
    let state = app.state::<AppState>();
    let path = state.player.status().path?;
    state.db.track_by_path(&path).ok().flatten()
}

/// What's playing, with the favourite flag the shell needs for its heart.
pub fn status(app: &AppHandle) -> Value {
    let state = app.state::<AppState>();
    let status = state.player.status();
    json!({
        "track": current_track(app),
        "isPlaying": status.is_playing,
        "positionSecs": status.position_secs,
        "durationSecs": status.duration_secs,
    })
}

/// The queue in the order it will actually play (shuffle applied), as a window
/// around the playing track. `index` is the position in the unshuffled queue,
/// which is what `queue.play` takes back.
pub fn queue(app: &AppHandle, before: usize, after: usize) -> Value {
    let state = app.state::<AppState>();
    let session = state.session.snapshot();
    let n = session.queue_paths.len();
    let (window, playing) = play_window(&session, before, after);
    let items: Vec<Value> = window
        .iter()
        .map(|&(index, position)| {
            let path = &session.queue_paths[index];
            let track = state.db.track_by_path(path).ok().flatten();
            json!({
                "index": index,
                "position": position,
                "playing": playing == Some(position),
                "path": path,
                "title": track.as_ref().map(|t| t.title.clone()),
                "artist": track.as_ref().and_then(|t| t.artist.clone()),
                "durationSecs": track.as_ref().and_then(|t| t.duration_secs),
                "trackId": track.as_ref().map(|t| t.id),
            })
        })
        .collect();

    json!({
        "length": n,
        "playingPosition": playing,
        "shuffle": session.shuffle,
        "repeat": session.repeat,
        "items": items,
    })
}

/// `(queue index, play position)` pairs around the playing track, plus the
/// playing track's play position. Shuffle order applies only when it matches
/// the queue - a stale permutation is ignored rather than trusted.
fn play_window(session: &Session, before: usize, after: usize) -> (Vec<(usize, usize)>, Option<usize>) {
    let n = session.queue_paths.len();
    let order: Vec<usize> = if session.shuffle && session.shuffle_order.len() == n {
        session.shuffle_order.iter().map(|&i| i as usize).collect()
    } else {
        (0..n).collect()
    };
    let playing = usize::try_from(session.cursor)
        .ok()
        .and_then(|cursor| order.iter().position(|&i| i == cursor));
    let (from, to) = match playing {
        Some(p) => (p.saturating_sub(before), (p + after + 1).min(n)),
        None => (0, after.min(n)),
    };
    let window = (from..to).map(|position| (order[position], position)).collect();
    (window, playing)
}

/// A favourite flipped: the shell hears it on the socket, and when the change
/// came from outside the app, the window's own heart is brought along too.
pub fn announce_favorite(app: &AppHandle, track_id: i32, is_favorite: bool, from_remote: bool) {
    let payload = json!({ "trackId": track_id, "isFavorite": is_favorite });
    if from_remote {
        let _ = app.emit("remote-favorite-changed", payload.clone());
    }
    app.state::<AppState>().control.publish("favorite", payload);
}

/// `muzon ctl <method> [json params]`: one request, prints the result.
/// `muzon ctl listen [topics…]` prints events until interrupted.
pub fn cli(args: &[String]) -> i32 {
    let Some(method) = args.first() else {
        eprintln!("usage: muzon ctl <status|queue|lyrics|like|toggle|next|previous|listen> [json]");
        return 2;
    };
    let mut stream = match UnixStream::connect(socket_path()) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("muzon is not running");
            return 1;
        }
    };
    let (method, params) = match method.as_str() {
        "like" => ("favorite.toggle", Value::Null),
        "listen" => {
            let topics: Vec<&str> = if args.len() > 1 {
                args[1..].iter().map(String::as_str).collect()
            } else {
                vec!["*"]
            };
            ("subscribe", json!({ "topics": topics }))
        }
        m => {
            let params = match args.get(1) {
                Some(raw) => match serde_json::from_str(raw) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("params must be JSON: {e}");
                        return 2;
                    }
                },
                None => Value::Null,
            };
            (m, params)
        }
    };
    let request = json!({ "id": 1, "method": method, "params": params });
    if writeln!(stream, "{request}").is_err() {
        eprintln!("muzon closed the connection");
        return 1;
    }
    let listening = method == "subscribe";
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return 1 };
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(err) = v.get("error").and_then(Value::as_str) {
            eprintln!("{err}");
            return 1;
        }
        if listening {
            if v.get("topic").is_some() {
                println!("{line}");
            }
            continue;
        }
        if let Some(result) = v.get("result") {
            println!("{}", serde_json::to_string_pretty(result).unwrap_or_default());
            return 0;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(n: usize, cursor: i32, shuffle: Option<Vec<u32>>) -> Session {
        Session {
            queue_paths: (0..n).map(|i| format!("/music/{i}.flac")).collect(),
            shuffle: shuffle.is_some(),
            shuffle_order: shuffle.unwrap_or_default(),
            cursor,
            ..Session::default()
        }
    }

    #[test]
    fn window_around_the_playing_track() {
        let (w, playing) = play_window(&session(10, 5, None), 2, 3);
        assert_eq!(playing, Some(5));
        assert_eq!(w, vec![(3, 3), (4, 4), (5, 5), (6, 6), (7, 7), (8, 8)]);
    }

    #[test]
    fn window_is_clamped_at_both_ends() {
        let (w, _) = play_window(&session(4, 0, None), 3, 10);
        assert_eq!(w.first(), Some(&(0, 0)));
        assert_eq!(w.len(), 4);
    }

    #[test]
    fn shuffle_order_is_the_play_order() {
        // Playing queue item 2, which is third in the shuffled order
        let (w, playing) = play_window(&session(4, 2, Some(vec![3, 0, 2, 1])), 1, 1);
        assert_eq!(playing, Some(2));
        assert_eq!(w, vec![(0, 1), (2, 2), (1, 3)]);
    }

    #[test]
    fn stale_shuffle_order_is_ignored() {
        let (w, playing) = play_window(&session(3, 1, Some(vec![1, 0])), 0, 0);
        assert_eq!(playing, Some(1));
        assert_eq!(w, vec![(1, 1)]);
    }

    #[test]
    fn nothing_playing_starts_at_the_top() {
        let (w, playing) = play_window(&session(5, -1, None), 3, 2);
        assert_eq!(playing, None);
        assert_eq!(w, vec![(0, 0), (1, 1)]);
    }
}
