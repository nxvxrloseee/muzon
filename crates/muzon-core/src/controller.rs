//! The native player's control worker.
//!
//! One thread owns the queue, the playback policy and the audio engine. The
//! UI never touches GStreamer or the queue directly: it sends [`Command`]s
//! and reads the latest [`Snapshot`]. That is what keeps "who decides what
//! plays next" in exactly one place — the same place that arms the gapless
//! follow-on, so a hand-off that already happened on the pipeline is never
//! followed by a second `next` from the UI.
//!
//! Deliberate shape of the plumbing:
//!
//! - **Commands** travel over a channel the worker drains at the top of every
//!   loop iteration. They are few and user-paced.
//! - **Snapshots** are a single shared "latest value" cell, not a queue. The
//!   position changes five times a second; a channel would either back up or
//!   drop, and either way the UI would be showing something stale. Reading the
//!   cell is cheap (`Arc` clone) and nothing accumulates.
//! - **The library is never cloned per tick.** Both the snapshot and the UI's
//!   copy hold an `Arc<Vec<Track>>`, replaced wholesale only when a scan
//!   produced something new.
//! - **Scanning and cover/lyrics decoding run on their own threads** and come
//!   back through the same command channel, so a slow disk or a large folder
//!   never stalls playback control.
//! - **Shutdown is explicit**: [`Controller::shutdown`] sends one command, the
//!   worker puts both GStreamer decks back into `NULL`, flushes the session
//!   and returns, and the caller joins it.
//!
//! Queue order, navigation, removal semantics and session restoration are
//! `muzon-queue`'s decisions. Nothing here re-implements them; the worker only
//! executes what those functions answer.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use muzon_queue::{
    append_tracks, display_order_indices, next_action, peek_next_index, plan_restore,
    play_order_indices, previous_action, remove_at, Navigation, QueueShape, RepeatMode,
};

use crate::data::audio::pipeline::{AudioPlayer, PlaybackError, PlaybackTick};
use crate::data::audio::pipeline::{MAX_TEMPO, MIN_TEMPO};
use crate::data::covers;
use crate::data::db::Db;
use crate::data::lrc_source;
use crate::data::playback_settings_store::PlaybackSettingsStore;
use crate::data::scanner;
use crate::data::session_store::{SessionState, SessionStore};
use crate::domain::library::ScanReport;
use crate::domain::lyrics::LrcLine;
use crate::domain::playback_settings::{PlaybackEdit, PlaybackSettings};
use crate::domain::session::RepeatMode as SessionRepeatMode;
use crate::domain::{playlist::Playlist, Track};
use crate::sleep_timer::{SleepChoice, SleepState, SleepTimer};

/// How often the worker polls GStreamer and republishes the snapshot.
pub const POSITION_POLL: Duration = Duration::from_millis(200);
/// Poll rate while a crossfade is ramping: a short fade stepped five times a
/// second is audibly a staircase.
pub const CROSSFADE_POLL: Duration = Duration::from_millis(40);
/// How often the in-memory session is written to disk, besides shutdown.
pub const SESSION_FLUSH_INTERVAL: Duration = Duration::from_secs(60);

/// How many broken files in a row are skipped before playback gives up.
///
/// Without this a queue of missing files walks the whole library at five
/// errors a second, each one advancing to the next. Same threshold as the
/// desktop frontend's `playerStore.ts`.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// What the worker needs to run. Everything is a store the caller already
/// owns, except the profile-local cover cache directory.
pub struct CoreConfig {
    pub db: Arc<Db>,
    pub session_store: SessionStore,
    pub playback_settings_store: PlaybackSettingsStore,
    /// Directory the thumbnail cache is written to; create it under the
    /// profile, never in a shared system location.
    pub cover_dir: PathBuf,
}

impl std::fmt::Debug for CoreConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoreConfig")
            .field("cover_dir", &self.cover_dir)
            .finish_non_exhaustive()
    }
}

/// A track's cover and local lyrics, loaded off the worker thread.
#[derive(Debug, Clone, Default)]
pub struct Media {
    /// The track this was loaded for. `None` before anything was requested.
    pub track_path: Option<String>,
    /// Encoded image bytes (JPEG or PNG) for the cover, when the file has one.
    pub cover: Option<Arc<Vec<u8>>>,
    pub lyrics: Vec<LrcLine>,
    /// True once loading finished for [`Media::track_path`] — including the
    /// "nothing found" outcome, which is not an error.
    pub loaded: bool,
}

/// The state the UI renders. A new `Arc` is published on every worker tick;
/// the expensive parts ([`Snapshot::library`], [`Snapshot::queue`],
/// [`Snapshot::display`]) are shared and only rebuilt when they changed.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Monotonic; lets a frontend skip re-rendering an unchanged snapshot
    /// (except the position, which moves every tick anyway).
    pub revision: u64,
    pub seek_revision: u64,
    pub seek_position_secs: f64,
    pub tempo: f64,
    pub library: Arc<Vec<Track>>,
    pub playlists: Arc<Vec<Playlist>>,
    pub playback: Arc<PlaybackSettings>,
    /// Preferences edited this run; metadata/search arrays stay immutable during drags.
    pub tempo_overrides: Arc<HashMap<i32, f64>>,
    pub audio_dirty: bool,
    pub audio_error: Option<String>,
    /// The queue in library order, entries owned by the queue itself.
    pub queue: Arc<Vec<Track>>,
    /// Indices into [`Snapshot::queue`] in the order the queue view shows:
    /// `muzon-queue`'s `display_order_indices`, playing element first.
    pub display: Arc<Vec<usize>>,
    /// Unrotated playback order for remote queue windows.
    pub order: Arc<Vec<usize>>,
    pub cursor: Option<usize>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub is_playing: bool,
    pub sleep: SleepState,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub current_path: Option<String>,
    pub volume: f64,
    /// A scan or a rescan is in flight.
    pub scanning: bool,
    /// Informational line ("scanned: 12 added"), or a non-fatal problem.
    pub message: Option<String>,
    /// Last fatal problem (a file that could not be played, a scan failure).
    pub error: Option<String>,
}

impl Snapshot {
    /// An empty library and queue — the state before anything loads, and the
    /// starting point for view tests that do not want a controller.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            revision: 0,
            seek_revision: 0,
            seek_position_secs: 0.0,
            tempo: 1.0,
            library: Arc::new(Vec::new()),
            playlists: Arc::new(Vec::new()),
            playback: Arc::new(PlaybackSettings::default_settings()),
            tempo_overrides: Arc::new(HashMap::new()),
            audio_dirty: false,
            audio_error: None,
            queue: Arc::new(Vec::new()),
            display: Arc::new(Vec::new()),
            order: Arc::new(Vec::new()),
            cursor: None,
            shuffle: false,
            repeat: RepeatMode::Off,
            is_playing: false,
            sleep: SleepState::default(),
            position_secs: 0.0,
            duration_secs: 0.0,
            current_path: None,
            volume: 1.0,
            scanning: false,
            message: None,
            error: None,
        }
    }

    /// The track under the queue cursor, if the queue has one.
    #[must_use]
    pub fn current_track(&self) -> Option<&Track> {
        self.cursor.and_then(|index| self.queue.get(index))
    }

    /// Per-track preference, including edits cached without rebuilding metadata.
    pub fn track_tempo(&self, track: &Track) -> f64 {
        self.tempo_overrides
            .get(&track.id)
            .copied()
            .unwrap_or(track.tempo)
    }

    /// Duration to show: the pipeline's answer when it has one, otherwise the
    /// tag's, so a paused restored track does not show `0:00`.
    #[must_use]
    pub fn display_duration(&self) -> f64 {
        if self.duration_secs > 0.0 {
            self.duration_secs
        } else {
            self.current_track()
                .and_then(|track| track.duration_secs)
                .unwrap_or(0.0)
        }
    }
}

/// Persistent playlist edits are serialized with all other core commands.
#[derive(Debug, Clone)]
pub enum PlaylistEdit {
    Create { name: String, tracks: Vec<i32> },
    Rename { id: i32, name: String },
    Delete { id: i32 },
    Add { id: i32, tracks: Vec<i32> },
    Remove { id: i32, track: i32 },
    Move { id: i32, track: i32, offset: i32 },
}

/// Commands the UI (or the worker's own helper threads) can send.
#[derive(Debug)]
pub(crate) enum Command {
    LoadEditor {
        id: i32,
        reply: Sender<Result<crate::editing::EditorSource, String>>,
    },
    SaveEdit {
        id: i32,
        edit: crate::editing::EditRequest,
        reply: Sender<Result<(), String>>,
    },
    EditFinished {
        result: Result<Track, String>,
        reply: Sender<Result<(), String>>,
    },
    AddFolder(PathBuf),
    MoveQueue {
        from: usize,
        to: usize,
    },
    Rescan,
    RefreshLibrary,
    /// Internal: a scan thread finished.
    ScanFinished(Result<ScanReport, String>),
    /// Replace the queue with tracks named by id, starting at `start_id` —
    /// what clicking a row in the (filtered) library does.
    SetQueue {
        ids: Vec<i32>,
        start_id: i32,
    },
    PlayIndex(usize),
    TogglePlay,
    SetSleep(SleepChoice),
    Spectrum(bool),
    Play,
    Pause,
    Stop,
    Seek(f64),
    SetVolume(f64),
    Next,
    Previous,
    SetShuffle(bool),
    ToggleShuffle,
    CycleRepeat,
    SetRepeat(RepeatMode),
    SetTempo(f64),
    PlaybackEdit(PlaybackEdit),
    TrackTempo {
        id: i32,
        rate: f64,
    },
    SaveAudio(Sender<Result<(), String>>),
    SeekFor {
        path: String,
        cursor: Option<usize>,
        position: f64,
    },
    Barrier(Sender<Arc<Snapshot>>),
    Favorite {
        id: Option<i32>,
        reply: Sender<Result<(i32, bool), String>>,
    },
    Enqueue(Vec<i32>),
    Remove(usize),
    /// Internal: load cover/lyrics on a helper thread.
    RequestMedia(String),
    /// Internal: the helper thread answered.
    MediaReady {
        generation: u64,
        path: String,
        cover: Option<(String, Vec<u8>)>,
        lyrics: Option<Vec<LrcLine>>,
    },
    PlaylistEdit {
        edit: PlaylistEdit,
        reply: Sender<Result<Option<i32>, String>>,
    },
    Shutdown,
}

/// The shared cells the UI reads.
struct Shared {
    snapshot: RwLock<Arc<Snapshot>>,
    media: RwLock<Arc<Media>>,
    spectrum: RwLock<Arc<Vec<f32>>>,
    spectrum_enabled: std::sync::atomic::AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            snapshot: RwLock::new(Arc::new(Snapshot::empty())),
            media: RwLock::new(Arc::new(Media::default())),
            spectrum: RwLock::new(Arc::new(Vec::new())),
            spectrum_enabled: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

/// Handle to the playback worker. Dropping it shuts the worker down, so an
/// app that forgets to call [`Controller::shutdown`] still exits cleanly.
pub struct Controller {
    client: ControllerClient,
    handle: Option<JoinHandle<()>>,
}

/// A clonable command/snapshot endpoint. Dropping a client never stops playback.
/// Only the owning Controller joins and shuts down the audio worker.
#[derive(Clone)]
pub struct ControllerClient {
    sender: Sender<Command>,
    shared: Arc<Shared>,
}
impl std::ops::Deref for Controller {
    type Target = ControllerClient;
    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl std::fmt::Debug for Controller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Controller")
            .field("running", &self.is_running())
            .finish_non_exhaustive()
    }
}

impl Controller {
    /// Starts the worker. Returns only once the audio engine is up, so a
    /// missing GStreamer surfaces as an error here rather than as a UI that
    /// silently never plays.
    pub fn spawn(config: CoreConfig) -> anyhow::Result<Self> {
        Self::spawn_inner(config, false)
    }

    /// Test entry point: the same worker, with both decks pointed at a
    /// `fakesink` so no audio device is needed.
    #[cfg(any(test, feature = "test-support"))]
    pub fn spawn_test(config: CoreConfig) -> anyhow::Result<Self> {
        Self::spawn_inner(config, true)
    }

    fn spawn_inner(config: CoreConfig, fake_sink: bool) -> anyhow::Result<Self> {
        // Only tests ask for fake sinks; a real build has no use for the flag.
        #[cfg(not(any(test, feature = "test-support")))]
        let _ = fake_sink;
        let (sender, receiver) = mpsc::channel();
        let shared = Arc::new(Shared::new());
        let (ready_tx, ready_rx) = mpsc::channel::<anyhow::Result<()>>();

        let worker_shared = Arc::clone(&shared);
        let worker_sender = sender.clone();
        let handle = thread::Builder::new()
            .name("muzon-core-worker".into())
            .spawn(move || match AudioPlayer::new() {
                Ok(player) => {
                    #[cfg(any(test, feature = "test-support"))]
                    if fake_sink {
                        if let Err(e) = player.use_fake_sinks() {
                            eprintln!("[muzon core] fakesink unavailable: {e}");
                        }
                    }
                    let mut worker =
                        Worker::new(config, player, worker_shared, worker_sender, receiver);
                    if !fake_sink {
                        let mut watch = crate::data::library_watch::LibraryWatch::new();
                        watch.sync(&worker.config.db.list_roots().unwrap_or_default());
                        worker.watch = Some(watch);
                        worker.rescan();
                    }
                    if ready_tx.send(Ok(())).is_err() {
                        return;
                    }
                    worker.run();
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                client: ControllerClient { sender, shared },
                handle: Some(handle),
            }),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => {
                let _ = handle.join();
                Err(anyhow::anyhow!("playback worker stopped before it started"))
            }
        }
    }

    /// Shares access without transferring the worker's shutdown ownership.
    pub fn client(&self) -> ControllerClient {
        self.client.clone()
    }

    /// Whether the worker thread is still alive.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    /// Stops the worker, flushes its session and joins it. Idempotent.
    pub fn shutdown(&mut self) {
        self.stop_and_join();
    }
    fn stop_and_join(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = self.sender.send(Command::Shutdown);
            let _ = handle.join();
        }
    }
}
impl ControllerClient {
    /// Latest published state; no track-list copies.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.shared.snapshot.read().unwrap())
    }
    /// Latest asynchronously loaded local media.
    pub fn media(&self) -> Arc<Media> {
        Arc::clone(&self.shared.media.read().unwrap())
    }
    /// Waits for all commands sent before this call to be reflected in a snapshot.
    /// Call on a blocking worker, never the UI or async executor thread.
    pub fn barrier(&self) -> Result<Arc<Snapshot>, String> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(Command::Barrier(tx))
            .map_err(|_| "playback worker stopped".to_owned())?;
        rx.recv_timeout(Duration::from_secs(3))
            .map_err(|e| e.to_string())
    }
    /// Changes a favorite on the playback worker and updates both library and queue.
    pub fn toggle_favorite(&self, id: Option<i32>) -> Result<(i32, bool), String> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(Command::Favorite { id, reply: tx })
            .map_err(|_| "playback worker stopped".to_owned())?;
        rx.recv_timeout(Duration::from_secs(3))
            .map_err(|e| e.to_string())?
    }
    /// Apply an edit and publish the refreshed playlist snapshot before returning.
    /// Like barrier(), call this from a blocking worker, not the UI thread.
    pub fn edit_playlist(&self, edit: PlaylistEdit) -> Result<Option<i32>, String> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(Command::PlaylistEdit { edit, reply: tx })
            .map_err(|_| "playback worker stopped".to_owned())?;
        rx.recv_timeout(Duration::from_secs(3))
            .map_err(|e| e.to_string())?
    }
    /// Live previews are serialized on the core worker; disk writes are coalesced.
    pub fn preview_playback(&self, edit: PlaybackEdit) {
        self.send(Command::PlaybackEdit(edit));
    }
    /// A track id is captured before a drag, so advancing cannot retarget it.
    pub fn preview_track_tempo(&self, id: i32, rate: f64) {
        self.send(Command::TrackTempo { id, rate });
    }
    pub fn save_audio(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(Command::SaveAudio(tx))
            .map_err(|_| "playback worker stopped".to_owned())?;
        rx.recv_timeout(Duration::from_secs(3))
            .map_err(|e| e.to_string())?
    }
    pub fn set_repeat(&self, repeat: RepeatMode) {
        self.send(Command::SetRepeat(repeat));
    }
    pub fn set_tempo(&self, rate: f64) {
        self.send(Command::SetTempo(rate));
    }
    /// Conditional seek: a stale MPRIS SetPosition cannot seek the next occurrence.
    pub fn seek_for(&self, path: String, cursor: Option<usize>, position: f64) {
        self.send(Command::SeekFor {
            path,
            cursor,
            position,
        });
    }

    pub fn add_folder(&self, path: PathBuf) {
        self.send(Command::AddFolder(path));
    }

    pub fn refresh_library(&self) {
        self.send(Command::RefreshLibrary);
    }
    pub fn rescan(&self) {
        self.send(Command::Rescan);
    }

    pub fn set_queue(&self, ids: Vec<i32>, start_id: i32) {
        self.send(Command::SetQueue { ids, start_id });
    }

    pub fn play_queue_index(&self, index: usize) {
        self.send(Command::PlayIndex(index));
    }

    pub fn set_spectrum(&self, on: bool) {
        self.send(Command::Spectrum(on));
    }
    pub fn spectrum(&self) -> Arc<Vec<f32>> {
        self.shared.spectrum.read().unwrap().clone()
    }
    pub fn set_sleep(&self, choice: SleepChoice) {
        self.send(Command::SetSleep(choice));
    }

    pub fn toggle_play(&self) {
        self.send(Command::TogglePlay);
    }

    pub fn play(&self) {
        self.send(Command::Play);
    }

    pub fn pause(&self) {
        self.send(Command::Pause);
    }

    pub fn stop(&self) {
        self.send(Command::Stop);
    }

    pub fn seek(&self, position_secs: f64) {
        self.send(Command::Seek(position_secs));
    }

    pub fn set_volume(&self, volume: f64) {
        self.send(Command::SetVolume(volume));
    }

    pub fn next(&self) {
        self.send(Command::Next);
    }

    pub fn previous(&self) {
        self.send(Command::Previous);
    }

    pub fn set_shuffle(&self, shuffle: bool) {
        self.send(Command::SetShuffle(shuffle));
    }

    pub fn toggle_shuffle(&self) {
        self.send(Command::ToggleShuffle);
    }

    pub fn cycle_repeat(&self) {
        self.send(Command::CycleRepeat);
    }

    /// Appends tracks (by library id) to the end of the queue.
    pub fn enqueue(&self, ids: Vec<i32>) {
        self.send(Command::Enqueue(ids));
    }

    /// Drops one entry by its position in the queue list (not the display
    /// order — callers map a clicked view row back through
    /// [`Snapshot::display`] first).
    pub fn move_queue(&self, from: usize, to: usize) {
        self.send(Command::MoveQueue { from, to });
    }
    pub fn remove_queue_index(&self, index: usize) {
        self.send(Command::Remove(index));
    }

    /// Asks for the cover and local lyrics of a track. Duplicate requests for
    /// the same path in a row are ignored; nothing here touches the network.
    pub fn load_editor(&self, id: i32) -> Result<crate::editing::EditorSource, String> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(Command::LoadEditor { id, reply: tx })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }
    pub fn save_edit(&self, id: i32, edit: crate::editing::EditRequest) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(Command::SaveEdit {
                id,
                edit,
                reply: tx,
            })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }
    pub fn request_media(&self, path: String) {
        self.send(Command::RequestMedia(path));
    }

    fn send(&self, command: Command) {
        // A closed channel means the worker is gone; the UI notices through
        // `is_running` and the last published snapshot, so there is nothing
        // useful left to do with the error here.
        let _ = self.sender.send(command);
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

/// `muzon-queue` and the persisted session each have a `RepeatMode`; the
/// session one is the IPC/serialization type and predates the shared crate.
/// Keeping them apart is deliberate (the IPC contract must not move), so the
/// controller maps between them at the two places it touches the session.
fn repeat_to_session(repeat: RepeatMode) -> SessionRepeatMode {
    match repeat {
        RepeatMode::Off => SessionRepeatMode::Off,
        RepeatMode::All => SessionRepeatMode::All,
        RepeatMode::One => SessionRepeatMode::One,
    }
}

fn repeat_from_session(repeat: SessionRepeatMode) -> RepeatMode {
    match repeat {
        SessionRepeatMode::Off => RepeatMode::Off,
        SessionRepeatMode::All => RepeatMode::All,
        SessionRepeatMode::One => RepeatMode::One,
    }
}

/// A tiny deterministic LCG. `muzon-queue` deliberately has no randomness, so
/// drawing a new shuffle permutation is the caller's job — this is that job.
struct Rng {
    state: u64,
}

impl Rng {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5EED_CAFE_F00D);
        Self {
            state: nanos ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }
}

/// Fisher–Yates over `0..n`.
fn shuffle_permutation(n: usize, rng: &mut Rng) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

/// The worker's own state. One instance lives for the thread's lifetime.
struct Worker {
    config: CoreConfig,
    player: AudioPlayer,
    rx: Receiver<Command>,
    tx: Sender<Command>,
    shared: Arc<Shared>,

    library: Arc<Vec<Track>>,
    playlists: Arc<Vec<Playlist>>,
    playback: Arc<PlaybackSettings>,
    tempo_overrides: Arc<HashMap<i32, f64>>,
    pending_tempos: HashMap<i32, f64>,
    playback_dirty: bool,
    audio_save_at: Option<Instant>,
    audio_error: Option<String>,
    library_by_id: HashMap<i32, usize>,
    shape: QueueShape<Track>,
    /// The queue as the snapshot hands it out: rebuilt only when the queue
    /// changes, so a tick shares it instead of cloning every track.
    queue: Arc<Vec<Track>>,
    display: Arc<Vec<usize>>,
    order: Arc<Vec<usize>>,
    shuffle: bool,
    repeat: RepeatMode,

    /// Queue index of the track the engine was told to follow on with; the
    /// gapless/crossfade hand-off is committed against this, not searched by
    /// path (a repeated track appears more than once).
    armed: Option<usize>,
    consecutive_failures: u32,
    sleep: SleepTimer,

    playing: bool,
    position: f64,
    duration: f64,
    current_path: Option<String>,
    volume: f64,

    scanning: bool,
    message: Option<String>,
    error: Option<String>,

    session: SessionState,
    /// True when the saved queue could not be matched against the library at
    /// startup: keep that session on disk instead of overwriting it with an
    /// empty queue on the first tick.
    preserve_session: bool,

    media_requested: Option<String>,
    media_generation: u64,
    file_lock: Arc<Mutex<()>>,
    edit_busy: bool,
    watch: Option<crate::data::library_watch::LibraryWatch>,
    listen_counted: bool,
    revision: u64,
    seek_revision: u64,
    seek_position_secs: f64,
    jobs: Vec<JoinHandle<()>>,
    rescan_pending: bool,
    rng: Rng,
}

impl Worker {
    fn new(
        config: CoreConfig,
        player: AudioPlayer,
        shared: Arc<Shared>,
        tx: Sender<Command>,
        rx: Receiver<Command>,
    ) -> Self {
        let session = SessionState::new(config.session_store.clone());
        let mut worker = Self {
            config,
            player,
            rx,
            tx,
            shared,
            library: Arc::new(Vec::new()),
            playlists: Arc::new(Vec::new()),
            playback: Arc::new(PlaybackSettings::default_settings()),
            tempo_overrides: Arc::new(HashMap::new()),
            pending_tempos: HashMap::new(),
            playback_dirty: false,
            audio_save_at: None,
            audio_error: None,
            library_by_id: HashMap::new(),
            shape: QueueShape::new(Vec::new()),
            queue: Arc::new(Vec::new()),
            display: Arc::new(Vec::new()),
            order: Arc::new(Vec::new()),
            shuffle: false,
            repeat: RepeatMode::Off,
            armed: None,
            consecutive_failures: 0,
            sleep: SleepTimer::default(),
            playing: false,
            position: 0.0,
            duration: 0.0,
            current_path: None,
            volume: 1.0,
            scanning: false,
            message: None,
            error: None,
            session,
            preserve_session: false,
            media_requested: None,
            media_generation: 0,
            file_lock: Arc::new(Mutex::new(())),
            edit_busy: false,
            watch: None,
            listen_counted: false,
            revision: 0,
            seek_revision: 0,
            seek_position_secs: 0.0,
            jobs: Vec::new(),
            rescan_pending: false,
            rng: Rng::new(),
        };
        if let Err(error) = worker.config.db.ensure_listen_outbox() {
            worker.error = Some(format!("listen queue: {error}"));
        }
        worker.load_library();
        if let Err(error) = worker.load_playlists() {
            worker.error = Some(error);
        }
        worker.restore();
        worker.publish();
        worker
    }

    // --- main loop ---------------------------------------------------------

    fn run(&mut self) {
        let mut last_flush = Instant::now();
        loop {
            // Bound command processing so continuous slider input cannot
            // starve the audio transition clock.
            for _ in 0..64 {
                match self.rx.try_recv() {
                    Ok(command) => {
                        if !self.handle(command) {
                            self.finish();
                            return;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.finish();
                        return;
                    }
                }
            }

            let mut watch_scan = false;
            if let Some(watch) = &mut self.watch {
                if watch.needs_rearm() {
                    watch_scan = watch.sync(&self.config.db.list_roots().unwrap_or_default());
                }
                watch_scan |= watch.due();
            }
            if watch_scan {
                self.rescan();
            }
            self.jobs.retain(|job| !job.is_finished());
            let started = Instant::now();
            let tick = self.player.tick();
            self.apply_tick(tick);
            if self.audio_save_at.is_some_and(|at| Instant::now() >= at) {
                let _ = self.flush_audio();
            }
            self.publish();

            if last_flush.elapsed() >= SESSION_FLUSH_INTERVAL {
                last_flush = Instant::now();
                self.flush_session();
            }

            let interval = if self.player.is_crossfading() {
                CROSSFADE_POLL
            } else {
                POSITION_POLL
            };
            let elapsed = started.elapsed();
            if elapsed < interval {
                thread::sleep(interval - elapsed);
            }
        }
    }

    /// The explicit shutdown path: GStreamer elements back to `NULL` (which
    /// is what releases device and codec resources), the session on disk, and
    /// the thread returning.
    fn finish(&mut self) {
        self.shared
            .spectrum_enabled
            .store(false, std::sync::atomic::Ordering::Release);
        self.player.set_spectrum_sink(None);
        if self.current_path.is_some() {
            let status = self.player.status();
            self.position = status.position_secs;
            self.remember_progress();
        }
        let _ = self.player.stop();
        for job in self.jobs.drain(..) {
            let _ = job.join();
        }
        self.playing = false;
        self.position = 0.0;
        self.armed = None;
        self.current_path = None;
        if let Err(error) = self.flush_audio() {
            eprintln!("[muzon audio settings] {error}");
        }
        self.flush_session();
        self.publish();
    }

    fn flush_session(&self) {
        if let Err(e) = self.session.flush() {
            eprintln!("[muzon core] failed to save session: {e}");
        }
    }

    /// Returns `false` when the worker should stop.
    fn handle(&mut self, command: Command) -> bool {
        match command {
            Command::LoadEditor { id, reply } => {
                let db = self.config.db.clone();
                let lock = self.file_lock.clone();
                self.jobs.push(thread::spawn(move || {
                    let result = lock.lock().map_err(|e| e.to_string()).and_then(|_guard| {
                        crate::editing::load(&db, id).map_err(|e| e.to_string())
                    });
                    let _ = reply.send(result);
                }));
            }
            Command::SaveEdit { id, edit, reply } => {
                if self.edit_busy {
                    let _ = reply.send(Err("Другая запись ещё выполняется".into()));
                    return true;
                }
                self.edit_busy = true;
                let db = self.config.db.clone();
                let lock = self.file_lock.clone();
                let tx = self.tx.clone();
                self.jobs.push(thread::spawn(move || {
                    let result = lock.lock().map_err(|e| e.to_string()).and_then(|_guard| {
                        crate::editing::save(&db, id, edit).map_err(|e| e.to_string())
                    });
                    let _ = tx.send(Command::EditFinished { result, reply });
                }));
            }
            Command::EditFinished { result, reply } => {
                self.edit_busy = false;
                if let Ok(track) = &result {
                    self.load_library();
                    // Fetch fresh metadata so a concurrent favorite/tempo change is preserved.
                    let updated = self
                        .library_by_id
                        .get(&track.id)
                        .and_then(|&i| self.library.get(i))
                        .cloned()
                        .unwrap_or_else(|| track.clone());
                    for entry in &mut self.shape.queue {
                        if entry.id == track.id {
                            *entry = updated.clone();
                        }
                    }
                    self.queue = Arc::new(self.shape.queue.clone());
                    if self.current_path.as_ref() == Some(&track.path) {
                        self.media_requested = None;
                        *self.shared.media.write().unwrap() = Arc::new(Media::default());
                        self.request_media(track.path.clone());
                    }
                    self.publish();
                }
                let _ = reply.send(result.map(|_| ()));
            }
            Command::Shutdown => return false,
            Command::PlaybackEdit(edit) => {
                if let Err(error) = self.edit_playback(edit) {
                    self.audio_error = Some(error);
                }
            }
            Command::TrackTempo { id, rate } => {
                if let Err(error) = self.preview_track_tempo(id, rate) {
                    self.audio_error = Some(error);
                }
            }
            Command::SaveAudio(reply) => {
                let result = self.flush_audio();
                self.publish();
                let _ = reply.send(result);
            }
            Command::PlaylistEdit { edit, reply } => {
                let result = self.edit_playlist(edit);
                self.publish();
                let _ = reply.send(result);
            }
            Command::Barrier(reply) => {
                self.publish();
                let _ = reply.send(Arc::clone(&self.shared.snapshot.read().unwrap()));
            }
            Command::SetRepeat(repeat) => self.set_repeat(repeat),
            Command::SetTempo(rate) => {
                if rate.is_finite() {
                    if let Some(path) = &self.current_path {
                        self.player.apply_tempo_for_path(path, rate);
                    }
                }
            }
            Command::SeekFor {
                path,
                cursor,
                position,
            } => {
                if self.current_path.as_ref() == Some(&path) && self.shape.cursor == cursor {
                    self.seek(position);
                }
            }
            Command::Favorite { id, reply } => {
                let result = id
                    .or_else(|| {
                        self.current_path.as_ref()?;
                        self.shape
                            .cursor
                            .and_then(|i| self.shape.queue.get(i))
                            .map(|t| t.id)
                    })
                    .ok_or_else(|| "nothing is playing".to_owned())
                    .and_then(|id| {
                        self.config
                            .db
                            .toggle_favorite(id)
                            .map(|on| (id, on))
                            .map_err(|e| e.to_string())
                    });
                if let Ok((id, on)) = result {
                    self.load_library();
                    for track in &mut self.shape.queue {
                        if track.id == id {
                            track.is_favorite = on;
                        }
                    }
                    self.queue = Arc::new(self.shape.queue.clone());
                    self.publish();
                }
                let _ = reply.send(result);
            }
            Command::AddFolder(path) => {
                if let Err(e) = self.config.db.add_root(&path) {
                    self.error = Some(format!("{}: {e}", path.display()));
                } else {
                    if let Some(watch) = &mut self.watch {
                        watch.sync(&self.config.db.list_roots().unwrap_or_default());
                    }
                    self.rescan();
                }
            }
            Command::RefreshLibrary => {
                self.load_library();
                if let Err(error) = self.load_playlists() {
                    self.error = Some(error);
                }
                for entry in &mut self.shape.queue {
                    if let Some(updated) = self
                        .library_by_id
                        .get(&entry.id)
                        .and_then(|&i| self.library.get(i))
                    {
                        *entry = updated.clone();
                    }
                }
                self.queue = Arc::new(self.shape.queue.clone());
                Arc::make_mut(&mut self.tempo_overrides)
                    .retain(|id, _| self.pending_tempos.contains_key(id));
                if let Some(path) = self.current_path.clone() {
                    self.apply_tempo(&path);
                }
            }
            Command::Rescan => self.rescan(),
            Command::ScanFinished(result) => self.on_scan_finished(result),
            Command::SetQueue { ids, start_id } => self.set_queue(&ids, start_id),
            Command::PlayIndex(index) => self.start_at(index),
            Command::Spectrum(on) => {
                use std::sync::atomic::Ordering;
                let old = self.shared.spectrum_enabled.swap(on, Ordering::AcqRel);
                if old != on {
                    if on {
                        let shared = Arc::downgrade(&self.shared);
                        self.player
                            .set_spectrum_sink(Some(Arc::new(move |mut bars| {
                                if let Some(shared) = shared.upgrade() {
                                    if shared.spectrum_enabled.load(Ordering::Acquire) {
                                        bars.truncate(crate::data::audio::spectrum::BARS);
                                        for bar in &mut bars {
                                            *bar = if bar.is_finite() {
                                                bar.clamp(0., 1.)
                                            } else {
                                                0.
                                            };
                                        }
                                        *shared.spectrum.write().unwrap() = Arc::new(bars);
                                    }
                                }
                            })));
                    } else {
                        self.player.set_spectrum_sink(None);
                        *self.shared.spectrum.write().unwrap() = Arc::new(Vec::new());
                    }
                }
            }
            Command::SetSleep(choice) => {
                self.sleep.set(choice, Instant::now());
                self.arm_next();
            }
            Command::TogglePlay => self.toggle_play(),
            Command::Play => self.play_current(),
            Command::Pause => {
                if self.player.pause().is_ok() {
                    self.playing = false;
                }
            }
            Command::Stop => self.stop_playback(),
            Command::Seek(position) => self.seek(position),
            Command::SetVolume(volume) => self.set_volume(volume),
            Command::Next => self.advance(),
            Command::Previous => self.rewind(),
            Command::SetShuffle(shuffle) => self.set_shuffle(shuffle),
            Command::ToggleShuffle => self.set_shuffle(!self.shuffle),
            Command::CycleRepeat => self.set_repeat(match self.repeat {
                RepeatMode::Off => RepeatMode::All,
                RepeatMode::All => RepeatMode::One,
                RepeatMode::One => RepeatMode::Off,
            }),
            Command::Enqueue(ids) => self.enqueue(&ids),
            Command::MoveQueue { from, to } => {
                let result = muzon_queue::move_in_view(self.take_shape(), self.shuffle, from, to);
                self.shape = result.shape;
                if result.changed {
                    self.preserve_session = false;
                    self.sync_session_queue();
                    self.refresh_display();
                    self.arm_next();
                }
            }
            Command::Remove(index) => self.remove(index),
            Command::RequestMedia(path) => self.request_media(path),
            Command::MediaReady {
                generation,
                path,
                cover,
                lyrics,
            } => self.on_media_ready(generation, path, cover, lyrics),
        }
        true
    }

    // --- playback ----------------------------------------------------------

    fn apply_tick(&mut self, tick: PlaybackTick) {
        self.playing = tick.is_playing;
        self.position = tick.position_secs;
        if tick.duration_secs > 0.0 {
            self.duration = tick.duration_secs;
        }
        if tick.path.is_some() {
            self.current_path = tick.path.clone();
        }
        if tick.is_playing && tick.position_secs > 0.25 {
            // A track that actually reaches audio clears the failure streak:
            // the guard exists to stop a run of broken files, not to remember
            // one from an hour ago.
            self.consecutive_failures = 0;
        }
        if let Some(error) = tick.error {
            let fatal = error.fatal;
            self.on_playback_error(error);
            if fatal {
                self.remember_progress();
                return;
            }
        }
        if tick.is_playing
            && tick.auto_advanced_to.is_none()
            && !self.listen_counted
            && crate::listens::should_count(tick.position_secs, tick.duration_secs)
        {
            if let Some(id) = self
                .shape
                .cursor
                .and_then(|i| self.shape.queue.get(i))
                .map(|t| t.id)
            {
                let started = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.)
                    - tick.position_secs / self.player.current_tempo().max(0.01);
                let listen = self
                    .shape
                    .cursor
                    .and_then(|i| self.shape.queue.get(i))
                    .and_then(|track| {
                        crate::domain::listen::Listen::from_track(track, started.round() as i64)
                    });
                match self.config.db.record_play_and_queue(id, listen.as_ref()) {
                    Ok(()) => {
                        self.listen_counted = true;
                        self.load_library();
                        if let Some(updated) = self
                            .library_by_id
                            .get(&id)
                            .and_then(|&i| self.library.get(i))
                            .cloned()
                        {
                            for track in &mut self.shape.queue {
                                if track.id == id {
                                    *track = updated.clone();
                                }
                            }
                            self.queue = Arc::new(self.shape.queue.clone());
                        }
                    }
                    Err(error) => {
                        self.listen_counted = true;
                        self.message = Some(format!("Не удалось записать прослушивание: {error}"));
                    }
                }
            }
        }
        let sleep_due = self.sleep.fires(
            Instant::now(),
            tick.ended || tick.auto_advanced_to.is_some(),
        );
        if sleep_due {
            // An already-running crossfade can finish after arming was removed.
            // Synchronize its cursor, then pause immediately instead of advancing again.
            if let Some(path) = tick.auto_advanced_to.as_deref() {
                self.on_auto_advanced(path);
            }
            let _ = self.player.pause();
            self.playing = false;
            self.arm_next();
            self.remember_progress();
            return;
        }
        // The two pulses are mutually exclusive in the engine: `ended` is only
        // reported when nothing was armed.
        if let Some(path) = tick.auto_advanced_to {
            self.on_auto_advanced(&path);
        }
        if tick.ended {
            self.advance();
        }
        self.remember_progress();
    }

    fn on_playback_error(&mut self, error: PlaybackError) {
        if !error.fatal {
            // Only the pre-roll died; the current track keeps playing and the
            // engine has already dropped the broken standby deck.
            self.message = Some(format!(
                "pre-load failed{}: {}",
                error
                    .path
                    .as_deref()
                    .map(|p| format!(" ({p})"))
                    .unwrap_or_default(),
                error.message
            ));
            return;
        }
        self.register_failure(format!(
            "{}: {}",
            error.path.as_deref().unwrap_or("playback"),
            error.message
        ));
    }

    /// A file that could not be loaded or decoded. Skips on, but not forever.
    fn register_failure(&mut self, message: String) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
            self.consecutive_failures = 0;
            let _ = self.player.stop();
            self.playing = false;
            self.position = 0.0;
            self.armed = None;
            self.current_path = None;
            self.error = Some(format!(
                "playback stopped after {MAX_CONSECUTIVE_FAILURES} failed tracks — {message}"
            ));
        } else {
            self.error = Some(message);
            self.advance();
        }
    }

    /// Commits a gapless/crossfade hand-off that already happened in the
    /// pipeline. The cursor moves; playback is not restarted.
    fn on_auto_advanced(&mut self, path: &str) {
        self.listen_counted = false;
        let index = self
            .armed
            .filter(|&index| {
                self.shape
                    .queue
                    .get(index)
                    .is_some_and(|track| track.path == path)
            })
            .or_else(|| self.shape.queue.iter().position(|track| track.path == path));
        if let Some(index) = index {
            self.shape.cursor = Some(index);
            self.position = 0.0;
            self.current_path = Some(path.to_string());
            self.refresh_display();
            self.remember_progress();
            // Re-arm for whatever follows this one — the controller, not the
            // UI, owns that.
            self.arm_next();
        }
    }

    fn advance(&mut self) {
        let navigation = next_action(&self.shape, self.shuffle, self.repeat);
        self.execute(navigation);
    }

    fn rewind(&mut self) {
        let navigation = previous_action(&self.shape, self.shuffle);
        self.execute(navigation);
    }

    fn execute(&mut self, navigation: Navigation) {
        match navigation {
            Navigation::Play(index) => self.start_at(index),
            Navigation::Stop => self.stop_playback(),
            Navigation::Noop => {}
        }
    }

    /// Loads and plays the queue element at `index`.
    fn start_at(&mut self, index: usize) {
        let Some(track) = self.shape.queue.get(index).cloned() else {
            return;
        };
        self.listen_counted = false;
        self.shape.cursor = Some(index);
        // `display` is the active order rotated around the cursor; moving the
        // cursor is what re-rotates it.
        self.refresh_display();
        match self.player.load_and_play(&track.path) {
            Ok(()) => {
                self.apply_tempo(&track.path);
                self.error = None;
                self.position = 0.0;
                self.duration = track.duration_secs.unwrap_or(0.0);
                self.current_path = Some(track.path.clone());
                self.playing = true;
                self.remember_progress();
                self.arm_next();
            }
            Err(e) => {
                self.error = Some(format!("{}: {e}", track.path));
                self.register_failure(format!("{}: {e}", track.path));
            }
        }
    }

    fn toggle_play(&mut self) {
        if self.shape.cursor.is_none() {
            // Nothing chosen yet: "play" starts the active order's first item.
            self.execute(next_action(&self.shape, self.shuffle, self.repeat));
            return;
        }
        if self.current_path.is_none() {
            // Stopped, so the pipeline holds nothing: reload the cursor's
            // track rather than pressing play on an empty deck.
            if let Some(index) = self.shape.cursor {
                self.start_at(index);
            }
            return;
        }
        if self.playing {
            if self.player.pause().is_ok() {
                self.playing = false;
            }
        } else {
            self.play_current();
        }
    }

    fn play_current(&mut self) {
        if self.current_path.is_none() {
            if let Some(index) = self.shape.cursor {
                self.start_at(index);
            } else {
                self.advance();
            }
        } else if self.player.play().is_ok() {
            self.playing = true;
        }
    }

    fn stop_playback(&mut self) {
        let _ = self.player.stop();
        self.playing = false;
        self.position = 0.0;
        self.armed = None;
        self.current_path = None;
        self.remember_progress();
    }

    fn seek(&mut self, position_secs: f64) {
        if !position_secs.is_finite() || self.current_path.is_none() {
            return;
        }
        let position = position_secs.max(0.0);
        if self.player.seek(position).is_ok() {
            self.position = position;
            self.seek_revision = self.seek_revision.wrapping_add(1);
            self.seek_position_secs = position;
            self.remember_progress();
        }
    }

    fn set_volume(&mut self, volume: f64) {
        let volume = volume.clamp(0.0, 1.0);
        self.player.set_volume(volume).ok();
        self.volume = volume;
        self.remember_progress();
    }

    /// Tells the engine what to pre-roll. Only the worker calls this, so the
    /// gapless/crossfade machinery never races the UI.
    fn arm_next(&mut self) {
        if self.sleep.waits_for_end() {
            self.armed = None;
            self.player.set_next_track(None);
            return;
        }
        self.armed = peek_next_index(&self.shape, self.shuffle, self.repeat);
        let path = self
            .armed
            .and_then(|index| self.shape.queue.get(index))
            .map(|track| track.path.clone());
        // The legacy pipeline detects a gapless join by a URI change. The
        // same URI cannot identify a new queue occurrence; let EOS advance
        // such a pair so repeat-one and duplicate entries remain correct.
        let path = path.filter(|p| Some(p.as_str()) != self.current_path.as_deref());
        self.player.set_next_track(path);
        if let Some(index) = self.armed {
            if let Some(track) = self.shape.queue.get(index) {
                if self.current_path.as_deref() != Some(track.path.as_str()) {
                    self.apply_tempo(&track.path);
                }
            }
        }
    }

    fn apply_tempo(&self, path: &str) {
        let tempo = self
            .library
            .iter()
            .find(|t| t.path == path)
            .and_then(|t| self.tempo_overrides.get(&t.id).copied())
            .unwrap_or_else(|| {
                self.config
                    .db
                    .track_tempo_by_path(path)
                    .ok()
                    .flatten()
                    .unwrap_or(1.0)
            });
        self.player.apply_tempo_for_path(path, tempo);
    }

    fn audio_edited(&mut self) {
        self.audio_error = None;
        self.audio_save_at = Some(Instant::now() + Duration::from_millis(300));
    }
    fn edit_playback(&mut self, edit: PlaybackEdit) -> Result<(), String> {
        let next = edit.apply(&self.playback)?;
        if *self.playback == next {
            return Ok(());
        }
        // Apply only the affected properties; changing EQ must not re-arm a
        // transition or reset a deck's independently saved playback speed.
        match edit {
            PlaybackEdit::EqBand { .. } | PlaybackEdit::Equalizer(_) => {
                self.player.set_equalizer_bands(next.eq_gains)
            }
            PlaybackEdit::Crossfade(_) => {
                self.player.set_crossfade_seconds(next.crossfade_secs);
                self.arm_next();
            }
            PlaybackEdit::ReplayGainMode(_) | PlaybackEdit::Preamp(_) => {
                self.player.set_replay_gain(next.replay_gain)
            }
            PlaybackEdit::Reset => {
                self.player.set_equalizer_bands(next.eq_gains);
                self.player.set_crossfade_seconds(next.crossfade_secs);
                self.player.set_replay_gain(next.replay_gain);
                self.arm_next();
            }
        }
        self.playback = Arc::new(next);
        self.playback_dirty = true;
        self.audio_edited();
        Ok(())
    }
    fn preview_track_tempo(&mut self, id: i32, rate: f64) -> Result<(), String> {
        if !rate.is_finite() {
            return Err("Скорость должна быть конечным числом".into());
        }
        let rate = rate.clamp(MIN_TEMPO, MAX_TEMPO);
        let track = self
            .library_by_id
            .get(&id)
            .and_then(|&i| self.library.get(i))
            .ok_or("Трек больше не существует")?;
        self.player.apply_tempo_for_path(&track.path, rate);
        Arc::make_mut(&mut self.tempo_overrides).insert(id, rate);
        self.pending_tempos.insert(id, rate);
        self.audio_edited();
        Ok(())
    }
    fn flush_audio(&mut self) -> Result<(), String> {
        self.audio_save_at = None;
        let result = (|| -> Result<(), String> {
            if self.playback_dirty {
                self.config
                    .playback_settings_store
                    .save(&self.playback)
                    .map_err(|e| e.to_string())?;
                self.playback_dirty = false;
            }
            if !self.pending_tempos.is_empty() {
                self.config
                    .db
                    .set_track_tempos(&self.pending_tempos)
                    .map_err(|e| e.to_string())?;
                self.pending_tempos.clear();
            }
            Ok(())
        })();
        self.audio_error = result.as_ref().err().cloned();
        result
    }

    fn take_shape(&mut self) -> QueueShape<Track> {
        std::mem::replace(&mut self.shape, QueueShape::new(Vec::new()))
    }

    fn set_queue(&mut self, ids: &[i32], start_id: i32) {
        let tracks: Vec<Track> = ids
            .iter()
            .filter_map(|id| self.library_by_id.get(id))
            .filter_map(|&index| self.library.get(index).cloned())
            .collect();
        if tracks.is_empty() {
            return;
        }
        // A genuinely new playback context draws a new permutation; toggling
        // shuffle later reuses it as-is — exactly `setQueue` in the original.
        let shuffle_order = shuffle_permutation(tracks.len(), &mut self.rng);
        let start = tracks
            .iter()
            .position(|track| track.id == start_id)
            .unwrap_or(0);
        self.shape = QueueShape {
            queue: tracks,
            shuffle_order,
            cursor: None,
        };
        self.preserve_session = false;
        self.sync_session_queue();
        self.refresh_display();
        self.start_at(start);
    }

    fn enqueue(&mut self, ids: &[i32]) {
        let was_empty = self.shape.queue.is_empty();
        let tracks: Vec<Track> = ids
            .iter()
            .filter_map(|id| self.library_by_id.get(id))
            .filter_map(|&index| self.library.get(index).cloned())
            .collect();
        if tracks.is_empty() {
            return;
        }
        self.shape = append_tracks(self.take_shape(), tracks);
        self.preserve_session = false;
        self.sync_session_queue();
        self.refresh_display();
        if was_empty && self.shape.cursor.is_none() {
            self.start_at(0);
        } else {
            self.arm_next();
        }
    }

    fn remove(&mut self, index: usize) {
        let result = remove_at(self.take_shape(), index, self.shuffle);
        if !result.removed {
            self.shape = result.shape;
            return;
        }
        self.shape = result.shape;
        self.preserve_session = false;
        self.sync_session_queue();
        self.refresh_display();
        if result.was_current {
            match result.next_cursor {
                Some(next) => self.start_at(next),
                None => self.stop_playback(),
            }
        } else {
            self.arm_next();
        }
    }

    fn set_shuffle(&mut self, shuffle: bool) {
        self.shuffle = shuffle;
        self.refresh_display();
        self.arm_next();
        self.remember_progress();
    }

    fn set_repeat(&mut self, repeat: RepeatMode) {
        self.repeat = repeat;
        self.arm_next();
        self.remember_progress();
    }

    fn refresh_display(&mut self) {
        // Queue contents and display order are pure functions of the shape;
        // both are cached so a tick shares them rather than rebuilding them.
        self.queue = Arc::new(self.shape.queue.clone());
        self.display = Arc::new(display_order_indices(&self.shape, self.shuffle));
        self.order = Arc::new(play_order_indices(&self.shape, self.shuffle));
    }

    fn sync_session_queue(&mut self) {
        let queue_paths: Vec<String> = self
            .shape
            .queue
            .iter()
            .map(|track| track.path.clone())
            .collect();
        let shuffle_order: Vec<u32> = self
            .shape
            .shuffle_order
            .iter()
            .map(|&index| index as u32)
            .collect();
        self.session.update(|session| {
            session.queue_paths = queue_paths;
            session.shuffle_order = shuffle_order;
        });
    }

    fn remember_progress(&mut self) {
        if self.preserve_session {
            return;
        }
        let cursor = self.shape.cursor.map(|index| index as i32).unwrap_or(-1);
        let position = self.position;
        let volume = self.volume;
        let shuffle = self.shuffle;
        let repeat = self.repeat;
        self.session.update(|session| {
            session.cursor = cursor;
            session.position_secs = position;
            session.volume = volume;
            session.shuffle = shuffle;
            session.repeat = repeat_to_session(repeat);
        });
    }

    // --- library -----------------------------------------------------------

    fn load_playlists(&mut self) -> Result<(), String> {
        self.playlists = Arc::new(
            self.config
                .db
                .playlist_snapshot()
                .map_err(|e| e.to_string())?,
        );
        Ok(())
    }
    fn edit_playlist(&mut self, edit: PlaylistEdit) -> Result<Option<i32>, String> {
        fn name(name: String) -> Result<String, String> {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err("Название плейлиста должно содержать от 1 до 120 символов".into());
            }
            Ok(name.to_owned())
        }
        let mut created = None;
        let id = match &edit {
            PlaylistEdit::Create { .. } => None,
            PlaylistEdit::Rename { id, .. }
            | PlaylistEdit::Delete { id }
            | PlaylistEdit::Add { id, .. }
            | PlaylistEdit::Remove { id, .. }
            | PlaylistEdit::Move { id, .. } => Some(*id),
        };
        if let Some(id) = id {
            if !self.playlists.iter().any(|p| p.id == id) {
                return Err("Плейлист больше не существует".into());
            }
        }
        let tracks = match &edit {
            PlaylistEdit::Create { tracks, .. } | PlaylistEdit::Add { tracks, .. } => {
                tracks.as_slice()
            }
            _ => &[],
        };
        if tracks.iter().any(|id| !self.library_by_id.contains_key(id)) {
            return Err("Один из треков больше не существует в библиотеке".into());
        }
        let result = match edit {
            PlaylistEdit::Create {
                name: label,
                tracks,
            } => self
                .config
                .db
                .create_playlist_with_tracks(&name(label)?, &tracks)
                .map(|id| {
                    created = Some(id);
                }),
            PlaylistEdit::Rename { id, name: label } => {
                self.config.db.rename_playlist(id, &name(label)?)
            }
            PlaylistEdit::Delete { id } => self.config.db.delete_playlist(id),
            PlaylistEdit::Add { id, tracks } => self.config.db.append_playlist_tracks(id, &tracks),
            PlaylistEdit::Remove { id, track } => {
                self.config.db.remove_track_from_playlist(id, track)
            }
            PlaylistEdit::Move { id, track, offset } => {
                let mut ids = self
                    .playlists
                    .iter()
                    .find(|p| p.id == id)
                    .unwrap()
                    .track_ids
                    .clone();
                let from = ids
                    .iter()
                    .position(|&i| i == track)
                    .ok_or("Трек больше не входит в плейлист")?;
                let to = from
                    .saturating_add_signed(offset as isize)
                    .min(ids.len() - 1);
                let track = ids.remove(from);
                ids.insert(to, track);
                self.config.db.reorder_playlist_tracks(id, &ids)
            }
        };
        result.map_err(|e| e.to_string())?;
        self.load_playlists()?;
        Ok(created)
    }

    fn load_library(&mut self) {
        match self.config.db.list_tracks() {
            Ok(tracks) => {
                self.library = Arc::new(tracks);
                self.library_by_id = self
                    .library
                    .iter()
                    .enumerate()
                    .map(|(index, track)| (track.id, index))
                    .collect();
                Arc::make_mut(&mut self.tempo_overrides)
                    .retain(|id, _| self.library_by_id.contains_key(id));
                self.pending_tempos
                    .retain(|id, _| self.library_by_id.contains_key(id));
            }
            Err(e) => self.error = Some(format!("library: {e}")),
        }
    }

    fn rescan(&mut self) {
        if self.scanning {
            self.rescan_pending = true;
            return;
        }
        self.scanning = true;
        self.message = Some("Scanning…".to_string());
        let db = Arc::clone(&self.config.db);
        let tx = self.tx.clone();
        let lock = self.file_lock.clone();
        self.jobs.push(thread::spawn(move || {
            let _guard = lock.lock().unwrap();
            let result = db
                .list_roots()
                .map_err(|e| e.to_string())
                .and_then(|roots| scanner::scan_roots(&db, &roots).map_err(|e| e.to_string()));
            let _ = tx.send(Command::ScanFinished(result));
        }));
    }

    fn on_scan_finished(&mut self, result: Result<ScanReport, String>) {
        self.scanning = false;
        if let Some(path) = self.current_path.clone() {
            covers::invalidate_thumbnail(&self.config.cover_dir, std::path::Path::new(&path));
            self.media_requested = None;
            *self.shared.media.write().unwrap() = Arc::new(Media::default());
            self.request_media(path);
        }
        match result {
            Ok(report) => {
                self.load_library();
                for entry in &mut self.shape.queue {
                    if let Some(updated) = self
                        .library_by_id
                        .get(&entry.id)
                        .and_then(|&i| self.library.get(i))
                    {
                        *entry = updated.clone();
                    }
                }
                self.queue = Arc::new(self.shape.queue.clone());

                if let Err(error) = self.load_playlists() {
                    self.error = Some(error);
                }
                let mut parts = Vec::new();
                if report.added > 0 {
                    parts.push(format!("{} added", report.added));
                }
                if report.updated > 0 {
                    parts.push(format!("{} updated", report.updated));
                }
                if report.moved > 0 {
                    parts.push(format!("{} moved", report.moved));
                }
                if report.removed > 0 {
                    parts.push(format!("{} removed", report.removed));
                }
                if parts.is_empty() {
                    parts.push("nothing new".to_string());
                }
                if report.errors.is_empty() {
                    self.message = Some(format!("Scanned: {}", parts.join(", ")));
                } else {
                    self.message = Some(format!(
                        "Scanned: {} — {} problem(s)",
                        parts.join(", "),
                        report.errors.len()
                    ));
                }
            }
            Err(e) => {
                self.error = Some(format!("scan failed: {e}"));
                self.message = None;
            }
        }
        if self.rescan_pending {
            self.rescan_pending = false;
            self.rescan();
        }
    }

    // --- session -----------------------------------------------------------

    fn restore(&mut self) {
        let session = self.session.snapshot();
        self.shuffle = session.shuffle;
        self.repeat = repeat_from_session(session.repeat);
        self.volume = session.volume.clamp(0.0, 1.0);
        let _ = self.player.set_volume(self.volume);

        let settings = self
            .config
            .playback_settings_store
            .load_or_default()
            .normalized();
        self.playback = Arc::new(settings.clone());
        self.player.set_crossfade_seconds(settings.crossfade_secs);
        self.player.set_equalizer_bands(settings.eq_gains);
        self.player.set_replay_gain(settings.replay_gain);

        let library = Arc::clone(&self.library);
        let plan = {
            let library_paths: Vec<&str> = library.iter().map(|t| t.path.as_str()).collect();
            let saved_paths: Vec<&str> = session.queue_paths.iter().map(String::as_str).collect();
            let mut rng = Rng::new();
            plan_restore(
                &saved_paths,
                &session.shuffle_order,
                session.cursor,
                &library_paths,
                |n| shuffle_permutation(n, &mut rng),
            )
        };

        match plan {
            Ok(Some(plan)) => {
                let queue: Vec<Track> = plan
                    .shape
                    .queue
                    .iter()
                    .filter_map(|&index| library.get(index).cloned())
                    .collect();
                self.shape = QueueShape {
                    queue,
                    shuffle_order: plan.shape.shuffle_order,
                    cursor: plan.shape.cursor,
                };
                // Persist the same reindexed shape whose cursor we publish.
                // Keeping old paths with a new cursor would resume on the
                // wrong occurrence on the next launch after a file vanished.
                self.sync_session_queue();
                if let Some(index) = self.shape.cursor {
                    if let Some(track) = self.shape.queue.get(index).cloned() {
                        // Paused, at the saved position: reopening the app
                        // must not start blaring.
                        let _ = self
                            .player
                            .load_paused_at(&track.path, session.position_secs);
                        self.apply_tempo(&track.path);
                        self.current_path = Some(track.path.clone());
                        self.position = session.position_secs;
                        self.duration = track.duration_secs.unwrap_or(0.0);
                        self.arm_next();
                    }
                }
                self.playing = false;
            }
            Ok(None) => {
                // Nothing of the saved queue is in this library. Keep the file
                // as it is instead of flushing an empty queue over it.
                self.preserve_session = !session.queue_paths.is_empty();
            }
            Err(e) => {
                self.preserve_session = !session.queue_paths.is_empty();
                self.error = Some(format!("session: {e:?}"));
            }
        }
        self.refresh_display();
    }

    // --- media -------------------------------------------------------------

    fn request_media(&mut self, path: String) {
        let current = self.shared.media.read().unwrap().clone();
        if current.track_path.as_deref() == Some(path.as_str()) && current.loaded {
            return;
        }
        if self.media_requested.as_deref() == Some(path.as_str()) {
            return;
        }
        self.media_requested = Some(path.clone());
        self.media_generation = self.media_generation.wrapping_add(1);
        let generation = self.media_generation;
        let lock = self.file_lock.clone();
        let tx = self.tx.clone();
        let cover_dir = self.config.cover_dir.clone();
        self.jobs.push(thread::spawn(move || {
            let _guard = lock.lock().unwrap();
            let track_path = PathBuf::from(&path);
            let cover = covers::read_cover_thumbnail(&cover_dir, &track_path);
            let lyrics = lrc_source::load_for_track(&track_path).map(|lyrics| lyrics.lines);
            let _ = tx.send(Command::MediaReady {
                generation,
                path,
                cover,
                lyrics,
            });
        }));
    }

    fn on_media_ready(
        &mut self,
        generation: u64,
        path: String,
        cover: Option<(String, Vec<u8>)>,
        lyrics: Option<Vec<LrcLine>>,
    ) {
        if generation != self.media_generation
            || self.media_requested.as_deref() != Some(path.as_str())
        {
            // The track changed while the decoder was working; its answer is
            // for a track nobody is looking at any more.
            return;
        }
        self.media_requested = None;
        let media = Media {
            track_path: Some(path),
            cover: cover.map(|(_, bytes)| Arc::new(bytes)),
            lyrics: lyrics.unwrap_or_default(),
            loaded: true,
        };
        *self.shared.media.write().unwrap() = Arc::new(media);
    }

    // --- publishing --------------------------------------------------------

    fn publish(&mut self) {
        self.revision += 1;
        let snapshot = Snapshot {
            revision: self.revision,
            seek_revision: self.seek_revision,
            seek_position_secs: self.seek_position_secs,
            tempo: self.player.current_tempo(),
            library: Arc::clone(&self.library),
            playlists: Arc::clone(&self.playlists),
            playback: Arc::clone(&self.playback),
            tempo_overrides: Arc::clone(&self.tempo_overrides),
            audio_dirty: self.playback_dirty || !self.pending_tempos.is_empty(),
            audio_error: self.audio_error.clone(),
            queue: Arc::clone(&self.queue),
            display: Arc::clone(&self.display),
            order: Arc::clone(&self.order),
            cursor: self.shape.cursor,
            shuffle: self.shuffle,
            repeat: self.repeat,
            is_playing: self.playing,
            sleep: self.sleep.state(Instant::now()),
            position_secs: self.position,
            duration_secs: self.duration,
            current_path: self.current_path.clone(),
            volume: self.volume,
            scanning: self.scanning,
            message: self.message.clone(),
            error: self.error.clone(),
        };
        *self.shared.snapshot.write().unwrap() = Arc::new(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::db::Db;
    use crate::testing::{gst_lock, scratch_dir, write_tone};
    use std::path::Path;

    // --- fixtures ----------------------------------------------------------

    /// A profile directory and the library inside it, without the installed
    /// app's data: every test gets its own temp dir.
    fn make_profile(name: &str) -> PathBuf {
        let dir = scratch_dir(name);
        std::fs::create_dir_all(dir.join("config")).unwrap();
        dir
    }

    /// Writes the tones and scans them into the profile's database.
    fn scan_into(dir: &Path, tones: &[(&str, f64)]) {
        let music = dir.join("music");
        for (name, seconds) in tones {
            write_tone(&music, name, *seconds);
        }
        let db = Db::open(&dir.join("muzon.sqlite3")).unwrap();
        db.add_root(&music).unwrap();
        let roots = db.list_roots().unwrap();
        let report = scanner::scan_roots(&db, &roots).unwrap();
        assert!(report.errors.is_empty(), "scan errors: {:?}", report.errors);
        assert_eq!(report.added as usize, tones.len());
    }

    fn config_for(dir: &Path) -> CoreConfig {
        CoreConfig {
            db: Arc::new(Db::open(&dir.join("muzon.sqlite3")).unwrap()),
            session_store: SessionStore::new(&dir.join("config")),
            playback_settings_store: PlaybackSettingsStore::new(&dir.join("config")),
            cover_dir: dir.join("covers"),
        }
    }

    /// Spawns the worker with fake sinks, or returns `None` when the machine
    /// has no usable GStreamer (the same skip the pipeline tests use).
    fn spawn(dir: &Path) -> Option<Controller> {
        Some(
            Controller::spawn_test(config_for(dir)).expect("GStreamer test worker must initialize"),
        )
    }

    fn id_of(snapshot: &Snapshot, name: &str) -> i32 {
        snapshot
            .library
            .iter()
            .find(|track| track.path.ends_with(name))
            .unwrap_or_else(|| panic!("no track named {name} in the library"))
            .id
    }

    fn paths(snapshot: &Snapshot) -> Vec<String> {
        snapshot
            .queue
            .iter()
            .map(|track| track.path.clone())
            .collect()
    }

    fn ends_with(snapshot: &Snapshot, name: &str) -> bool {
        snapshot
            .current_path
            .as_deref()
            .is_some_and(|path| path.ends_with(name))
    }

    fn wait_for<F>(controller: &Controller, seconds: f64, what: &str, mut done: F) -> Arc<Snapshot>
    where
        F: FnMut(&Snapshot) -> bool,
    {
        let deadline = Instant::now() + Duration::from_secs_f64(seconds);
        loop {
            let snapshot = controller.snapshot();
            if done(&snapshot) {
                return snapshot;
            }
            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for {what}: playing={} cursor={:?} path={:?} \
                     queue={} pos={:.2} scanning={} error={:?}",
                    snapshot.is_playing,
                    snapshot.cursor,
                    snapshot.current_path,
                    snapshot.queue.len(),
                    snapshot.position_secs,
                    snapshot.scanning,
                    snapshot.error,
                );
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    // --- tests -------------------------------------------------------------

    #[test]
    fn spectrum_keeps_only_latest_frame_and_can_be_disabled() {
        let _gst = gst_lock();
        let dir = make_profile("native-spectrum");
        scan_into(&dir, &[("tone.wav", 5.)]);
        let mut core = spawn(&dir).unwrap();
        let id = core.snapshot().library[0].id;
        core.set_spectrum(true);
        core.set_queue(vec![id], id);
        let deadline = Instant::now() + Duration::from_secs(4);
        while !core.spectrum().iter().any(|v| *v > 0.) {
            assert!(Instant::now() < deadline, "spectrum frames missing");
            thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(core.spectrum().len(), crate::data::audio::spectrum::BARS);
        core.set_spectrum(false);
        core.barrier().unwrap();
        thread::sleep(Duration::from_millis(100));
        assert!(core.spectrum().is_empty());
        core.shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn listens_count_once_per_occurrence_and_queue_moves_keep_audio_on_same_track() {
        let _gst = gst_lock();
        let dir = make_profile("controller-batch-listens-move");
        scan_into(&dir, &[("a.wav", 40.0), ("b.wav", 40.0), ("c.wav", 40.0)]);
        let mut core = spawn(&dir).unwrap();
        let lib = core.snapshot();
        let a = id_of(&lib, "a.wav");
        let b = id_of(&lib, "b.wav");
        let c = id_of(&lib, "c.wav");
        core.set_queue(vec![a, b, c], a);
        wait_for(&core, 3., "first track", |s| s.is_playing);
        core.seek(22.);
        wait_for(&core, 3., "count listen", |s| {
            s.library.iter().find(|t| t.id == a).unwrap().play_count == 1
        });
        core.play_queue_index(usize::MAX);
        core.seek(0.);
        core.seek(22.);
        thread::sleep(Duration::from_millis(500));
        assert_eq!(
            core.snapshot()
                .library
                .iter()
                .find(|t| t.id == a)
                .unwrap()
                .play_count,
            1
        );
        core.move_queue(1, 2);
        let moved = core.barrier().unwrap();
        assert_eq!(
            moved.queue.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![a, c, b]
        );
        assert!(ends_with(&moved, "a.wav"));
        assert!(moved.is_playing);
        core.next();
        wait_for(&core, 3., "moved successor", |s| {
            ends_with(s, "c.wav") && s.is_playing
        });
        core.shutdown();
        let mut restored = spawn(&dir).unwrap();
        assert_eq!(
            restored
                .snapshot()
                .queue
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            vec![a, c, b]
        );
        restored.shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn editor_updates_all_queue_copies_and_reloads_current_lyrics() {
        let _gst = gst_lock();
        let dir = make_profile("controller-editor");
        scan_into(&dir, &[("a.wav", 10.0)]);
        let mut controller = spawn(&dir).unwrap();
        let track = controller.snapshot().library[0].clone();
        controller.set_queue(vec![track.id, track.id], track.id);
        wait_for(&controller, 3.0, "playing", |s| s.is_playing);
        controller.pause();
        controller.barrier().unwrap();
        let source = controller.load_editor(track.id).unwrap();
        assert_eq!(source.track.id, track.id);
        controller
            .save_edit(
                track.id,
                crate::editing::EditRequest::Tags(crate::domain::library::TrackEditInput {
                    title: "Changed in editor".into(),
                    artist: Some("Artist".into()),
                    album: None,
                    track_no: None,
                    genre: None,
                    year: None,
                    cover_path: None,
                }),
            )
            .unwrap();
        let snapshot = controller.barrier().unwrap();
        assert_eq!(snapshot.cursor, Some(0));
        assert_eq!(snapshot.queue.len(), 2);
        assert!(snapshot
            .queue
            .iter()
            .all(|t| t.title == "Changed in editor"));
        assert_eq!(snapshot.library[0].title, "Changed in editor");
        assert!(!snapshot.is_playing);
        controller
            .save_edit(
                track.id,
                crate::editing::EditRequest::Lyrics {
                    text: "[00:00]Edited lyric".into(),
                    in_tag: false,
                },
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let media = controller.media();
            if media.loaded
                && media
                    .lyrics
                    .first()
                    .is_some_and(|l| l.text == "Edited lyric")
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "current lyrics were not reloaded"
            );
            thread::sleep(Duration::from_millis(20));
        }
        controller.shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sleep_after_track_overrides_repeat_and_disarms_crossfade() {
        let _gst = gst_lock();
        let dir = make_profile("controller-sleep-eos");
        scan_into(&dir, &[("a.wav", 1.5), ("b.wav", 2.0)]);
        let mut controller = spawn(&dir).unwrap();
        let library = controller.snapshot();
        let a = id_of(&library, "a.wav");
        let b = id_of(&library, "b.wav");
        controller.preview_playback(PlaybackEdit::Crossfade(0.7));
        controller.set_repeat(RepeatMode::All);
        controller.set_sleep(SleepChoice::EndOfTrack);
        controller.set_queue(vec![a, b], a);
        wait_for(&controller, 3.0, "sleep armed", |s| {
            s.is_playing && s.sleep.end_of_track
        });
        let paused = wait_for(&controller, 5.0, "sleep EOS", |s| {
            !s.is_playing && !s.sleep.end_of_track
        });
        assert_eq!(paused.cursor, Some(0));
        assert!(ends_with(&paused, "a.wav"));
        thread::sleep(Duration::from_millis(400));
        assert_eq!(controller.snapshot().cursor, Some(0));
        controller.next();
        wait_for(&controller, 3.0, "manual resume after sleep", |s| {
            s.is_playing && s.cursor == Some(1)
        });
        controller.shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn plays_pauses_seeks_and_skips_through_a_real_library() {
        let _gst = gst_lock();
        let dir = make_profile("controller-transport");
        scan_into(&dir, &[("a.wav", 3.0), ("b.wav", 3.0), ("c.wav", 3.0)]);
        let Some(mut controller) = spawn(&dir) else {
            return;
        };

        let library = wait_for(&controller, 5.0, "the library", |s| s.library.len() == 3);
        let ids = vec![
            id_of(&library, "a.wav"),
            id_of(&library, "b.wav"),
            id_of(&library, "c.wav"),
        ];
        controller.set_queue(ids, id_of(&library, "a.wav"));

        wait_for(&controller, 5.0, "the first track", |s| {
            s.is_playing && ends_with(s, "a.wav")
        });
        controller.pause();
        let paused = wait_for(&controller, 5.0, "pause", |s| !s.is_playing);
        assert!(paused.cursor == Some(0));

        controller.seek(1.0);
        let sought = wait_for(&controller, 5.0, "the seek", |s| {
            (s.position_secs - 1.0).abs() < 0.25
        });
        assert!(sought.display_duration() >= 2.5, "duration from the file");

        controller.play();
        wait_for(&controller, 5.0, "resume", |s| {
            s.is_playing && s.position_secs > 1.0
        });

        controller.set_volume(0.3);
        wait_for(&controller, 2.0, "the volume", |s| {
            (s.volume - 0.3).abs() < 1e-9
        });

        controller.next();
        wait_for(&controller, 5.0, "next", |s| {
            s.is_playing && s.cursor == Some(1) && ends_with(s, "b.wav")
        });

        controller.previous();
        wait_for(&controller, 5.0, "previous", |s| {
            s.is_playing && s.cursor == Some(0) && ends_with(s, "a.wav")
        });

        controller.stop();
        let stopped = wait_for(&controller, 5.0, "stop", |s| !s.is_playing);
        assert!(stopped.current_path.is_none());
        controller.shutdown();
    }

    #[test]
    fn a_track_end_advances_exactly_once() {
        let _gst = gst_lock();
        let dir = make_profile("controller-auto-advance");
        // Short first track: it ends while we watch. Long second one: an
        // accidental second advance would move off it immediately.
        scan_into(
            &dir,
            &[("one.wav", 0.4), ("two.wav", 3.0), ("three.wav", 0.4)],
        );
        let Some(mut controller) = spawn(&dir) else {
            return;
        };

        let library = wait_for(&controller, 5.0, "the library", |s| s.library.len() == 3);
        let ids = vec![
            id_of(&library, "one.wav"),
            id_of(&library, "two.wav"),
            id_of(&library, "three.wav"),
        ];
        controller.set_queue(ids, id_of(&library, "one.wav"));

        let advanced = wait_for(&controller, 8.0, "the automatic advance", |s| {
            s.is_playing && s.cursor == Some(1)
        });
        assert!(
            ends_with(&advanced, "two.wav"),
            "{:?}",
            advanced.current_path
        );

        // Longer than the first track by a wide margin: if the hand-off were
        // also replayed as a queue `next`, the cursor would already be on
        // `three.wav` (or stopped after it).
        thread::sleep(Duration::from_millis(1500));
        let later = controller.snapshot();
        assert_eq!(later.cursor, Some(1), "advanced more than once");
        assert!(later.is_playing);
        assert!(ends_with(&later, "two.wav"), "{:?}", later.current_path);

        controller.shutdown();
    }

    #[test]
    fn shuffle_and_repeat_keep_the_active_order_addressable() {
        let _gst = gst_lock();
        let dir = make_profile("controller-shuffle-repeat");
        scan_into(
            &dir,
            &[
                ("a.wav", 4.0),
                ("b.wav", 4.0),
                ("c.wav", 4.0),
                ("d.wav", 4.0),
            ],
        );
        let Some(mut controller) = spawn(&dir) else {
            return;
        };

        let library = wait_for(&controller, 5.0, "the library", |s| s.library.len() == 4);
        let ids: Vec<i32> = ["a.wav", "b.wav", "c.wav", "d.wav"]
            .iter()
            .map(|name| id_of(&library, name))
            .collect();
        controller.set_queue(ids, id_of(&library, "a.wav"));
        wait_for(&controller, 5.0, "the first track", |s| s.is_playing);

        controller.set_shuffle(true);
        let shuffled = wait_for(&controller, 3.0, "shuffle", |s| s.shuffle);
        let mut display = shuffled.display.to_vec();
        display.sort_unstable();
        assert_eq!(display, vec![0, 1, 2, 3], "display is a permutation");

        // Repeat All: four `next` calls visit the other three elements and
        // come back to where they started.
        controller.cycle_repeat();
        let repeating = wait_for(&controller, 3.0, "repeat all", |s| {
            s.repeat == RepeatMode::All
        });
        let start = repeating.cursor.expect("a playing cursor");
        let mut visited = vec![start];
        for _ in 0..3 {
            let before = controller.snapshot();
            let expected = before.display[1];
            controller.next();
            let now = wait_for(&controller, 5.0, "the next shuffled track", |s| {
                s.cursor == Some(expected)
            });
            visited.push(now.cursor.unwrap());
        }
        visited.sort_unstable();
        assert_eq!(visited, vec![0, 1, 2, 3], "each element played once");

        controller.next();
        wait_for(&controller, 5.0, "the wrap back to the start", |s| {
            s.cursor == Some(start)
        });

        // Repeat One: `next` repeats the current element rather than moving.
        controller.cycle_repeat();
        wait_for(&controller, 3.0, "repeat one", |s| {
            s.repeat == RepeatMode::One
        });
        controller.next();
        let repeated = wait_for(&controller, 5.0, "the repeated element", |s| {
            s.cursor == Some(start)
        });
        assert!(repeated.is_playing);

        // Repeat Off at the end of the order stops instead of wrapping.
        // Shuffle goes off first so "the last element" is the queue's own last
        // one; with shuffle on, the display's tail is just whatever precedes
        // the cursor in the cycle, not the end of the play order.
        controller.set_shuffle(false);
        wait_for(&controller, 3.0, "shuffle off", |s| !s.shuffle);
        controller.cycle_repeat();
        wait_for(&controller, 3.0, "repeat off", |s| {
            s.repeat == RepeatMode::Off
        });
        let last = controller.snapshot().display.last().copied().unwrap();
        assert_eq!(last, 3);
        controller.play_queue_index(last);
        wait_for(&controller, 5.0, "the last element", |s| {
            s.cursor == Some(last) && s.is_playing
        });
        controller.next();
        let ended = wait_for(&controller, 5.0, "the end of the queue", |s| {
            !s.is_playing && s.current_path.is_none()
        });
        assert_eq!(ended.cursor, Some(last), "cursor kept the last element");

        controller.shutdown();
    }

    #[test]
    fn a_repeated_track_stays_a_separate_queue_entry() {
        let _gst = gst_lock();
        let dir = make_profile("controller-duplicates");
        scan_into(&dir, &[("a.wav", 4.0), ("b.wav", 4.0)]);
        let Some(mut controller) = spawn(&dir) else {
            return;
        };

        let library = wait_for(&controller, 5.0, "the library", |s| s.library.len() == 2);
        let a = id_of(&library, "a.wav");
        let b = id_of(&library, "b.wav");
        controller.set_queue(vec![a, b, a], a);
        let queued = wait_for(&controller, 5.0, "the queue", |s| s.queue.len() == 3);
        assert_eq!(queued.queue[0].path, queued.queue[2].path);
        assert_ne!(queued.queue[0].id, queued.queue[1].id);

        controller.play_queue_index(1);
        wait_for(&controller, 5.0, "the middle entry", |s| {
            s.cursor == Some(1) && ends_with(s, "b.wav")
        });
        controller.next();
        let third = wait_for(&controller, 5.0, "the repeated entry", |s| {
            s.cursor == Some(2) && s.is_playing
        });
        assert!(ends_with(&third, "a.wav"));
        assert_eq!(third.queue.len(), 3, "nothing was collapsed by path");

        controller.shutdown();
    }

    #[test]
    fn removing_the_current_element_continues_in_the_active_order() {
        let _gst = gst_lock();
        let dir = make_profile("controller-remove-current");
        scan_into(&dir, &[("a.wav", 4.0), ("b.wav", 4.0), ("c.wav", 4.0)]);
        let Some(mut controller) = spawn(&dir) else {
            return;
        };

        let library = wait_for(&controller, 5.0, "the library", |s| s.library.len() == 3);
        let ids = vec![
            id_of(&library, "a.wav"),
            id_of(&library, "b.wav"),
            id_of(&library, "c.wav"),
        ];
        controller.set_queue(ids, id_of(&library, "a.wav"));
        controller.play_queue_index(1);
        wait_for(&controller, 5.0, "the middle track", |s| {
            s.cursor == Some(1) && ends_with(s, "b.wav")
        });

        // The element playing goes away: playback continues with whatever
        // followed it in the active order — `c.wav`, now at index 1.
        controller.remove_queue_index(1);
        let after = wait_for(&controller, 5.0, "the successor", |s| {
            s.queue.len() == 2 && s.is_playing && ends_with(s, "c.wav")
        });
        assert_eq!(after.cursor, Some(1));

        // Removing the element *before* the cursor shifts the index but must
        // not change what is playing.
        controller.remove_queue_index(0);
        let shifted = wait_for(&controller, 5.0, "the shifted cursor", |s| {
            s.queue.len() == 1 && s.cursor == Some(0) && s.is_playing
        });
        assert!(ends_with(&shifted, "c.wav"));

        controller.shutdown();
    }

    #[test]
    fn corrupted_files_stop_after_five_failures_instead_of_looping() {
        let _gst = gst_lock();
        let dir = make_profile("controller-broken-files");
        let names = ["a.wav", "b.wav", "c.wav", "d.wav", "e.wav", "f.wav"];
        let tones: Vec<(&str, f64)> = names.iter().map(|name| (*name, 2.0)).collect();
        scan_into(&dir, &tones);
        // Valid headers at scan time, undecodable contents at play time.
        for name in names {
            std::fs::write(dir.join("music").join(name), vec![0u8; 4096]).unwrap();
        }
        let Some(mut controller) = spawn(&dir) else {
            return;
        };

        let library = wait_for(&controller, 5.0, "the library", |s| s.library.len() == 6);
        let ids: Vec<i32> = names.iter().map(|name| id_of(&library, name)).collect();
        controller.set_queue(ids, id_of(&library, "a.wav"));

        let gave_up = wait_for(&controller, 30.0, "the failure guard", |s| {
            !s.is_playing
                && s.error
                    .as_deref()
                    .is_some_and(|error| error.contains("5 failed tracks"))
        });
        assert!(gave_up.queue.len() == 6, "the queue itself is untouched");

        // Give it a moment to prove it really stopped rather than walking the
        // next file; a sixth attempt would overwrite the message.
        thread::sleep(Duration::from_millis(800));
        let later = controller.snapshot();
        assert!(!later.is_playing);
        assert_eq!(later.error, gave_up.error);

        controller.shutdown();
    }

    #[test]
    fn the_profile_reopens_with_the_same_queue_paused_at_the_same_place() {
        let _gst = gst_lock();
        let dir = make_profile("controller-session");
        scan_into(&dir, &[("a.wav", 4.0), ("b.wav", 4.0), ("c.wav", 4.0)]);

        let before = {
            let Some(mut controller) = spawn(&dir) else {
                return;
            };
            let library = wait_for(&controller, 5.0, "the library", |s| s.library.len() == 3);
            let ids = vec![
                id_of(&library, "a.wav"),
                id_of(&library, "b.wav"),
                id_of(&library, "c.wav"),
            ];
            controller.set_queue(ids, id_of(&library, "a.wav"));
            controller.set_shuffle(true);
            controller.set_volume(0.42);
            controller.play_queue_index(1);
            wait_for(&controller, 5.0, "the second track", |s| {
                s.cursor == Some(1) && s.is_playing
            });
            controller.seek(1.2);
            wait_for(&controller, 5.0, "the seek", |s| s.position_secs >= 1.15);
            controller.pause();
            let paused = wait_for(&controller, 5.0, "pause", |s| !s.is_playing);
            controller.shutdown();
            (*paused).clone()
        };

        // A new worker over the same profile: the session file is the only
        // thing it has.
        let Some(mut reopened) = spawn(&dir) else {
            return;
        };
        let restored = wait_for(&reopened, 5.0, "the restored session", |s| {
            s.queue.len() == 3
        });
        assert_eq!(paths(&restored), paths(&before), "same queue, same order");
        assert_eq!(restored.display, before.display, "same permutation");
        assert_eq!(restored.cursor, Some(1), "same cursor");
        assert!(!restored.is_playing, "restored paused, never blaring");
        assert!((restored.volume - 0.42).abs() < 1e-9, "same volume");
        assert!(
            (restored.position_secs - before.position_secs).abs() < 0.4,
            "position {} vs {}",
            restored.position_secs,
            before.position_secs
        );
        assert!(ends_with(&restored, "b.wav"));
        reopened.shutdown();
    }

    #[test]
    fn rescanning_leaves_the_playing_queue_numbered_as_it_was() {
        let _gst = gst_lock();
        let dir = make_profile("controller-rescan");
        scan_into(&dir, &[("a.wav", 4.0), ("b.wav", 4.0), ("c.wav", 4.0)]);
        let Some(mut controller) = spawn(&dir) else {
            return;
        };

        let library = wait_for(&controller, 5.0, "the library", |s| s.library.len() == 3);
        let ids = vec![
            id_of(&library, "a.wav"),
            id_of(&library, "b.wav"),
            id_of(&library, "c.wav"),
        ];
        controller.set_queue(ids, id_of(&library, "a.wav"));
        let playing = wait_for(&controller, 5.0, "the first track", |s| {
            s.cursor == Some(0) && s.is_playing
        });
        let queue_before = paths(&playing);

        // A new file arrives; the user rescans (the same path a changed search
        // would not take: searching is a view, not a queue mutation).
        write_tone(&dir.join("music"), "d.wav", 4.0);
        controller.rescan();
        wait_for(&controller, 5.0, "the scan to start", |s| s.scanning);
        let scanned = wait_for(&controller, 20.0, "the scan to finish", |s| {
            !s.scanning && s.library.len() == 4
        });
        assert_eq!(
            paths(&scanned),
            queue_before,
            "queue not rebuilt or renumbered"
        );
        assert_eq!(scanned.cursor, Some(0));
        assert!(scanned.is_playing);
        assert!(ends_with(&scanned, "a.wav"));

        controller.shutdown();
    }

    #[test]
    fn shutdown_stops_the_engine_and_ends_the_worker() {
        let _gst = gst_lock();
        let dir = make_profile("controller-shutdown");
        scan_into(&dir, &[("a.wav", 4.0)]);
        let Some(mut controller) = spawn(&dir) else {
            return;
        };

        let library = wait_for(&controller, 5.0, "the library", |s| s.library.len() == 1);
        controller.set_queue(vec![id_of(&library, "a.wav")], id_of(&library, "a.wav"));
        wait_for(&controller, 5.0, "playback", |s| s.is_playing);
        assert!(controller.is_running());

        let started = Instant::now();
        controller.shutdown();
        assert!(!controller.is_running(), "the worker thread ended");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "shutdown took {:?}",
            started.elapsed()
        );
        assert!(
            !controller.snapshot().is_playing,
            "the last state says stopped"
        );
    }

    #[test]
    fn enqueue_rearms_an_already_playing_last_track() {
        let _gst = gst_lock();
        let dir = make_profile("controller-enqueue-follow");
        scan_into(&dir, &[("a.wav", 1.2), ("b.wav", 4.0)]);
        let mut core = spawn(&dir).unwrap();
        let initial = core.snapshot();
        let a = id_of(&initial, "a.wav");
        let b = id_of(&initial, "b.wav");
        core.set_queue(vec![a], a);
        wait_for(&core, 3.0, "first track", |s| s.is_playing);
        core.enqueue(vec![b]);
        wait_for(&core, 6.0, "enqueued follow-on", |s| {
            s.cursor == Some(1) && s.is_playing
        });
        core.shutdown();
    }

    #[test]
    fn adjacent_duplicate_paths_advance_by_occurrence() {
        let _gst = gst_lock();
        let dir = make_profile("controller-adjacent-duplicates");
        scan_into(&dir, &[("a.wav", 0.8), ("b.wav", 4.0)]);
        let mut core = spawn(&dir).unwrap();
        let initial = core.snapshot();
        let a = id_of(&initial, "a.wav");
        let b = id_of(&initial, "b.wav");
        core.set_queue(vec![a, a, b], a);
        wait_for(&core, 6.0, "second occurrence of the same path", |s| {
            s.cursor == Some(1) && s.is_playing
        });
        wait_for(&core, 6.0, "track following both occurrences", |s| {
            s.cursor == Some(2) && s.is_playing
        });
        core.shutdown();
    }

    #[test]
    fn restoring_missing_files_persists_the_reindexed_queue() {
        let _gst = gst_lock();
        let dir = make_profile("controller-restore-missing");
        scan_into(&dir, &[("a.wav", 4.0), ("b.wav", 4.0)]);
        let store = SessionStore::new(&dir.join("config"));
        let saved = crate::domain::session::Session {
            queue_paths: vec![
                "/missing.wav".into(),
                dir.join("music/a.wav").to_string_lossy().into(),
                dir.join("music/b.wav").to_string_lossy().into(),
            ],
            shuffle_order: vec![2, 0, 1],
            cursor: 2,
            position_secs: 1.0,
            ..Default::default()
        };
        store.save(&saved).unwrap();
        let mut core = spawn(&dir).unwrap();
        assert_eq!(core.snapshot().cursor, Some(1));
        core.shutdown();
        let repaired = store.load_or_default();
        assert_eq!(repaired.queue_paths.len(), 2);
        assert_eq!(repaired.cursor, 1);
        assert_eq!(repaired.shuffle_order, vec![1, 0]);
        let mut reopened = spawn(&dir).unwrap();
        assert_eq!(reopened.snapshot().cursor, Some(1));
        assert!(ends_with(&reopened.snapshot(), "b.wav"));
        reopened.shutdown();
    }

    #[test]
    fn media_without_cover_or_lyrics_is_not_an_error() {
        let _gst = gst_lock();
        let dir = make_profile("controller-media");
        scan_into(&dir, &[("a.wav", 4.0)]);
        let Some(mut controller) = spawn(&dir) else {
            return;
        };

        let library = wait_for(&controller, 5.0, "the library", |s| s.library.len() == 1);
        let path = library.library[0].path.clone();
        controller.request_media(path.clone());

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let media = controller.media();
            if media.loaded && media.track_path.as_deref() == Some(path.as_str()) {
                assert!(media.cover.is_none(), "a bare WAV has no cover");
                assert!(media.lyrics.is_empty(), "and no lyrics");
                assert!(controller.snapshot().error.is_none(), "not an error");
                break;
            }
            assert!(Instant::now() < deadline, "timed out loading media");
            thread::sleep(Duration::from_millis(20));
        }

        controller.shutdown();
    }
    #[test]
    fn client_lifetime_barriers_and_stale_seeks_do_not_own_the_worker() {
        let _gst = gst_lock();
        let dir = make_profile("controller-client-ownership");
        scan_into(&dir, &[("a.wav", 20.0), ("b.wav", 20.0)]);
        let mut owner = spawn(&dir).unwrap();
        let client = owner.client();
        drop(client.clone());
        assert!(owner.is_running());
        let ids: Vec<_> = client.snapshot().library.iter().map(|t| t.id).collect();
        client.set_queue(ids.clone(), ids[0]);
        client.pause();
        let old = client.barrier().unwrap();
        assert_eq!(old.cursor, Some(0));
        client.play_queue_index(1);
        client.pause();
        client.barrier().unwrap();
        client.seek_for(old.current_path.clone().unwrap(), old.cursor, 7.0);
        let current = client.barrier().unwrap();
        assert_eq!(current.cursor, Some(1));
        assert_eq!(current.seek_revision, old.seek_revision);
        let (_, on) = client.toggle_favorite(Some(ids[1])).unwrap();
        assert!(on);
        let current = client.barrier().unwrap();
        assert!(current.queue[1].is_favorite);
        assert!(
            current
                .library
                .iter()
                .find(|t| t.id == ids[1])
                .unwrap()
                .is_favorite
        );
        assert_eq!(current.order.len(), 2);
        owner.shutdown();
        assert!(!owner.is_running());
        assert!(client.barrier().is_err());
    }
    #[test]
    fn playlist_edits_persist_without_changing_the_playing_queue() {
        let _gst = gst_lock();
        let dir = make_profile("controller-playlists");
        scan_into(&dir, &[("a.wav", 20.0), ("b.wav", 20.0), ("c.wav", 20.0)]);
        let mut core = spawn(&dir).unwrap();
        let ids: Vec<_> = core.snapshot().library.iter().map(|t| t.id).collect();
        core.set_queue(ids.clone(), ids[0]);
        core.pause();
        let before = core.barrier().unwrap();
        let id = core
            .edit_playlist(PlaylistEdit::Create {
                name: "  Mix  ".into(),
                tracks: vec![ids[2], ids[0]],
            })
            .unwrap()
            .unwrap();
        assert_eq!(core.snapshot().playlists[0].name, "Mix");
        core.edit_playlist(PlaylistEdit::Add {
            id,
            tracks: vec![ids[0], ids[1]],
        })
        .unwrap();
        core.edit_playlist(PlaylistEdit::Move {
            id,
            track: ids[1],
            offset: -1,
        })
        .unwrap();
        assert_eq!(
            core.snapshot().playlists[0].track_ids,
            vec![ids[2], ids[1], ids[0]]
        );
        assert!(core
            .edit_playlist(PlaylistEdit::Rename {
                id,
                name: "  ".into()
            })
            .is_err());
        assert!(core
            .edit_playlist(PlaylistEdit::Add {
                id,
                tracks: vec![i32::MAX]
            })
            .is_err());
        assert!(core
            .edit_playlist(PlaylistEdit::Delete { id: i32::MAX })
            .is_err());
        assert_eq!(core.snapshot().playlists[0].name, "Mix");
        core.edit_playlist(PlaylistEdit::Rename {
            id,
            name: "Road".into(),
        })
        .unwrap();
        core.edit_playlist(PlaylistEdit::Remove { id, track: ids[2] })
            .unwrap();
        let after = core.snapshot();
        assert_eq!(after.playlists[0].track_ids, vec![ids[1], ids[0]]);
        assert_eq!(after.cursor, before.cursor);
        assert_eq!(after.current_path, before.current_path);
        assert!(
            Arc::ptr_eq(&before.queue, &after.queue),
            "playlist edits never replace the playing queue"
        );
        assert!(
            Arc::ptr_eq(&before.library, &after.library),
            "playlist edits never rebuild track metadata"
        );
        core.shutdown();
        drop(core);
        let mut reopened = spawn(&dir).unwrap();
        assert_eq!(reopened.snapshot().playlists[0].name, "Road");
        assert_eq!(
            reopened.snapshot().playlists[0].track_ids,
            vec![ids[1], ids[0]]
        );
        reopened.edit_playlist(PlaylistEdit::Delete { id }).unwrap();
        assert!(reopened.snapshot().playlists.is_empty());
        assert_eq!(
            reopened.snapshot().queue.len(),
            3,
            "deleting a playlist keeps the restored queue"
        );
        reopened.shutdown();
    }
    #[test]
    fn audio_commands_reach_both_equalizers_and_flush_on_shutdown() {
        use crate::domain::playback_settings::ReplayGainMode;
        let _gst = gst_lock();
        let dir = make_profile("controller-audio-effects");
        let (tx, rx) = mpsc::channel();
        let player = AudioPlayer::new().unwrap();
        player.use_fake_sinks().unwrap();
        let mut worker = Worker::new(config_for(&dir), player, Arc::new(Shared::new()), tx, rx);
        worker.handle(Command::PlaybackEdit(PlaybackEdit::EqBand {
            index: 0,
            gain: 99.0,
        }));
        worker.handle(Command::PlaybackEdit(PlaybackEdit::EqBand {
            index: 9,
            gain: -100.0,
        }));
        for deck in worker.player.equalizer_gains_for_test() {
            assert_eq!(deck[0], 12.0);
            assert_eq!(deck[9], -24.0);
        }
        worker.handle(Command::PlaybackEdit(PlaybackEdit::ReplayGainMode(
            ReplayGainMode::Album,
        )));
        worker.handle(Command::PlaybackEdit(PlaybackEdit::Preamp(5.0)));
        worker.handle(Command::PlaybackEdit(PlaybackEdit::Crossfade(2.5)));
        let before = worker.playback.clone();
        worker.handle(Command::PlaybackEdit(PlaybackEdit::EqBand {
            index: 10,
            gain: 1.0,
        }));
        assert_eq!(*before, *worker.playback, "invalid edits are atomic");
        worker.handle(Command::PlaybackEdit(PlaybackEdit::Equalizer([0.; 10])));
        assert_eq!(
            worker.playback.crossfade_secs, 2.5,
            "an EQ preset cannot overwrite crossfade or ReplayGain"
        );
        assert_eq!(worker.playback.replay_gain.mode, ReplayGainMode::Album);
        assert!(
            !dir.join("config/playback_settings.json").exists(),
            "previews do not write per mouse event"
        );
        worker.finish();
        let stored = PlaybackSettingsStore::new(&dir.join("config")).load_or_default();
        assert_eq!(stored.crossfade_secs, 2.5);
        assert_eq!(stored.replay_gain.preamp_db, 5.0);
        assert_eq!(stored.eq_gains, [0.; 10]);
    }
    #[test]
    fn track_tempo_edits_survive_transitions_without_rebuilding_the_library() {
        let _gst = gst_lock();
        let dir = make_profile("controller-audio-tempos");
        scan_into(&dir, &[("a.wav", 20.0), ("b.wav", 20.0)]);
        let mut core = spawn(&dir).unwrap();
        let original = core.snapshot();
        let a = id_of(&original, "a.wav");
        let b = id_of(&original, "b.wav");
        core.set_queue(vec![a, b], a);
        core.pause();
        core.barrier().unwrap();
        core.preview_track_tempo(a, 1.25);
        core.play_queue_index(1);
        core.pause();
        core.barrier().unwrap();
        core.preview_track_tempo(a, 1.5);
        let s = core.barrier().unwrap();
        assert!(
            (s.tempo - 1.0).abs() < 0.001,
            "a late edit for A cannot alter B"
        );
        core.preview_track_tempo(b, 0.8);
        core.preview_playback(PlaybackEdit::Crossfade(3.0));
        core.save_audio().unwrap();
        let s = core.snapshot();
        assert!((s.tempo - 0.8).abs() < 0.001);
        assert!(!s.audio_dirty);
        assert!(
            Arc::ptr_eq(&original.library, &s.library),
            "speed drags do not rebuild search metadata"
        );
        assert_eq!(s.tempo_overrides[&a], 1.5);
        let db = Db::open(&dir.join("muzon.sqlite3")).unwrap();
        assert_eq!(
            db.track_tempo_by_path(&dir.join("music/a.wav").to_string_lossy())
                .unwrap(),
            Some(1.5)
        );
        core.set_tempo(0.7);
        let s = core.barrier().unwrap();
        assert!((s.tempo - 0.7).abs() < 0.001);
        assert_eq!(s.tempo_overrides[&b], 0.8, "MPRIS rate is temporary");
        core.set_repeat(RepeatMode::One);
        core.preview_playback(PlaybackEdit::Crossfade(3.5));
        let s = core.barrier().unwrap();
        assert!(
            (s.tempo - 0.7).abs() < 0.001,
            "rearming a repeated URI cannot reset the audible rate"
        );
        core.set_repeat(RepeatMode::Off);
        core.preview_playback(PlaybackEdit::Crossfade(3.0));
        core.play_queue_index(0);
        core.pause();
        let s = core.barrier().unwrap();
        assert!((s.tempo - 1.5).abs() < 0.001);
        core.play_queue_index(1);
        core.pause();
        core.barrier().unwrap();
        core.shutdown();
        drop(core);
        let mut restored = spawn(&dir).unwrap();
        let s = restored.snapshot();
        assert_eq!(s.playback.crossfade_secs, 3.0);
        assert!((s.tempo - 0.8).abs() < 0.001);
        assert!(!s.is_playing);
        restored.preview_playback(PlaybackEdit::EqBand {
            index: 2,
            gain: 4.0,
        });
        let s = wait_for(&restored, 5.0, "debounced effect persistence", |s| {
            s.playback.eq_gains[2] == 4.0 && !s.audio_dirty
        });
        assert!(s.audio_error.is_none());
        restored.shutdown();
        assert_eq!(
            PlaybackSettingsStore::new(&dir.join("config"))
                .load_or_default()
                .eq_gains[2],
            4.0
        );
    }
    #[test]
    fn failed_audio_save_is_reported_and_can_be_retried() {
        let _gst = gst_lock();
        let dir = make_profile("controller-audio-save-retry");
        let mut core = spawn(&dir).unwrap();
        let path = dir.join("config/playback_settings.json");
        std::fs::create_dir(&path).unwrap();
        core.preview_playback(PlaybackEdit::Crossfade(4.0));
        assert!(core.save_audio().is_err());
        assert!(core.snapshot().audio_dirty);
        assert!(core.snapshot().audio_error.is_some());
        std::fs::remove_dir(path).unwrap();
        core.save_audio().unwrap();
        assert!(!core.snapshot().audio_dirty);
        assert!(core.snapshot().audio_error.is_none());
        core.shutdown();
    }
}
