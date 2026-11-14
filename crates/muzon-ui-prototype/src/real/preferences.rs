//! Ordered atomic JSON preferences; never used for credentials.
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    written: Arc<Mutex<u64>>,
}
impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            written: Arc::new(Mutex::new(0)),
        }
    }
    pub fn save(&self, revision: u64, value: &serde_json::Value) -> Result<(), String> {
        let mut written = self.written.lock().map_err(|e| e.to_string())?;
        if revision <= *written {
            return Ok(());
        }
        let result = (|| -> anyhow::Result<()> {
            use std::io::Write;
            std::fs::create_dir_all(self.path.parent().unwrap())?;
            let temp = self.path.with_extension("json.tmp");
            let result = (|| -> anyhow::Result<()> {
                let mut file = std::fs::File::create(&temp)?;
                file.write_all(&serde_json::to_vec_pretty(value)?)?;
                file.sync_all()?;
                std::fs::rename(&temp, &self.path)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(&temp);
            }
            result
        })();
        result.map_err(|e| e.to_string())?;
        *written = revision;
        Ok(())
    }
}
