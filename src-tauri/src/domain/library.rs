use crate::data::db::{Db, NewTrack};
use crate::data::{scanner, tag_writer, tags};
use crate::domain::Track;
use serde::Serialize;
use specta::Type;
use std::path::Path;
use std::time::UNIX_EPOCH;

#[derive(Debug, Serialize, Clone, Default, Type)]
pub struct ScanReport {
    pub added: u32,
    pub updated: u32,
    pub removed: u32,
    pub errors: Vec<String>,
}

pub struct TrackEditInput {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_no: Option<i32>,
    /// Path to an image file to embed as the new cover; `None` leaves any
    /// existing embedded cover untouched.
    pub cover_path: Option<String>,
}

pub fn add_root_and_scan(db: &Db, root: &Path) -> anyhow::Result<ScanReport> {
    db.add_root(root)?;
    scanner::scan(db, root)
}

pub fn list_tracks(db: &Db) -> anyhow::Result<Vec<Track>> {
    Ok(db.list_tracks()?)
}

pub fn toggle_favorite(db: &Db, track_id: i32) -> anyhow::Result<bool> {
    Ok(db.toggle_favorite(track_id)?)
}

/// Writes edited tags (and optionally a new embedded cover) back to the file,
/// then re-reads the file's tags and upserts the DB row so the library
/// reflects the change immediately, without a full rescan.
pub fn update_tags(db: &Db, track_path: &Path, edit: TrackEditInput) -> anyhow::Result<Track> {
    let cover_bytes = edit.cover_path.as_ref().map(std::fs::read).transpose()?;
    let cover = match (&edit.cover_path, &cover_bytes) {
        (Some(cover_path), Some(bytes)) => {
            let mime = if cover_path.to_lowercase().ends_with(".png") {
                "image/png"
            } else {
                "image/jpeg"
            };
            Some((mime, bytes.as_slice()))
        }
        _ => None,
    };

    tag_writer::write_tags(
        track_path,
        &tag_writer::TagEdit {
            title: &edit.title,
            artist: edit.artist.as_deref(),
            album: edit.album.as_deref(),
            track_no: edit.track_no,
            cover,
        },
    )?;

    let tag_data = tags::read_tags(track_path)?;
    let mtime = std::fs::metadata(track_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let path_str = track_path.to_string_lossy().to_string();
    db.upsert_track(&NewTrack {
        path: path_str.clone(),
        title: tag_data.title,
        artist: tag_data.artist,
        album: tag_data.album,
        duration_secs: tag_data.duration_secs,
        track_no: tag_data.track_no,
        mtime,
    })?;

    db.list_tracks()?
        .into_iter()
        .find(|t| t.path == path_str)
        .ok_or_else(|| anyhow::anyhow!("track not found after tag update"))
}
