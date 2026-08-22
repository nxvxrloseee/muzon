use crate::data::db::Db;
use crate::domain::Track;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Serialize, Type)]
pub struct Playlist {
    pub id: i32,
    pub name: String,
    pub track_count: i32,
}

pub fn list_playlists(db: &Db) -> anyhow::Result<Vec<Playlist>> {
    Ok(db
        .list_playlists()?
        .into_iter()
        .map(|(id, name, track_count)| Playlist {
            id,
            name,
            track_count,
        })
        .collect())
}

pub fn create_playlist(db: &Db, name: &str) -> anyhow::Result<Playlist> {
    let id = db.create_playlist(name)?;
    Ok(Playlist {
        id,
        name: name.to_string(),
        track_count: 0,
    })
}

pub fn rename_playlist(db: &Db, id: i32, name: &str) -> anyhow::Result<()> {
    Ok(db.rename_playlist(id, name)?)
}

pub fn delete_playlist(db: &Db, id: i32) -> anyhow::Result<()> {
    Ok(db.delete_playlist(id)?)
}

pub fn playlist_tracks(db: &Db, playlist_id: i32) -> anyhow::Result<Vec<Track>> {
    Ok(db.playlist_tracks(playlist_id)?)
}

pub fn add_track(db: &Db, playlist_id: i32, track_id: i32) -> anyhow::Result<()> {
    Ok(db.add_track_to_playlist(playlist_id, track_id)?)
}

pub fn remove_track(db: &Db, playlist_id: i32, track_id: i32) -> anyhow::Result<()> {
    Ok(db.remove_track_from_playlist(playlist_id, track_id)?)
}

pub fn reorder_tracks(db: &Db, playlist_id: i32, track_ids: &[i32]) -> anyhow::Result<()> {
    Ok(db.reorder_playlist_tracks(playlist_id, track_ids)?)
}
