use crate::data::mpris::loop_status_of;
use crate::domain::session::{RepeatMode, Session};
use crate::state::AppState;
use mpris_server::Property;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub fn get_session(state: State<AppState>) -> Session {
    state.session.snapshot()
}

/// Replaces the queue half of the session. Split from `set_session_progress`
/// because the queue is the big, rarely-changing part - playing a whole library
/// as one queue is one path per track, and it must not cross the IPC boundary
/// again every few seconds just because the playhead moved.
#[tauri::command]
#[specta::specta]
pub fn set_session_queue(state: State<AppState>, queue_paths: Vec<String>, shuffle_order: Vec<u32>) {
    state.session.update(|session| {
        session.queue_paths = queue_paths;
        session.shuffle_order = shuffle_order;
    });
}

/// The cheap, frequently-updated half: where playback is and how it's
/// configured. In-memory only - `save_session` owns the disk write.
#[tauri::command]
#[specta::specta]
pub fn set_session_progress(
    state: State<AppState>,
    cursor: i32,
    position_secs: f64,
    volume: f64,
    shuffle: bool,
    repeat: RepeatMode,
) {
    let before = state.session.snapshot();
    state.session.update(|session| {
        session.cursor = cursor;
        session.position_secs = position_secs;
        session.volume = volume;
        session.shuffle = shuffle;
        session.repeat = repeat;
    });

    // The session is also where MPRIS reads Shuffle and LoopStatus from, so a
    // toggle in the app has to be announced from here - nothing else knows the
    // switches moved.
    if before.shuffle != shuffle || before.repeat != repeat {
        state.mpris.notify(vec![
            Property::Shuffle(shuffle),
            Property::LoopStatus(loop_status_of(repeat)),
        ]);
    }
}

/// Flushes the in-memory session to disk. A no-op when nothing has changed
/// since the last flush.
#[tauri::command]
#[specta::specta]
pub fn save_session(state: State<AppState>) -> Result<(), String> {
    state.session.flush().map_err(|e| e.to_string())
}
