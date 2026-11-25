//! Mirroring the library to S3-compatible storage.
//!
//! Two directions, no deletions: what is missing on either side is copied, and
//! differing existing objects are reported as conflicts; nothing is ever removed.
//! A mistake in a prefix or a bucket can then cost disk space, never music.
//!
//! Layout under the configured prefix:
//!
//! ```text
//! muzon/tracks/<имя папки>/<путь внутри неё>.flac
//! muzon/library.json
//! ```
//!
//! `library.json` carries what the files themselves don't: favourites, play
//! counts and playlists. It is merged rather than overwritten, so two machines
//! syncing to the same bucket keep each other's likes.

use super::client::{RemoteObject, S3Client};
use crate::domain::S3Config;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const TRACKS_DIR: &str = "tracks/";
pub const LIBRARY_FILE: &str = "library.json";

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    pub uploaded: u32,
    pub downloaded: u32,
    pub up_to_date: u32,
    pub bytes_up: f64,
    pub bytes_down: f64,
    /// Existing object keys with differing content. Count with `conflicts.len()`.
    /// Also included in failures for clients predating this field.
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// Files that failed, with the reason - the rest of the sync still runs
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    /// "scan" | "upload" | "download" | "library" | "done"
    pub phase: String,
    pub done: u32,
    pub total: u32,
    pub current: String,
}

/// One local file and where it belongs in the bucket.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalFile {
    pub path: PathBuf,
    pub key: String,
    pub size: u64,
}

#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    pub uploads: Vec<LocalFile>,
    pub downloads: Vec<(String, PathBuf, u64)>,
    pub up_to_date: u32,
    pub comparisons: Vec<LocalFile>,
}

/// The key a local file gets: the name of its library folder, then the path
/// inside it. Keeps the bucket readable and lets another machine put the files
/// back where they belong.
pub fn key_for(prefix: &str, root: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(root).ok()?;
    let root_name = root.file_name()?.to_string_lossy().to_string();
    let rel = rel.to_string_lossy().replace('\\', "/");
    Some(format!("{prefix}{TRACKS_DIR}{root_name}/{rel}"))
}

/// Where a remote key lands locally: into the library folder of the same name
/// when there is one, otherwise under the first folder, keeping the structure.
pub fn local_path_for(prefix: &str, key: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let rest = key.strip_prefix(prefix)?.strip_prefix(TRACKS_DIR)?;
    let (root_name, rel) = rest.split_once('/')?;
    if root_name.is_empty()
        || matches!(root_name, "." | "..")
        || root_name.contains('\\')
        || rel.is_empty()
        || rel.contains('\\')
        || !Path::new(rel)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return None;
    }

    let matching = roots
        .iter()
        .find(|r| r.file_name().map(|n| n == root_name).unwrap_or(false));
    match matching {
        Some(root) => Some(root.join(rel)),
        None => roots.first().map(|root| root.join(root_name).join(rel)),
    }
    .filter(|path| supported_file(path))
}

/// Size and ETags cannot prove content equality (tags can change in place,
/// and multipart/encrypted ETags are opaque). Existing keys require SHA-256
/// comparison; differences are conflicts, never implicit replacements.
pub fn plan(prefix: &str, local: &[LocalFile], remote: &[RemoteObject], roots: &[PathBuf]) -> Plan {
    let remote_by_key: HashMap<&str, &RemoteObject> =
        remote.iter().map(|o| (o.key.as_str(), o)).collect();
    let local_by_key: HashMap<&str, &LocalFile> =
        local.iter().map(|f| (f.key.as_str(), f)).collect();

    let mut out = Plan::default();

    for file in local {
        match remote_by_key.get(file.key.as_str()) {
            Some(_) => out.comparisons.push(file.clone()),
            None => out.uploads.push(file.clone()),
        }
    }

    for obj in remote {
        if !obj.key.starts_with(&format!("{prefix}{TRACKS_DIR}"))
            || local_by_key.contains_key(obj.key.as_str())
        {
            continue;
        }
        if let Some(dest) = local_path_for(prefix, &obj.key, roots) {
            if !dest.exists() {
                out.downloads.push((obj.key.clone(), dest, obj.size));
            }
        }
    }

    out
}

/// Hash asynchronously in bounded chunks, so cancellation can interrupt large files.
async fn local_hash(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path).await?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex::encode(hash.finalize()))
}

pub(super) async fn same_content(client: &S3Client, file: &LocalFile) -> anyhow::Result<bool> {
    // Hash both sides rather than interpreting ETags as checksums. No manifest
    // can become stale or be trusted after another client changes an object.
    Ok(local_hash(&file.path).await? == client.content_hash(&file.key).await?)
}

// --- library snapshot -------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotTrack {
    pub key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub play_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPlaylist {
    pub name: String,
    pub tracks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: u32,
    pub exported_at: f64,
    #[serde(default)]
    pub tracks: Vec<SnapshotTrack>,
    #[serde(default)]
    pub playlists: Vec<SnapshotPlaylist>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            version: 1,
            exported_at: 0.0,
            tracks: Vec::new(),
            playlists: Vec::new(),
        }
    }
}

/// Neither side wins outright: a like is a like wherever it was made, the
/// larger play count is the true one, and playlists are unions.
pub fn merge(local: &Snapshot, remote: &Snapshot) -> Snapshot {
    let mut tracks: HashMap<String, SnapshotTrack> = HashMap::new();

    for t in local.tracks.iter().chain(remote.tracks.iter()) {
        tracks
            .entry(t.key.clone())
            .and_modify(|existing| {
                existing.is_favorite |= t.is_favorite;
                existing.play_count = existing.play_count.max(t.play_count);
                if existing.title.is_empty() {
                    existing.title = t.title.clone();
                }
                if existing.artist.is_none() {
                    existing.artist = t.artist.clone();
                }
                if existing.album.is_none() {
                    existing.album = t.album.clone();
                }
            })
            .or_insert_with(|| t.clone());
    }

    let mut playlists: Vec<SnapshotPlaylist> = Vec::new();
    for p in local.playlists.iter().chain(remote.playlists.iter()) {
        match playlists.iter_mut().find(|x| x.name == p.name) {
            Some(existing) => {
                for key in &p.tracks {
                    if !existing.tracks.contains(key) {
                        existing.tracks.push(key.clone());
                    }
                }
            }
            None => playlists.push(p.clone()),
        }
    }

    let mut tracks: Vec<SnapshotTrack> = tracks.into_values().collect();
    tracks.sort_by(|a, b| a.key.cmp(&b.key));
    playlists.sort_by(|a, b| a.name.cmp(&b.name));

    Snapshot {
        version: 1,
        exported_at: local.exported_at.max(remote.exported_at),
        tracks,
        playlists,
    }
}

pub fn library_key(cfg: &S3Config) -> String {
    format!("{}{LIBRARY_FILE}", cfg.normalised_prefix())
}

pub async fn fetch_snapshot(client: &S3Client, cfg: &S3Config) -> anyhow::Result<Snapshot> {
    match client.get_bytes(&library_key(cfg)).await? {
        Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
        None => Ok(Snapshot::default()),
    }
}

// --- running a sync ---------------------------------------------------------

use crate::data::db::Db;

fn progress(
    callback: &(dyn Fn(SyncProgress) + Send + Sync),
    phase: &str,
    done: u32,
    total: u32,
    current: &str,
) {
    callback(SyncProgress {
        phase: phase.into(),
        done,
        total,
        current: current.into(),
    });
}
/// Every track the library knows about that still exists on disk.
fn supported_file(path: &Path) -> bool {
    crate::data::scanner::is_audio(path)
        || path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lrc"))
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
fn local_files(db: &Db, prefix: &str, roots: &[PathBuf]) -> Vec<LocalFile> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for track in db.list_tracks().unwrap_or_default() {
        let audio = PathBuf::from(&track.path);
        let mut files = vec![audio.clone(), audio.with_extension("lrc")];
        if let Some(parent) = audio.parent() {
            for name in [
                "cover.jpg",
                "cover.jpeg",
                "cover.png",
                "folder.jpg",
                "folder.jpeg",
                "folder.png",
                "front.jpg",
                "front.png",
            ] {
                files.push(parent.join(name));
            }
        }
        for path in files {
            if !seen.insert(path.clone()) || !path.is_file() {
                continue;
            }
            if let Some(root) = roots.iter().find(|root| path.starts_with(root)) {
                if let (Ok(meta), Some(key)) =
                    (std::fs::metadata(&path), key_for(prefix, root, &path))
                {
                    out.push(LocalFile {
                        path,
                        key,
                        size: meta.len(),
                    });
                }
            }
        }
    }
    out
}

fn snapshot_from_db(db: &Db, prefix: &str, roots: &[PathBuf]) -> Snapshot {
    let mut tracks = Vec::new();
    for track in db.list_tracks().unwrap_or_default() {
        let path = PathBuf::from(&track.path);
        let Some(key) = roots.iter().find_map(|root| key_for(prefix, root, &path)) else {
            continue;
        };
        tracks.push(SnapshotTrack {
            key,
            title: track.title,
            artist: track.artist,
            album: track.album,
            is_favorite: track.is_favorite,
            play_count: track.play_count,
        });
    }

    let mut playlists = Vec::new();
    for (id, name, _) in db.list_playlists().unwrap_or_default() {
        let keys = db
            .playlist_tracks(id)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|t| {
                let path = PathBuf::from(&t.path);
                roots.iter().find_map(|root| key_for(prefix, root, &path))
            })
            .collect();
        playlists.push(SnapshotPlaylist { name, tracks: keys });
    }

    Snapshot {
        version: 1,
        exported_at: super::sigv4::now_unix() as f64,
        tracks,
        playlists,
    }
}

/// Applies what came from the other machine: likes, play counts and playlists.
/// Nothing is removed - a track missing from the snapshot simply stays as is.
fn apply_snapshot(db: &Db, prefix: &str, roots: &[PathBuf], snap: &Snapshot) {
    let path_for = |key: &str| local_path_for(prefix, key, roots);

    for t in &snap.tracks {
        let Some(path) = path_for(&t.key) else {
            continue;
        };
        let path = path.to_string_lossy().to_string();
        if t.is_favorite {
            let _ = db.set_favorite_by_path(&path, true);
        }
        if t.play_count > 0 {
            let _ = db.raise_play_count_by_path(&path, t.play_count);
        }
    }

    for p in &snap.playlists {
        let id = match db.playlist_id_by_name(&p.name) {
            Ok(Some(id)) => id,
            Ok(None) => match db.create_playlist(&p.name) {
                Ok(id) => id,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        let existing: Vec<String> = db
            .playlist_tracks(id)
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.path)
            .collect();

        for key in &p.tracks {
            let Some(path) = path_for(key) else { continue };
            let path = path.to_string_lossy().to_string();
            if existing.contains(&path) {
                continue;
            }
            if let Ok(Some(track)) = db.track_by_path(&path) {
                let _ = db.add_track_to_playlist(id, track.id);
            }
        }
    }
}

/// The whole sync: compare, copy both ways, then merge the library snapshot.
/// Individual failures are collected and reported, never fatal.
pub async fn run(
    db: std::sync::Arc<Db>,
    cfg: S3Config,
    creds: super::sigv4::Credentials,
    callback: std::sync::Arc<dyn Fn(SyncProgress) + Send + Sync>,
    scan: std::sync::Arc<dyn Fn() -> anyhow::Result<()> + Send + Sync>,
) -> anyhow::Result<SyncOutcome> {
    if !cfg.is_configured() {
        anyhow::bail!("хранилище не настроено: укажите бакет");
    }
    let client = S3Client::new(cfg.clone(), creds)?;
    let prefix = cfg.normalised_prefix();

    let roots = db.list_roots()?;
    anyhow::ensure!(!roots.is_empty(), "Сначала добавьте папку библиотеки");
    let local = local_files(&db, &prefix, &roots);

    progress(callback.as_ref(), "scan", 0, 0, "");
    let remote = client.list(&prefix).await?;
    let plan = plan(&prefix, &local, &remote, &roots);

    let mut outcome = SyncOutcome {
        up_to_date: plan.up_to_date,
        ..Default::default()
    };

    let total = (plan.uploads.len() + plan.downloads.len() + plan.comparisons.len()) as u32;
    let mut done = 0;

    for file in &plan.comparisons {
        // Keep the native UI's existing comparison stage; unknown phase strings
        // currently render as "finishing" in older clients.
        progress(callback.as_ref(), "scan", done, total, &file.key);
        match same_content(&client, file).await {
            Ok(true) => outcome.up_to_date += 1,
            Ok(false) => {
                outcome.conflicts.push(file.key.clone());
                outcome.failures.push(format!(
                    "{}: content conflict; both copies preserved",
                    file.key
                ));
            }
            Err(error) => outcome
                .failures
                .push(format!("{}: comparison failed: {error}", file.key)),
        }
        done += 1;
    }

    if !outcome.conflicts.is_empty() {
        // Existing UI displays the first ten failures: reserve the first line
        // for a total so a large conflict set cannot look like a clean success.
        outcome.failures.insert(
            0,
            format!(
                "Конфликтов содержимого: {}. Копии сохранены; требуется ручное разрешение.",
                outcome.conflicts.len()
            ),
        );
    }

    for file in &plan.uploads {
        let name = file
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        progress(callback.as_ref(), "upload", done, total, &name);
        match client.put_file_if_absent(&file.key, &file.path).await {
            Ok(()) => {
                outcome.uploaded += 1;
                outcome.bytes_up += file.size as f64;
            }
            Err(e) => outcome.failures.push(format!("{name}: {e}")),
        }
        done += 1;
    }

    let mut downloaded_any = false;
    for (key, dest, size) in &plan.downloads {
        let name = dest
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        progress(callback.as_ref(), "download", done, total, &name);
        let safe = roots.iter().any(|root| {
            if !dest.starts_with(root) {
                return false;
            }
            let Ok(root) = root.canonicalize() else {
                return false;
            };
            dest.parent()
                .and_then(|parent| parent.ancestors().find(|p| p.exists()))
                .and_then(|ancestor| ancestor.canonicalize().ok())
                .is_some_and(|ancestor| ancestor.starts_with(&root))
        });
        if !safe {
            outcome.failures.push(format!(
                "{name}: папка недоступна или путь выходит за пределы библиотеки"
            ));
            done += 1;
            continue;
        }
        match client.get_file(key, dest).await {
            Ok(()) => {
                outcome.downloaded += 1;
                outcome.bytes_down += *size as f64;
                downloaded_any = true;
            }
            Err(e) => outcome.failures.push(format!("{name}: {e}")),
        }
        done += 1;
    }

    // New files have to reach the library before the snapshot can point at them
    if downloaded_any {
        progress(callback.as_ref(), "library", done, total, "");
        let scan = scan.clone();
        tokio::task::spawn_blocking(move || scan()).await??;
    }

    progress(callback.as_ref(), "library", done, total, LIBRARY_FILE);
    let remote_snapshot = fetch_snapshot(&client, &cfg).await?;
    let merged = {
        let local_snapshot = snapshot_from_db(&db, &prefix, &roots);
        let merged = merge(&local_snapshot, &remote_snapshot);
        apply_snapshot(&db, &prefix, &roots, &merged);
        merged
    };
    client
        .put_bytes(&library_key(&cfg), serde_json::to_vec_pretty(&merged)?)
        .await?;

    progress(callback.as_ref(), "done", total, total, "");
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(key: &str, size: u64) -> RemoteObject {
        RemoteObject {
            key: key.into(),
            size,
            etag: String::new(),
        }
    }

    #[test]
    fn keys_keep_the_folder_name_and_structure() {
        let key = key_for(
            "muzon/",
            Path::new("/home/u/Music"),
            Path::new("/home/u/Music/Camellia/track.flac"),
        );
        assert_eq!(
            key.as_deref(),
            Some("muzon/tracks/Music/Camellia/track.flac")
        );
    }

    #[test]
    fn a_key_maps_back_to_the_folder_it_came_from() {
        let roots = vec![
            PathBuf::from("/home/u/Music"),
            PathBuf::from("/mnt/big/Lossless"),
        ];
        assert_eq!(
            local_path_for("muzon/", "muzon/tracks/Lossless/a/b.flac", &roots),
            Some(PathBuf::from("/mnt/big/Lossless/a/b.flac"))
        );
    }

    #[test]
    fn an_unknown_folder_lands_under_the_first_one() {
        let roots = vec![PathBuf::from("/home/u/Music")];
        assert_eq!(
            local_path_for("muzon/", "muzon/tracks/FromLaptop/a.mp3", &roots),
            Some(PathBuf::from("/home/u/Music/FromLaptop/a.mp3"))
        );
    }

    #[test]
    fn refuses_keys_that_climb_out_of_the_library() {
        let roots = vec![PathBuf::from("/home/u/Music")];
        assert_eq!(
            local_path_for("muzon/", "muzon/tracks/x/../../etc/passwd", &roots),
            None
        );
        assert_eq!(
            local_path_for("muzon/", "other/tracks/x/a.mp3", &roots),
            None
        );
    }

    #[test]
    fn plans_both_directions_and_skips_matching_files() {
        let roots = vec![PathBuf::from("/home/u/Music")];
        let local = vec![
            LocalFile {
                path: "/home/u/Music/same.flac".into(),
                key: "muzon/tracks/Music/same.flac".into(),
                size: 100,
            },
            LocalFile {
                path: "/home/u/Music/new.flac".into(),
                key: "muzon/tracks/Music/new.flac".into(),
                size: 200,
            },
        ];
        let remote = vec![
            obj("muzon/tracks/Music/same.flac", 100),
            obj("muzon/tracks/Music/theirs.flac", 300),
            obj("muzon/library.json", 10),
        ];

        let plan = plan("muzon/", &local, &remote, &roots);
        assert_eq!(plan.up_to_date, 0);
        assert_eq!(plan.comparisons.len(), 1);
        assert_eq!(plan.uploads.len(), 1);
        assert_eq!(plan.uploads[0].key, "muzon/tracks/Music/new.flac");
        assert_eq!(plan.downloads.len(), 1);
        assert_eq!(plan.downloads[0].0, "muzon/tracks/Music/theirs.flac");
        assert_eq!(
            plan.downloads[0].1,
            PathBuf::from("/home/u/Music/theirs.flac")
        );
    }

    #[test]
    fn a_file_of_a_different_size_requires_comparison_without_overwrite() {
        let local = vec![LocalFile {
            path: "/m/a.flac".into(),
            key: "p/tracks/m/a.flac".into(),
            size: 120,
        }];
        let plan = plan("p/", &local, &[obj("p/tracks/m/a.flac", 100)], &[]);
        assert!(plan.uploads.is_empty());
        assert_eq!(plan.comparisons.len(), 1);
        assert_eq!(plan.up_to_date, 0);
    }

    #[test]
    fn merging_keeps_likes_from_both_machines() {
        let local = Snapshot {
            exported_at: 10.0,
            tracks: vec![
                SnapshotTrack {
                    key: "k1".into(),
                    is_favorite: true,
                    play_count: 5,
                    title: "A".into(),
                    ..Default::default()
                },
                SnapshotTrack {
                    key: "k2".into(),
                    play_count: 1,
                    ..Default::default()
                },
            ],
            playlists: vec![SnapshotPlaylist {
                name: "Ночь".into(),
                tracks: vec!["k1".into()],
            }],
            ..Default::default()
        };
        let remote = Snapshot {
            exported_at: 20.0,
            tracks: vec![
                SnapshotTrack {
                    key: "k1".into(),
                    play_count: 9,
                    ..Default::default()
                },
                SnapshotTrack {
                    key: "k3".into(),
                    is_favorite: true,
                    ..Default::default()
                },
            ],
            playlists: vec![
                SnapshotPlaylist {
                    name: "Ночь".into(),
                    tracks: vec!["k3".into(), "k1".into()],
                },
                SnapshotPlaylist {
                    name: "Дорога".into(),
                    tracks: vec!["k2".into()],
                },
            ],
            ..Default::default()
        };

        let merged = merge(&local, &remote);
        let k1 = merged.tracks.iter().find(|t| t.key == "k1").unwrap();
        assert!(k1.is_favorite, "лайк с этой машины должен сохраниться");
        assert_eq!(k1.play_count, 9, "берётся больший счётчик");
        assert_eq!(k1.title, "A", "название не теряется");
        assert!(merged.tracks.iter().any(|t| t.key == "k3" && t.is_favorite));
        assert_eq!(merged.tracks.len(), 3);

        let night = merged.playlists.iter().find(|p| p.name == "Ночь").unwrap();
        assert_eq!(night.tracks, vec!["k1", "k3"], "объединение без дублей");
        assert_eq!(merged.playlists.len(), 2);
        assert_eq!(merged.exported_at, 20.0);
    }

    #[test]
    fn a_broken_snapshot_reads_as_empty_rather_than_failing() {
        let parsed: Snapshot = serde_json::from_str("{\"version\":1,\"exportedAt\":0}").unwrap();
        assert!(parsed.tracks.is_empty());
        assert!(parsed.playlists.is_empty());
    }
}
