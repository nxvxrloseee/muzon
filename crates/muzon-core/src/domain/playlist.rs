//! Playlist membership uses library ids; track metadata stays in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub id: i32,
    pub name: String,
    pub track_ids: Vec<i32>,
}
