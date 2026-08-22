use crate::data::db::{Db, NewTrack};
use crate::data::tags;
use crate::domain::library::ScanReport;
use std::collections::HashSet;
use std::path::Path;
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wv"];

pub fn scan(db: &Db, root: &Path) -> anyhow::Result<ScanReport> {
    let mut report = ScanReport::default();

    let known = db.known_tracks_under(root)?;
    let mut seen: HashSet<String> = HashSet::new();
    // Gather every upsert/removal first (tag reads are the slow, file-I/O-bound
    // part) with the DB connection untouched, then commit the whole batch in one
    // transaction - one lock acquisition and one fsync instead of one per file.
    let mut upserts: Vec<NewTrack> = Vec::new();

    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        let mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        seen.insert(path_str.clone());

        if let Some(known_mtime) = known.get(&path_str) {
            if *known_mtime == mtime {
                continue;
            }
        }

        match tags::read_tags(path) {
            Ok(t) => {
                let is_new = !known.contains_key(&path_str);
                upserts.push(NewTrack {
                    path: path_str,
                    title: t.title,
                    artist: t.artist,
                    album: t.album,
                    duration_secs: t.duration_secs,
                    track_no: t.track_no,
                    mtime,
                });
                if is_new {
                    report.added += 1;
                } else {
                    report.updated += 1;
                }
            }
            Err(e) => report.errors.push(format!("{}: {}", path.display(), e)),
        }
    }

    let removed: Vec<String> = known
        .keys()
        .filter(|p| !seen.contains(*p))
        .cloned()
        .collect();
    report.removed = removed.len() as u32;

    db.apply_scan_results(&upserts, &removed)?;

    Ok(report)
}
