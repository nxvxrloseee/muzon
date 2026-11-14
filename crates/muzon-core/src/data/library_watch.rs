//! Inotify notifications are coalesced into one flag, never an unbounded queue.
use notify::{event::ModifyKind, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
pub struct LibraryWatch {
    watcher: Option<RecommendedWatcher>,
    changed: Arc<AtomicBool>,
    armed: HashMap<PathBuf, (u64, u64)>,
    first: Option<Instant>,
    last: Instant,
    rearm: Instant,
}
impl LibraryWatch {
    pub fn new() -> Self {
        let changed = Arc::new(AtomicBool::new(false));
        let flag = changed.clone();
        let watcher =
            notify::recommended_watcher(
                move |result: notify::Result<notify::Event>| match result {
                    Ok(event)
                        if !matches!(
                            event.kind,
                            EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_))
                        ) && event.paths.iter().any(|p| relevant(p)) =>
                    {
                        flag.store(true, Ordering::Release);
                    }
                    Err(_) => {
                        flag.store(true, Ordering::Release);
                    }
                    _ => {}
                },
            )
            .map_err(|error| eprintln!("[muzon watch] {error}"))
            .ok();
        let now = Instant::now();
        Self {
            watcher,
            changed,
            armed: HashMap::new(),
            first: None,
            last: now,
            rearm: now,
        }
    }
    pub fn sync(&mut self, roots: &[PathBuf]) -> bool {
        use std::os::unix::fs::MetadataExt;
        let Some(watcher) = &mut self.watcher else {
            return false;
        };
        let mut returned = false;
        self.armed.retain(|root, _| {
            if roots.contains(root) {
                true
            } else {
                let _ = watcher.unwatch(root);
                false
            }
        });
        for root in roots {
            let Ok(metadata) = std::fs::metadata(root) else {
                continue;
            };
            if !metadata.is_dir() {
                continue;
            }
            let identity = (metadata.dev(), metadata.ino());
            if self.armed.get(root) != Some(&identity) {
                let _ = watcher.unwatch(root);
                match watcher.watch(root, RecursiveMode::Recursive) {
                    Ok(()) => {
                        self.armed.insert(root.clone(), identity);
                        returned = true;
                    }
                    Err(error) => eprintln!("[muzon watch] {}: {error}", root.display()),
                }
            }
        }
        self.rearm = Instant::now() + Duration::from_secs(30);
        returned
    }
    pub fn needs_rearm(&self) -> bool {
        Instant::now() >= self.rearm
    }
    pub fn due(&mut self) -> bool {
        let now = Instant::now();
        if self.changed.swap(false, Ordering::AcqRel) {
            self.first.get_or_insert(now);
            self.last = now;
        }
        if self.first.is_some_and(|first| {
            now.duration_since(self.last) >= Duration::from_millis(1500)
                || now.duration_since(first) >= Duration::from_secs(10)
        }) {
            self.first = None;
            return true;
        }
        false
    }
}
fn relevant(path: &Path) -> bool {
    if path.extension().is_some_and(|s| s == "tmp" || s == "part") {
        return false;
    }
    crate::data::scanner::is_audio(path)
        || path.is_dir()
        || path.extension().is_none()
        || path.extension().is_some_and(|s| s == "lrc")
        || path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| {
                matches!(
                    name.to_ascii_lowercase().as_str(),
                    "cover.jpg"
                        | "cover.jpeg"
                        | "cover.png"
                        | "folder.jpg"
                        | "folder.jpeg"
                        | "folder.png"
                        | "front.jpg"
                        | "front.png"
                )
            })
}
impl Default for LibraryWatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_notifications_coalesce_until_the_folder_is_quiet() {
        let dir = crate::testing::scratch_dir("native-watch");
        let mut watch = LibraryWatch::new();
        assert!(watch.sync(std::slice::from_ref(&dir)));
        crate::testing::write_tone(&dir, "new.wav", 1.0);
        let deadline = Instant::now() + Duration::from_secs(8);
        while !watch.due() {
            assert!(
                Instant::now() < deadline,
                "inotify did not trigger a debounced scan"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!watch.due());
        assert!(!relevant(Path::new("dc783919ab.png")));
        drop(watch);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn temporary_files_and_playback_access_do_not_trigger_scans() {
        assert!(!relevant(Path::new(".song.wav.muzon-1-2.tmp")));
        assert!(relevant(Path::new("song.flac")));
        assert!(relevant(Path::new("song.lrc")));
    }
}
