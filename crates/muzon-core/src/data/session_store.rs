use crate::domain::session::Session;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Clone)]
pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("session.json"),
        }
    }

    pub fn load_or_default(&self) -> Session {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, session: &Session) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        use std::io::Write;
        let bytes = serde_json::to_vec_pretty(session)?;
        let temporary = self.path.with_extension("json.tmp");
        let result = (|| -> anyhow::Result<()> {
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}

/// The live session plus the disk copy behind it.
///
/// The split matters because the two halves change at wildly different rates:
/// the queue is one path per track and is replaced only when you start playing
/// from somewhere new, while the playhead moves continuously. So mutations only
/// ever touch memory and mark the session dirty, and writing to disk is a
/// separate, deliberate act - a slow heartbeat plus the window closing. That
/// keeps a multi-megabyte queue from being re-serialized every few seconds just
/// because a song is playing.
pub struct SessionState {
    current: Mutex<Session>,
    dirty: AtomicBool,
    store: SessionStore,
}

impl SessionState {
    pub fn new(store: SessionStore) -> Self {
        Self {
            current: Mutex::new(store.load_or_default()),
            dirty: AtomicBool::new(false),
            store,
        }
    }

    pub fn snapshot(&self) -> Session {
        self.current.lock().unwrap().clone()
    }

    pub fn update(&self, edit: impl FnOnce(&mut Session)) {
        edit(&mut self.current.lock().unwrap());
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Writes to disk, but only if something has changed since the last flush.
    pub fn flush(&self) -> anyhow::Result<()> {
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return Ok(());
        }
        let session = self.snapshot();
        if let Err(e) = self.store.save(&session) {
            // Put the flag back so the next heartbeat retries rather than
            // silently dropping the write.
            self.dirty.store(true, Ordering::Relaxed);
            return Err(e);
        }
        Ok(())
    }
}
