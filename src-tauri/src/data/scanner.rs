use crate::data::db::{Db, NewTrack};
use crate::data::tags;
use crate::domain::library::ScanReport;
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wv"];

fn is_audio(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    AUDIO_EXTENSIONS.contains(&ext.as_str())
}

fn mtime_of(entry: &walkdir::DirEntry) -> i64 {
    entry
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// What reading one file produced. Kept as a value rather than counted in place
/// so the reads can run in parallel and still be folded into one report in a
/// fixed order afterwards.
enum Scanned {
    Unchanged,
    Added(Box<NewTrack>),
    Updated(Box<NewTrack>),
    Failed(String),
}

pub fn scan(db: &Db, root: &Path) -> anyhow::Result<ScanReport> {
    let known = db.known_tracks_under(root)?;

    // Walking the tree is cheap and inherently sequential, so it only gathers
    // the work. Reading tags is the expensive part - file I/O plus a parse per
    // file - and that is what gets spread across cores.
    let candidates: Vec<(PathBuf, i64)> = WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| entry.file_type().is_file() && is_audio(entry.path()))
        .map(|entry| {
            let mtime = mtime_of(&entry);
            (entry.into_path(), mtime)
        })
        .collect();

    let seen: HashSet<String> = candidates
        .iter()
        .map(|(path, _)| path.to_string_lossy().to_string())
        .collect();

    // `collect` off a parallel iterator keeps input order, so the same folder
    // always produces the same batch - worth having for reproducible reports.
    let scanned: Vec<Scanned> = candidates
        .into_par_iter()
        .map(|(path, mtime)| {
            let path_str = path.to_string_lossy().to_string();
            let known_mtime = known.get(&path_str);
            if known_mtime == Some(&mtime) {
                return Scanned::Unchanged;
            }

            match tags::read_tags(&path) {
                Ok(t) => {
                    let track = Box::new(NewTrack {
                        path: path_str,
                        title: t.title,
                        artist: t.artist,
                        album: t.album,
                        duration_secs: t.duration_secs,
                        track_no: t.track_no,
                        mtime,
                    });
                    if known_mtime.is_none() {
                        Scanned::Added(track)
                    } else {
                        Scanned::Updated(track)
                    }
                }
                Err(e) => Scanned::Failed(format!("{}: {}", path.display(), e)),
            }
        })
        .collect();

    let mut report = ScanReport::default();
    let mut upserts: Vec<NewTrack> = Vec::new();
    for outcome in scanned {
        match outcome {
            Scanned::Unchanged => {}
            Scanned::Added(track) => {
                report.added += 1;
                upserts.push(*track);
            }
            Scanned::Updated(track) => {
                report.updated += 1;
                upserts.push(*track);
            }
            Scanned::Failed(message) => report.errors.push(message),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{scratch_dir, write_tone};

    fn open_db(dir: &Path) -> Db {
        Db::open(&dir.join("library.sqlite3")).unwrap()
    }

    #[test]
    fn a_first_scan_adds_every_audio_file_and_ignores_the_rest() {
        let dir = scratch_dir("scan-first");
        write_tone(&dir, "a.wav", 0.1);
        write_tone(&dir, "b.wav", 0.1);
        std::fs::write(dir.join("cover.jpg"), b"not audio").unwrap();
        std::fs::write(dir.join("notes.txt"), b"not audio").unwrap();
        let db = open_db(&dir);

        let report = scan(&db, &dir).unwrap();

        assert_eq!(report.added, 2);
        assert_eq!(report.updated, 0);
        assert_eq!(report.removed, 0);
        assert!(report.errors.is_empty());
        assert_eq!(db.list_tracks().unwrap().len(), 2);
    }

    #[test]
    fn scanning_into_subfolders_finds_files_at_any_depth() {
        let dir = scratch_dir("scan-nested");
        write_tone(&dir, "top.wav", 0.1);
        write_tone(&dir.join("album").join("disc 1"), "deep.wav", 0.1);
        let db = open_db(&dir);

        assert_eq!(scan(&db, &dir).unwrap().added, 2);
    }

    #[test]
    fn re_scanning_untouched_files_does_nothing() {
        let dir = scratch_dir("scan-repeat");
        write_tone(&dir, "a.wav", 0.1);
        let db = open_db(&dir);
        scan(&db, &dir).unwrap();

        // The mtime hasn't moved, so the second pass must not even read the
        // tags again, let alone report the file as updated.
        let report = scan(&db, &dir).unwrap();
        assert_eq!(report.added, 0);
        assert_eq!(report.updated, 0);
        assert_eq!(report.removed, 0);
        assert_eq!(db.list_tracks().unwrap().len(), 1);
    }

    /// Backdates a file so a change to it is distinguishable from the scan that
    /// recorded it. Necessary because mtimes are stored with one-second
    /// resolution: simply rewriting a file within the same second as the last
    /// scan looks unchanged, and the scanner will skip it until it is touched
    /// again in some later second.
    fn backdate(path: &Path, seconds: u64) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(seconds))
            .unwrap();
    }

    #[test]
    fn a_file_whose_mtime_moved_is_re_read() {
        let dir = scratch_dir("scan-touched");
        let path = write_tone(&dir, "a.wav", 0.1);
        backdate(&path, 60);
        let db = open_db(&dir);
        assert_eq!(scan(&db, &dir).unwrap().added, 1);

        // A newer mtime is the only signal the scanner has that a file's tags
        // may have changed.
        write_tone(&dir, "a.wav", 0.2);

        let report = scan(&db, &dir).unwrap();
        assert_eq!(report.added, 0);
        assert_eq!(report.updated, 1);
    }

    #[test]
    fn deleted_files_are_removed_from_the_library() {
        let dir = scratch_dir("scan-deleted");
        write_tone(&dir, "a.wav", 0.1);
        let gone = write_tone(&dir, "b.wav", 0.1);
        let db = open_db(&dir);
        scan(&db, &dir).unwrap();

        std::fs::remove_file(&gone).unwrap();
        let report = scan(&db, &dir).unwrap();

        assert_eq!(report.removed, 1);
        let paths: Vec<String> = db.list_tracks().unwrap().into_iter().map(|t| t.path).collect();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("a.wav"));
    }

    #[test]
    fn an_unreadable_file_is_reported_without_stopping_the_scan() {
        let dir = scratch_dir("scan-broken");
        write_tone(&dir, "good.wav", 0.1);
        // A real extension over bytes that are not audio at all.
        std::fs::write(dir.join("broken.flac"), b"this is not a flac file").unwrap();
        let db = open_db(&dir);

        let report = scan(&db, &dir).unwrap();

        assert_eq!(report.added, 1, "the readable file still has to land");
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("broken.flac"));
    }
}

#[cfg(test)]
mod benchmark {
    use super::*;
    use std::time::Instant;

    /// Compares the tag-reading phase - the part rayon actually parallelises -
    /// against doing the same reads one after another. Ignored by default: it
    /// needs a real music folder and reports rather than asserts.
    #[test]
    #[ignore = "measures against the machine's own music library"]
    fn tag_reading_scales_across_cores() {
        let root = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Music");
        if !root.is_dir() {
            return;
        }

        let files: Vec<PathBuf> = WalkDir::new(&root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file() && is_audio(e.path()))
            .map(|e| e.into_path())
            .collect();

        let started = Instant::now();
        let sequential: usize = files.iter().filter(|p| tags::read_tags(p).is_ok()).count();
        let sequential_time = started.elapsed();

        let started = Instant::now();
        let parallel: usize = files
            .par_iter()
            .filter(|p| tags::read_tags(p).is_ok())
            .count();
        let parallel_time = started.elapsed();

        assert_eq!(sequential, parallel);
        println!(
            "{} files | sequential {:?} | parallel {:?} | {:.1}x on {} cores",
            files.len(),
            sequential_time,
            parallel_time,
            sequential_time.as_secs_f64() / parallel_time.as_secs_f64(),
            std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        );
    }
}
