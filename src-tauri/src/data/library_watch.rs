//! Keeps the library in step with its folders: one rescan of every folder at
//! startup (whatever changed while the app was closed), then inotify for as
//! long as it runs.
//!
//! Every scan goes through `rescan`, which serialises them and tells the
//! frontend when the library actually changed, so the watcher, the settings
//! page's button and the S3 sync can't run over each other.

use crate::data::scanner;
use crate::domain::library::ScanReport;
use crate::state::AppState;
use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Copying an album in produces a burst of events per file; wait for the
/// folder to go quiet this long before scanning it.
const QUIET: Duration = Duration::from_millis(1500);

/// ...but not forever: a copy of a whole discography shouldn't keep the
/// library blind until it finishes.
const MAX_WAIT: Duration = Duration::from_secs(10);

/// Emitted with the `ScanReport` whenever a scan added, updated or removed
/// something; the frontend reloads the track list on it.
pub const LIBRARY_CHANGED: &str = "library-changed";

/// How often to look for a library folder that came back as a different
/// directory - a drive unplugged and mounted again.
const REARM_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct LibraryWatcher {
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// The inode each folder had when its watch was set. inotify watches the
    /// directory, not the path: once a drive is remounted the path names a new
    /// directory and the old watch is silently dead.
    armed: Mutex<HashMap<PathBuf, u64>>,
}

impl LibraryWatcher {
    pub fn watch(&self, root: &Path) {
        let Ok(meta) = std::fs::metadata(root) else {
            return;
        };
        if let Some(watcher) = self.watcher.lock().unwrap().as_mut() {
            // Usually the inotify watch limit on a very large tree; the library
            // still works, it just stops following changes live.
            if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
                eprintln!("[muzon library] не слежу за {}: {e}", root.display());
                return;
            }
            self.armed
                .lock()
                .unwrap()
                .insert(root.to_path_buf(), meta.ino());
        }
    }

    pub fn unwatch(&self, root: &Path) {
        self.armed.lock().unwrap().remove(root);
        if let Some(watcher) = self.watcher.lock().unwrap().as_mut() {
            let _ = watcher.unwatch(root);
        }
    }

    /// True when `root` is a directory again but not the one being watched.
    fn needs_rearming(&self, root: &Path) -> bool {
        let Ok(meta) = std::fs::metadata(root) else {
            return false;
        };
        meta.is_dir() && self.armed.lock().unwrap().get(root) != Some(&meta.ino())
    }
}

/// Called once, off the main thread, after the app state is in place.
pub fn start(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (tx, rx) = channel::<PathBuf>();
    let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        if !changes_the_library(&event.kind) {
            return;
        }
        for path in event.paths {
            if could_be_music(&path) {
                let _ = tx.send(path);
            }
        }
    });
    match watcher {
        Ok(w) => *state.library_watcher.watcher.lock().unwrap() = Some(w),
        Err(e) => eprintln!("[muzon library] inotify недоступен: {e}"),
    }

    let debounce_app = app.clone();
    std::thread::spawn(move || debounce(&debounce_app, rx));
    let rearm_app = app.clone();
    std::thread::spawn(move || rearm(&rearm_app));

    // Watch before the startup scan, so nothing that changes during it is missed
    let roots = state.db.list_roots().unwrap_or_default();
    for root in &roots {
        state.library_watcher.watch(root);
    }
    rescan(app, &roots);
}

/// Scans `roots` one after another, holding the scan lock, and announces the
/// result if anything changed.
pub fn rescan(app: &AppHandle, roots: &[PathBuf]) -> ScanReport {
    let state = app.state::<AppState>();
    let _serial = state.scan_lock.lock().unwrap();
    let mut report = ScanReport::default();
    for root in roots {
        match scanner::scan(&state.db, root) {
            Ok(r) => report.absorb(r),
            Err(e) => report.errors.push(format!("{}: {e}", root.display())),
        }
    }
    if report.changed_anything() {
        let _ = app.emit(LIBRARY_CHANGED, report.clone());
    }
    report
}

/// Reading a file (which playing it does) and touching its permissions or
/// times says nothing about its contents.
fn changes_the_library(kind: &EventKind) -> bool {
    !matches!(
        kind,
        EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_))
    )
}

/// Audio files and folders. A deleted folder can't be asked whether it was
/// one, so anything without an extension counts too; one with a dot in its
/// name ("Mr. Bungle") is only caught while it exists, and otherwise by the
/// next startup scan. Covers, `.part` downloads and playlists don't concern
/// the scanner.
fn could_be_music(path: &Path) -> bool {
    scanner::is_audio(path) || path.extension().is_none() || path.is_dir()
}

fn debounce(app: &AppHandle, rx: Receiver<PathBuf>) {
    while let Ok(first) = rx.recv() {
        let mut paths = vec![first];
        let started = Instant::now();
        while started.elapsed() < MAX_WAIT {
            match rx.recv_timeout(QUIET) {
                Ok(path) => paths.push(path),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }

        let roots = app.state::<AppState>().db.list_roots().unwrap_or_default();
        let affected = roots_containing(&roots, &paths);
        if !affected.is_empty() {
            rescan(app, &affected);
        }
    }
}

/// Picks up folders that came back as a new directory (or were unavailable at
/// startup) and catches up with whatever changed on them meanwhile.
fn rearm(app: &AppHandle) {
    loop {
        std::thread::sleep(REARM_INTERVAL);
        let state = app.state::<AppState>();
        if state.library_watcher.watcher.lock().unwrap().is_none() {
            return;
        }
        let returned: Vec<PathBuf> = state
            .db
            .list_roots()
            .unwrap_or_default()
            .into_iter()
            .filter(|root| state.library_watcher.needs_rearming(root))
            .collect();
        for root in &returned {
            state.library_watcher.unwatch(root);
            state.library_watcher.watch(root);
        }
        if !returned.is_empty() {
            rescan(app, &returned);
        }
    }
}

/// The library folders the changed paths are in, each once, in library order.
fn roots_containing(roots: &[PathBuf], paths: &[PathBuf]) -> Vec<PathBuf> {
    let hit: HashSet<&PathBuf> = paths
        .iter()
        .filter_map(|p| roots.iter().find(|root| p.starts_with(root)))
        .collect();
    roots.iter().filter(|r| hit.contains(r)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, DataChange, MetadataKind, RemoveKind};

    #[test]
    fn playing_a_file_does_not_trigger_a_scan() {
        assert!(!changes_the_library(&EventKind::Access(AccessKind::Any)));
        assert!(!changes_the_library(&EventKind::Modify(
            ModifyKind::Metadata(MetadataKind::AccessTime)
        )));
        assert!(changes_the_library(&EventKind::Create(CreateKind::File)));
        assert!(changes_the_library(&EventKind::Modify(ModifyKind::Data(
            DataChange::Content
        ))));
        assert!(changes_the_library(&EventKind::Remove(RemoveKind::Folder)));
    }

    #[test]
    fn only_audio_and_folders_count() {
        assert!(could_be_music(Path::new("/m/a/song.FLAC")));
        assert!(could_be_music(Path::new("/m/New Album")));
        assert!(!could_be_music(Path::new("/m/a/cover.jpg")));
        assert!(!could_be_music(Path::new("/m/a/song.mp3.part")));
    }

    #[test]
    fn changes_map_to_their_folders_by_component() {
        let roots = [PathBuf::from("/music"), PathBuf::from("/music2")];
        let paths = [
            PathBuf::from("/music2/x.mp3"),
            PathBuf::from("/music2/y.mp3"),
            PathBuf::from("/elsewhere/z.mp3"),
        ];
        assert_eq!(roots_containing(&roots, &paths), [PathBuf::from("/music2")]);
    }
}
