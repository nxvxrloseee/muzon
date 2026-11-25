//! Window preferences live in the explicit profile, independently of playback.
use iced::Size;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}
impl Default for Geometry {
    fn default() -> Self {
        Self {
            width: 1180.0,
            height: 760.0,
            maximized: false,
        }
    }
}
impl Geometry {
    pub fn size(self) -> Size {
        Size::new(self.width, self.height)
    }
    fn decode(value: serde_json::Value) -> Option<Self> {
        let width = value.get("width")?.as_f64()? as f32;
        let height = value.get("height")?.as_f64()? as f32;
        valid_size(Size::new(width, height)).then_some(Self {
            width,
            height,
            maximized: value
                .get("maximized")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }
}
fn valid_size(size: Size) -> bool {
    size.width.is_finite()
        && size.height.is_finite()
        && (800.0..=8192.0).contains(&size.width)
        && (480.0..=8192.0).contains(&size.height)
}

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    // Serializes atomic replacements, including the final shutdown flush.
    written: Arc<Mutex<u64>>,
}
impl Store {
    pub fn new(profile: &Path) -> Self {
        Self {
            path: profile.join("config/window.json"),
            written: Arc::new(Mutex::new(0)),
        }
    }
    fn load(&self) -> Geometry {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .and_then(Geometry::decode)
            .unwrap_or_default()
    }
    pub fn save(&self, revision: u64, geometry: Geometry) -> Result<(), String> {
        let mut written = self.written.lock().map_err(|e| e.to_string())?;
        if revision <= *written {
            return Ok(());
        }
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(self.path.parent().expect("window settings parent"))?;
            let bytes = serde_json::to_vec_pretty(&serde_json::json!({
                "width": geometry.width, "height": geometry.height,
                "maximized": geometry.maximized,
            }))?;
            let temporary = self.path.with_extension("json.tmp");
            let result = (|| {
                use std::io::Write;
                let mut file = std::fs::File::create(&temporary)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                std::fs::rename(&temporary, &self.path)
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(&temporary);
            }
            result
        };
        write().map_err(|e| e.to_string())?;
        *written = revision;
        Ok(())
    }
}

pub struct WindowMemory {
    pub geometry: Geometry,
    pub store: Store,
    revision: u64,
    observed: u64,
    deadline: Option<Instant>,
    saving: bool,
}
impl WindowMemory {
    pub fn new(profile: &Path) -> Self {
        let store = Store::new(profile);
        Self {
            geometry: store.load(),
            store,
            revision: 0,
            observed: 0,
            deadline: None,
            saving: false,
        }
    }
    pub fn next_observation(&mut self) -> u64 {
        self.observed += 1;
        self.observed
    }
    pub fn observe(&mut self, generation: u64, size: Size, maximized: bool) {
        if generation != self.observed || !valid_size(size) {
            return;
        }
        let geometry = if maximized {
            Geometry {
                maximized: true,
                ..self.geometry
            }
        } else {
            Geometry {
                width: size.width,
                height: size.height,
                maximized: false,
            }
        };
        if self.geometry != geometry {
            self.geometry = geometry;
            self.revision += 1;
            self.deadline = Some(Instant::now() + Duration::from_millis(500));
        }
    }
    pub fn due(&mut self) -> Option<(Store, u64, Geometry)> {
        if self.saving || !self.deadline.is_some_and(|d| Instant::now() >= d) {
            return None;
        }
        self.saving = true;
        Some(self.flush())
    }
    pub fn flush(&self) -> (Store, u64, Geometry) {
        (self.store.clone(), self.revision, self.geometry)
    }
    pub fn saved(&mut self, revision: u64) {
        self.saving = false;
        // A failure is reported once; another resize or shutdown retries.
        if revision == self.revision {
            self.deadline = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_or_out_of_range_sizes_use_defaults() {
        let dir = muzon_core::testing::scratch_dir("window-malformed");
        let store = Store::new(&dir);
        std::fs::create_dir_all(dir.join("config")).unwrap();
        for bytes in [
            "garbage",
            "{}",
            r#"{"width":0,"height":760}"#,
            r#"{"width":1000,"height":99999}"#,
        ] {
            std::fs::write(&store.path, bytes).unwrap();
            assert_eq!(store.load(), Geometry::default());
        }
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn maximize_keeps_normal_size_and_stale_observations_are_ignored() {
        let mut memory = WindowMemory::new(Path::new("/does-not-exist/muzon-test"));
        let old = memory.next_observation();
        let latest = memory.next_observation();
        memory.observe(old, Size::new(900.0, 600.0), false);
        assert_eq!(memory.geometry, Geometry::default());
        memory.observe(latest, Size::new(1920.0, 1080.0), true);
        assert!(memory.geometry.maximized);
        assert_eq!(memory.geometry.size(), Geometry::default().size());
        memory.observe(latest, Size::ZERO, false);
        assert!(memory.geometry.maximized);
    }
    #[test]
    fn late_autosave_cannot_overwrite_final_flush() {
        let dir = muzon_core::testing::scratch_dir("window-save-order");
        let store = Store::new(&dir);
        let latest = Geometry {
            width: 1000.0,
            height: 650.0,
            maximized: true,
        };
        store.save(2, latest).unwrap();
        store.save(1, Geometry::default()).unwrap();
        assert_eq!(store.load(), latest);
        assert_eq!(Store::new(&dir).load(), latest);
        assert!(!dir.join("config/window.json.tmp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
