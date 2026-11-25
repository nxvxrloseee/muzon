use super::control::{queue, ControlServer};
use muzon_core::{
    controller::{Controller, CoreConfig},
    data::{
        db::Db, playback_settings_store::PlaybackSettingsStore, scanner,
        session_store::SessionStore,
    },
    domain::session::Session,
    testing::{scratch_dir, write_tone},
};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
static SERIAL: AtomicUsize = AtomicUsize::new(0);
pub(super) fn fixture() -> (PathBuf, Controller) {
    let name = format!(
        "desktop-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    );
    let dir = scratch_dir(&name);
    let music = dir.join("music");
    let a = write_tone(&music, "a.wav", 20.0);
    let b = write_tone(&music, "b.wav", 20.0);
    std::fs::write(
        music.join("a.lrc"),
        "[00:00]Native lyrics\n[00:02]Second line\n",
    )
    .unwrap();
    let db = Arc::new(Db::open(&dir.join("library.sqlite3")).unwrap());
    db.add_root(&music).unwrap();
    scanner::scan_roots(&db, &db.list_roots().unwrap()).unwrap();
    let store = SessionStore::new(&dir.join("config"));
    store
        .save(&Session {
            queue_paths: vec![a.to_string_lossy().into(), b.to_string_lossy().into()],
            shuffle_order: vec![1, 0],
            cursor: 0,
            volume: 0.0,
            ..Default::default()
        })
        .unwrap();
    let core = Controller::spawn_test(CoreConfig {
        db,
        session_store: store,
        playback_settings_store: PlaybackSettingsStore::new(&dir.join("config")),
        cover_dir: dir.join("covers"),
    })
    .unwrap();
    (dir, core)
}
fn frame(reader: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    assert!(reader.read_line(&mut line).unwrap() > 0);
    serde_json::from_str(&line).unwrap()
}
fn request(reader: &mut BufReader<UnixStream>, id: u32, method: &str, params: Value) -> Value {
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"id":id,"method":method,"params":params})
    )
    .unwrap();
    for _ in 0..64 {
        let value = frame(reader);
        if value.get("id") == Some(&json!(id)) {
            return value;
        }
    }
    panic!("reply not received")
}
#[test]
fn socket_controls_real_queue_and_favorites_and_joins_idle_clients() {
    let _gst = muzon_core::testing::gst_lock();
    let (dir, core) = fixture();
    let path = dir.join("control.sock");
    let mut server = ControlServer::start(core.client(), path.clone()).unwrap();
    let stream = UnixStream::connect(&path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    assert_eq!(
        request(&mut reader, 1, "status", Value::Null)["result"]["isPlaying"],
        false
    );
    let id = core.snapshot().current_track().unwrap().id;
    core.preview_track_tempo(id, 1.25);
    core.save_audio().unwrap();
    assert_eq!(
        request(&mut reader, 10, "status", Value::Null)["result"]["track"]["tempo"],
        1.25
    );
    // NDJSON may arrive in arbitrary byte fragments.
    reader
        .get_mut()
        .write_all(b"{\"id\":2,\"method\":\"sta")
        .unwrap();
    reader.get_mut().write_all(b"tus\"}\n").unwrap();
    assert_eq!(frame(&mut reader)["id"], 2);
    request(&mut reader, 3, "subscribe", json!({"topics":["favorite"]}));
    let on = request(&mut reader, 4, "favorite.toggle", Value::Null);
    assert_eq!(on["result"]["isFavorite"], true);
    let event = frame(&mut reader);
    assert_eq!(event["topic"], "favorite");
    assert_eq!(event["data"]["isFavorite"], true);
    assert!(core.snapshot().queue[0].is_favorite);
    let lyrics = request(&mut reader, 5, "lyrics", Value::Null);
    assert_eq!(
        lyrics["result"]["lyrics"]["lines"][0]["text"],
        "Native lyrics"
    );
    assert!(request(&mut reader, 6, "queue.play", json!({"index":99}))
        .get("error")
        .is_some());
    request(&mut reader, 7, "next", Value::Null);
    assert_eq!(core.snapshot().cursor, Some(1));
    core.pause();
    core.barrier().unwrap();
    let q = request(
        &mut reader,
        8,
        "queue",
        json!({"before":u64::MAX,"after":u64::MAX}),
    );
    assert_eq!(q["result"]["items"].as_array().unwrap().len(), 2);
    assert!(request(
        &mut reader,
        9,
        "favorite.toggle",
        json!({"trackId":i64::MAX})
    )
    .get("error")
    .is_some());
    assert!(ControlServer::start(core.client(), path.clone()).is_err());
    let _idle = UnixStream::connect(&path).unwrap();
    server.shutdown();
    assert!(!path.exists());
    assert!(core.is_running());
}
#[test]
fn invalid_socket_requests_are_bounded_and_regular_files_are_preserved() {
    let _gst = muzon_core::testing::gst_lock();
    let (dir, core) = fixture();
    let path = dir.join("control.sock");
    std::fs::write(&path, b"keep me").unwrap();
    assert!(ControlServer::start(core.client(), path.clone()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
    std::fs::remove_file(&path).unwrap();
    let _server = ControlServer::start(core.client(), path.clone()).unwrap();
    let stream = UnixStream::connect(path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    reader.get_mut().write_all(b"bad json\n").unwrap();
    assert!(frame(&mut reader).get("error").is_some());
    reader.get_mut().write_all(&vec![b'x'; 65 * 1024]).unwrap();
    let mut line = String::new();
    let result = reader.read_line(&mut line);
    assert!(matches!(result, Ok(0)) || result.is_err());
}
#[test]
fn remote_queue_uses_unrotated_shuffle_order() {
    let _gst = muzon_core::testing::gst_lock();
    let (_dir, core) = fixture();
    core.set_shuffle(true);
    let state = core.barrier().unwrap();
    let q = queue(&state, usize::MAX, usize::MAX);
    assert_eq!(q["playingPosition"], 1);
    assert_eq!(q["items"][0]["index"], 1);
    assert_eq!(q["items"][1]["index"], 0);
    assert_eq!(state.display.as_ref(), &vec![0, 1]);
}

#[test]
fn stalled_desktop_io_is_interrupted_by_stop_and_deadline() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let setter = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            stopping.store(true, Ordering::Release);
        });
        let started = std::time::Instant::now();
        let error = super::bounded(&stop, Duration::from_secs(2), std::future::pending::<()>())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("stopped"));
        assert!(started.elapsed() < Duration::from_secs(1));
        setter.await.unwrap();
        let stop = AtomicBool::new(false);
        let error = super::bounded(
            &stop,
            Duration::from_millis(30),
            std::future::pending::<()>(),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("timed out"));
    });
}
