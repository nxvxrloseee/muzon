use crate::data::db::{Db, NewTrack};
use crate::data::tags;
use crate::domain::library::ScanReport;
use crate::domain::Track;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wv"];

pub fn is_audio(path: &Path) -> bool {
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

/// One folder's findings, not yet written.
#[derive(Default)]
struct Plan {
    added: Vec<NewTrack>,
    updated: Vec<NewTrack>,
    removed: Vec<String>,
    errors: Vec<String>,
}

fn plan(db: &Db, root: &Path) -> anyhow::Result<Plan> {
    let known = db.known_tracks_under(root)?;

    // A folder on a drive that isn't mounted is indistinguishable from one
    // whose files were all deleted - except that deleting them would throw away
    // their favourites, play counts and playlist places. Leave such a folder
    // alone until it comes back.
    if !root.is_dir() {
        return Ok(Plan {
            errors: vec![format!("{}: папка недоступна", root.display())],
            ..Plan::default()
        });
    }

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
                        genre: t.genre,
                        year: t.year,
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

    let mut plan = Plan::default();
    for outcome in scanned {
        match outcome {
            Scanned::Unchanged => {}
            Scanned::Added(track) => plan.added.push(*track),
            Scanned::Updated(track) => plan.updated.push(*track),
            Scanned::Failed(message) => plan.errors.push(message),
        }
    }

    // An empty mount point looks the same as an unmounted drive's folder: an
    // existing directory with nothing in it. Losing every file at once is far
    // more likely to be that than a deliberate purge.
    plan.removed = if seen.is_empty() && !known.is_empty() {
        plan.errors.push(format!(
            "{}: в папке не осталось ни одного файла, библиотека не тронута",
            root.display()
        ));
        Vec::new()
    } else {
        known
            .keys()
            .filter(|p| !seen.contains(*p))
            .cloned()
            .collect()
    };
    Ok(plan)
}

#[cfg(test)]
pub fn scan(db: &Db, root: &Path) -> anyhow::Result<ScanReport> {
    scan_roots(db, &[root.to_path_buf()])
}

/// Scans several library folders as one pass, so a file moved from one of
/// them to another is recognised as the same track.
pub fn scan_roots(db: &Db, roots: &[PathBuf]) -> anyhow::Result<ScanReport> {
    let mut all = Plan::default();
    for root in roots {
        let p = plan(db, root)?;
        all.added.extend(p.added);
        all.updated.extend(p.updated);
        all.removed.extend(p.removed);
        all.errors.extend(p.errors);
    }

    // Renaming or moving a file makes it vanish from one path and appear at
    // another. Treated as a delete and an add, that would throw away its
    // favourite, play count, date added and playlist places - so first pair
    // up what vanished with what appeared, and keep the row
    let removed_tracks: Vec<Track> = if all.removed.is_empty() || all.added.is_empty() {
        Vec::new()
    } else {
        let removed: HashSet<&String> = all.removed.iter().collect();
        db.list_tracks()?
            .into_iter()
            .filter(|t| removed.contains(&t.path))
            .collect()
    };
    let pairs = match_moves(&removed_tracks, &all.added);

    let moved_from: HashSet<&str> = pairs
        .iter()
        .map(|(r, _)| removed_tracks[*r].path.as_str())
        .collect();
    let moved_to: HashSet<usize> = pairs.iter().map(|(_, a)| *a).collect();
    let moves: Vec<(String, NewTrack)> = pairs
        .iter()
        .map(|(r, a)| (removed_tracks[*r].path.clone(), all.added[*a].clone()))
        .collect();
    let removed: Vec<String> = all
        .removed
        .iter()
        .filter(|p| !moved_from.contains(p.as_str()))
        .cloned()
        .collect();
    let added: Vec<NewTrack> = all
        .added
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !moved_to.contains(i))
        .map(|(_, t)| t)
        .collect();

    let report = ScanReport {
        added: added.len() as u32,
        updated: all.updated.len() as u32,
        removed: removed.len() as u32,
        moved: moves.len() as u32,
        errors: all.errors,
    };
    let mut upserts = added;
    upserts.extend(all.updated);
    db.apply_scan_results(&upserts, &removed, &moves)?;
    Ok(report)
}

/// What a file's tags say it is. A rename or a move leaves the file itself
/// untouched, so all of this comes through the same; the duration is to the
/// tenth of a second, which two different encodings rarely share.
type Identity = (String, String, String, Option<i32>, Option<i64>);

fn identity(
    title: &str,
    artist: Option<&str>,
    album: Option<&str>,
    track_no: Option<i32>,
    duration_secs: Option<f64>,
) -> Identity {
    let norm = |s: Option<&str>| s.unwrap_or("").trim().to_lowercase();
    (
        norm(Some(title)),
        norm(artist),
        norm(album),
        track_no,
        duration_secs.map(|d| (d * 10.0).round() as i64),
    )
}

/// Pairs vanished tracks with appeared files that are evidently the same
/// recording, as `(index into removed, index into added)`.
///
/// Only one-to-one matches count. Two copies of a song vanishing, or
/// appearing, at once can't be told apart - and guessing wrong would give one
/// file the other's history, which is worse than losing it.
fn match_moves(removed: &[Track], added: &[NewTrack]) -> Vec<(usize, usize)> {
    let mut gone: HashMap<Identity, Vec<usize>> = HashMap::new();
    for (i, t) in removed.iter().enumerate() {
        let key = identity(&t.title, t.artist.as_deref(), t.album.as_deref(), t.track_no, t.duration_secs);
        gone.entry(key).or_default().push(i);
    }
    let mut new: HashMap<Identity, Vec<usize>> = HashMap::new();
    for (i, t) in added.iter().enumerate() {
        let key = identity(&t.title, t.artist.as_deref(), t.album.as_deref(), t.track_no, t.duration_secs);
        new.entry(key).or_default().push(i);
    }
    let mut pairs: Vec<(usize, usize)> = gone
        .iter()
        .filter_map(|(key, r)| match (r.as_slice(), new.get(key).map(Vec::as_slice)) {
            ([r], Some([a])) => Some((*r, *a)),
            _ => None,
        })
        .collect();
    pairs.sort_unstable();
    pairs
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
    fn genre_and_year_come_from_the_file_tags() {
        let dir = scratch_dir("scan-genre-year");
        let path = write_tone(&dir, "a.wav", 0.1);
        crate::data::tag_writer::write_tags(
            &path,
            &crate::data::tag_writer::TagEdit {
                title: "Tone",
                artist: None,
                album: None,
                track_no: None,
                genre: Some("Ambient"),
                year: Some(1978),
                cover: None,
            },
        )
        .unwrap();
        let db = open_db(&dir);

        scan(&db, &dir).unwrap();

        let track = &db.list_tracks().unwrap()[0];
        assert_eq!(track.genre.as_deref(), Some("Ambient"));
        assert_eq!(track.year, Some(1978));
    }

    /// A tone with real tags, so it is recognisable after a rename.
    fn tagged(dir: &Path, name: &str, title: &str) -> PathBuf {
        let path = write_tone(dir, name, 0.2);
        crate::data::tag_writer::write_tags(
            &path,
            &crate::data::tag_writer::TagEdit {
                title,
                artist: Some("Artist"),
                album: Some("Album"),
                track_no: Some(1),
                genre: None,
                year: None,
                cover: None,
            },
        )
        .unwrap();
        path
    }

    /// Likes, plays and playlists the only track in the library, returning
    /// its id and the playlist's.
    fn give_history(db: &Db) -> (i32, i32) {
        let id = db.list_tracks().unwrap()[0].id;
        db.toggle_favorite(id).unwrap();
        db.record_play(id).unwrap();
        let playlist = db.create_playlist("Mix").unwrap();
        db.add_track_to_playlist(playlist, id).unwrap();
        (id, playlist)
    }

    fn assert_history_kept(db: &Db, id: i32, playlist: i32, now_at: &Path) {
        let track = db.track_by_path(&now_at.to_string_lossy()).unwrap().unwrap();
        assert_eq!(track.id, id, "the row should have moved, not been replaced");
        assert!(track.is_favorite);
        assert_eq!(track.play_count, 1);
        assert_eq!(db.playlist_tracks(playlist).unwrap()[0].id, id);
        assert_eq!(db.list_tracks().unwrap().len(), 1);
    }

    #[test]
    fn a_renamed_file_keeps_its_history() {
        let dir = scratch_dir("scan-rename");
        let old = tagged(&dir, "a.wav", "Song");
        let db = open_db(&dir);
        scan(&db, &dir).unwrap();
        let (id, playlist) = give_history(&db);

        let new = dir.join("renamed.wav");
        std::fs::rename(&old, &new).unwrap();
        let report = scan(&db, &dir).unwrap();

        assert_eq!((report.moved, report.added, report.removed), (1, 0, 0));
        assert_history_kept(&db, id, playlist, &new);
    }

    #[test]
    fn a_file_moved_into_a_subfolder_keeps_its_history() {
        let dir = scratch_dir("scan-move-sub");
        let old = tagged(&dir, "a.wav", "Song");
        let db = open_db(&dir);
        scan(&db, &dir).unwrap();
        let (id, playlist) = give_history(&db);

        let new = dir.join("Artist").join("Album").join("a.wav");
        std::fs::create_dir_all(new.parent().unwrap()).unwrap();
        std::fs::rename(&old, &new).unwrap();
        assert_eq!(scan(&db, &dir).unwrap().moved, 1);
        assert_history_kept(&db, id, playlist, &new);
    }

    #[test]
    fn a_file_moved_between_library_folders_keeps_its_history() {
        let dir = scratch_dir("scan-move-roots");
        let (one, two) = (dir.join("one"), dir.join("two"));
        std::fs::create_dir_all(&two).unwrap();
        let old = tagged(&one, "a.wav", "Song");
        let db = open_db(&dir);
        scan_roots(&db, &[one.clone(), two.clone()]).unwrap();
        let (id, playlist) = give_history(&db);

        // Leave a file behind, or the emptied folder reads as an unplugged drive
        tagged(&one, "stays.wav", "Other");
        scan_roots(&db, &[one.clone(), two.clone()]).unwrap();
        let new = two.join("a.wav");
        std::fs::rename(&old, &new).unwrap();
        let report = scan_roots(&db, &[one, two]).unwrap();

        assert_eq!((report.moved, report.removed), (1, 0));
        let track = db.track_by_path(&new.to_string_lossy()).unwrap().unwrap();
        assert_eq!(track.id, id);
        assert!(track.is_favorite);
        assert_eq!(db.playlist_tracks(playlist).unwrap()[0].id, id);
    }

    #[test]
    fn identical_copies_moved_together_are_not_guessed_at() {
        let dir = scratch_dir("scan-move-twins");
        let a = tagged(&dir, "a.wav", "Twin");
        let b = tagged(&dir, "b.wav", "Twin");
        tagged(&dir, "keep.wav", "Keep");
        let db = open_db(&dir);
        scan(&db, &dir).unwrap();

        std::fs::rename(&a, dir.join("c.wav")).unwrap();
        std::fs::rename(&b, dir.join("d.wav")).unwrap();
        let report = scan(&db, &dir).unwrap();

        // Which new file had which history can't be known
        assert_eq!((report.moved, report.added, report.removed), (0, 2, 2));
    }

    #[test]
    fn an_untagged_file_renamed_is_a_new_track() {
        // Its title comes from the file name, so the rename changes what it
        // is as far as anyone can tell
        let dir = scratch_dir("scan-rename-untagged");
        let old = write_tone(&dir, "a.wav", 0.2);
        write_tone(&dir, "keep.wav", 0.2);
        let db = open_db(&dir);
        scan(&db, &dir).unwrap();

        std::fs::rename(&old, dir.join("b.wav")).unwrap();
        let report = scan(&db, &dir).unwrap();
        assert_eq!((report.moved, report.added, report.removed), (0, 1, 1));
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
    fn a_folder_that_went_missing_keeps_its_tracks() {
        let dir = scratch_dir("scan-unmounted");
        let music = dir.join("drive");
        write_tone(&music, "a.wav", 0.1);
        let db = open_db(&dir);
        scan(&db, &music).unwrap();

        // What an unplugged drive looks like from here
        std::fs::remove_dir_all(&music).unwrap();
        let report = scan(&db, &music).unwrap();

        assert_eq!(report.removed, 0);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(db.list_tracks().unwrap().len(), 1);
    }

    #[test]
    fn an_emptied_folder_is_treated_as_unavailable_not_purged() {
        let dir = scratch_dir("scan-empty-mount");
        let music = dir.join("mnt");
        let a = write_tone(&music, "a.wav", 0.1);
        let b = write_tone(&music, "b.wav", 0.1);
        let db = open_db(&dir);
        scan(&db, &music).unwrap();

        // The mount point directory is still there, with nothing in it
        std::fs::remove_file(a).unwrap();
        std::fs::remove_file(b).unwrap();
        let report = scan(&db, &music).unwrap();

        assert_eq!(report.removed, 0);
        assert_eq!(db.list_tracks().unwrap().len(), 2);
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
