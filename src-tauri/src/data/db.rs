use crate::domain::Track;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

pub struct NewTrack {
    pub path: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
    pub track_no: Option<i32>,
    pub mtime: i64,
}

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::from_connection(conn)
    }

    #[cfg(test)]
    fn open_in_memory() -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> rusqlite::Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS roots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS tracks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                artist TEXT,
                album TEXT,
                duration_secs REAL,
                track_no INTEGER,
                mtime INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS playlists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS playlist_tracks (
                playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                PRIMARY KEY (playlist_id, track_id)
            );",
        )?;

        let has_favorite_column = conn
            .prepare("SELECT is_favorite FROM tracks LIMIT 1")
            .is_ok();
        if !has_favorite_column {
            conn.execute(
                "ALTER TABLE tracks ADD COLUMN is_favorite INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        let has_tempo_column = conn.prepare("SELECT tempo FROM tracks LIMIT 1").is_ok();
        if !has_tempo_column {
            conn.execute(
                "ALTER TABLE tracks ADD COLUMN tempo REAL NOT NULL DEFAULT 1.0",
                [],
            )?;
        }

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn add_root(&self, path: &Path) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO roots (path) VALUES (?1)",
            [path.to_string_lossy().to_string()],
        )?;
        Ok(())
    }

    /// Paths (and mtimes) of all tracks currently known under `root`, used by the
    /// scanner to diff against the filesystem for incremental updates.
    pub fn known_tracks_under(&self, root: &Path) -> rusqlite::Result<HashMap<String, i64>> {
        let conn = self.conn.lock().unwrap();
        let prefix = root.to_string_lossy().to_string();
        let mut stmt = conn.prepare("SELECT path, mtime FROM tracks")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut map = HashMap::new();
        for r in rows {
            let (path, mtime) = r?;
            if path.starts_with(&prefix) {
                map.insert(path, mtime);
            }
        }
        Ok(map)
    }

    pub fn upsert_track(&self, t: &NewTrack) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, duration_secs, track_no, mtime)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(path) DO UPDATE SET
                title = excluded.title,
                artist = excluded.artist,
                album = excluded.album,
                duration_secs = excluded.duration_secs,
                track_no = excluded.track_no,
                mtime = excluded.mtime",
            rusqlite::params![
                t.path,
                t.title,
                t.artist,
                t.album,
                t.duration_secs,
                t.track_no,
                t.mtime
            ],
        )?;
        Ok(())
    }

    /// Commits an entire scan's worth of upserts and removals in one transaction,
    /// instead of one autocommit per file - the scanner gathers everything first
    /// (tag reads, mtime checks) with the connection unlocked, then hands the
    /// whole batch here so the lock is only held for the actual writes.
    pub fn apply_scan_results(
        &self,
        upserts: &[NewTrack],
        removed_paths: &[String],
    ) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO tracks (path, title, artist, album, duration_secs, track_no, mtime)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(path) DO UPDATE SET
                    title = excluded.title,
                    artist = excluded.artist,
                    album = excluded.album,
                    duration_secs = excluded.duration_secs,
                    track_no = excluded.track_no,
                    mtime = excluded.mtime",
            )?;
            for t in upserts {
                stmt.execute(rusqlite::params![
                    t.path,
                    t.title,
                    t.artist,
                    t.album,
                    t.duration_secs,
                    t.track_no,
                    t.mtime
                ])?;
            }
        }
        {
            let mut stmt = tx.prepare("DELETE FROM tracks WHERE path = ?1")?;
            for path in removed_paths {
                stmt.execute([path])?;
            }
        }
        tx.commit()
    }

    pub fn list_tracks(&self) -> rusqlite::Result<Vec<Track>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, title, artist, album, duration_secs, track_no, is_favorite, tempo
             FROM tracks ORDER BY artist, album, track_no, title",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2)?,
                artist: row.get(3)?,
                album: row.get(4)?,
                duration_secs: row.get(5)?,
                track_no: row.get(6)?,
                is_favorite: row.get(7)?,
                tempo: row.get(8)?,
            })
        })?;
        rows.collect()
    }

    /// Writes the per-track saved tempo. Kept separate from `upsert_track` since
    /// this is a user preference the scanner must never touch on rescan (same
    /// reasoning as `is_favorite`).
    pub fn set_track_tempo(&self, track_id: i32, tempo: f64) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tracks SET tempo = ?1 WHERE id = ?2",
            rusqlite::params![tempo, track_id],
        )?;
        Ok(())
    }

    pub fn track_tempo_by_path(&self, path: &str) -> rusqlite::Result<Option<f64>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT tempo FROM tracks WHERE path = ?1", [path], |row| {
            row.get(0)
        })
        .optional()
    }

    /// Returns the new favorite state after toggling.
    pub fn toggle_favorite(&self, track_id: i32) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tracks SET is_favorite = 1 - is_favorite WHERE id = ?1",
            [track_id],
        )?;
        conn.query_row(
            "SELECT is_favorite FROM tracks WHERE id = ?1",
            [track_id],
            |row| row.get(0),
        )
    }

    pub fn create_playlist(&self, name: &str) -> rusqlite::Result<i32> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT INTO playlists (name) VALUES (?1)", [name])?;
        Ok(conn.last_insert_rowid() as i32)
    }

    pub fn rename_playlist(&self, id: i32, name: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE playlists SET name = ?1 WHERE id = ?2",
            rusqlite::params![name, id],
        )?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: i32) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM playlists WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn list_playlists(&self) -> rusqlite::Result<Vec<(i32, String, i32)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, COUNT(pt.track_id)
             FROM playlists p
             LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id
             GROUP BY p.id
             ORDER BY p.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        rows.collect()
    }

    pub fn playlist_tracks(&self, playlist_id: i32) -> rusqlite::Result<Vec<Track>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.path, t.title, t.artist, t.album, t.duration_secs, t.track_no, t.is_favorite, t.tempo
             FROM playlist_tracks pt
             JOIN tracks t ON t.id = pt.track_id
             WHERE pt.playlist_id = ?1
             ORDER BY pt.position",
        )?;
        let rows = stmt.query_map([playlist_id], |row| {
            Ok(Track {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2)?,
                artist: row.get(3)?,
                album: row.get(4)?,
                duration_secs: row.get(5)?,
                track_no: row.get(6)?,
                is_favorite: row.get(7)?,
                tempo: row.get(8)?,
            })
        })?;
        rows.collect()
    }

    pub fn add_track_to_playlist(&self, playlist_id: i32, track_id: i32) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let next_position: i32 = conn.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM playlist_tracks WHERE playlist_id = ?1",
            [playlist_id],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![playlist_id, track_id, next_position],
        )?;
        Ok(())
    }

    pub fn remove_track_from_playlist(
        &self,
        playlist_id: i32,
        track_id: i32,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2",
            rusqlite::params![playlist_id, track_id],
        )?;
        Ok(())
    }

    /// Rewrites every row's `position` to match the given order, in one transaction.
    pub fn reorder_playlist_tracks(
        &self,
        playlist_id: i32,
        ordered_track_ids: &[i32],
    ) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for (position, track_id) in ordered_track_ids.iter().enumerate() {
            tx.execute(
                "UPDATE playlist_tracks SET position = ?1 WHERE playlist_id = ?2 AND track_id = ?3",
                rusqlite::params![position as i32, playlist_id, track_id],
            )?;
        }
        tx.commit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(path: &str, title: &str, artist: &str, album: &str, track_no: i32) -> NewTrack {
        NewTrack {
            path: path.to_string(),
            title: title.to_string(),
            artist: Some(artist.to_string()),
            album: Some(album.to_string()),
            duration_secs: Some(180.0),
            track_no: Some(track_no),
            mtime: 1000,
        }
    }

    #[test]
    fn upsert_inserts_then_updates_in_place() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "Old Title", "Artist", "Album", 1))
            .unwrap();
        let tracks = db.list_tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Old Title");

        db.upsert_track(&sample("/a.mp3", "New Title", "Artist", "Album", 1))
            .unwrap();
        let tracks = db.list_tracks().unwrap();
        assert_eq!(tracks.len(), 1, "re-scanning the same path must not duplicate the row");
        assert_eq!(tracks[0].title, "New Title");
    }

    #[test]
    fn upsert_does_not_clobber_favorite_flag_on_rescan() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "Title", "Artist", "Album", 1))
            .unwrap();
        let id = db.list_tracks().unwrap()[0].id;
        let is_favorite = db.toggle_favorite(id).unwrap();
        assert!(is_favorite);

        // Re-scanning (e.g. the user edits a tag and the scanner re-upserts the row)
        // must not reset favorites picked up along the way.
        db.upsert_track(&sample("/a.mp3", "Retitled", "Artist", "Album", 1))
            .unwrap();
        let tracks = db.list_tracks().unwrap();
        assert_eq!(tracks[0].title, "Retitled");
        assert!(tracks[0].is_favorite);
    }

    #[test]
    fn toggle_favorite_flips_and_reports_new_state() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "Title", "Artist", "Album", 1))
            .unwrap();
        let id = db.list_tracks().unwrap()[0].id;

        assert!(db.toggle_favorite(id).unwrap());
        assert!(!db.toggle_favorite(id).unwrap());
    }

    #[test]
    fn list_tracks_orders_by_artist_album_track_no() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/b2.mp3", "B Second", "Bea", "Album", 2))
            .unwrap();
        db.upsert_track(&sample("/b1.mp3", "B First", "Bea", "Album", 1))
            .unwrap();
        db.upsert_track(&sample("/a1.mp3", "A First", "Ann", "Album", 1))
            .unwrap();

        let titles: Vec<String> = db.list_tracks().unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, vec!["A First", "B First", "B Second"]);
    }

    #[test]
    fn new_tracks_default_to_tempo_one() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "A", "Artist", "Album", 1))
            .unwrap();
        assert_eq!(db.list_tracks().unwrap()[0].tempo, 1.0);
    }

    #[test]
    fn set_track_tempo_is_readable_by_path_and_survives_rescan() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "A", "Artist", "Album", 1))
            .unwrap();
        let id = db.list_tracks().unwrap()[0].id;

        db.set_track_tempo(id, 1.25).unwrap();
        assert_eq!(db.track_tempo_by_path("/a.mp3").unwrap(), Some(1.25));

        // Same reasoning as favorites: re-scanning (e.g. a tag edit) must not
        // reset a track's saved tempo back to the default.
        db.upsert_track(&sample("/a.mp3", "Retitled", "Artist", "Album", 1))
            .unwrap();
        assert_eq!(db.list_tracks().unwrap()[0].tempo, 1.25);
    }

    #[test]
    fn track_tempo_by_path_is_none_for_unknown_path() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.track_tempo_by_path("/missing.mp3").unwrap(), None);
    }

    #[test]
    fn apply_scan_results_upserts_and_removes_in_one_batch() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/stale.mp3", "Stale", "Artist", "Album", 1))
            .unwrap();

        db.apply_scan_results(
            &[
                sample("/a.mp3", "A", "Artist", "Album", 1),
                sample("/b.mp3", "B", "Artist", "Album", 2),
            ],
            &["/stale.mp3".to_string()],
        )
        .unwrap();

        let paths: Vec<String> = db.list_tracks().unwrap().into_iter().map(|t| t.path).collect();
        assert_eq!(paths, vec!["/a.mp3", "/b.mp3"]);
    }

    #[test]
    fn delete_track_by_path_removes_only_that_row() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "A", "Artist", "Album", 1))
            .unwrap();
        db.upsert_track(&sample("/b.mp3", "B", "Artist", "Album", 2))
            .unwrap();

        db.apply_scan_results(&[], &["/a.mp3".to_string()]).unwrap();

        let tracks = db.list_tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].path, "/b.mp3");
    }

    #[test]
    fn known_tracks_under_filters_by_path_prefix() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/music/a.mp3", "A", "Artist", "Album", 1))
            .unwrap();
        db.upsert_track(&sample("/other/b.mp3", "B", "Artist", "Album", 2))
            .unwrap();

        let known = db.known_tracks_under(Path::new("/music")).unwrap();
        assert_eq!(known.len(), 1);
        assert!(known.contains_key("/music/a.mp3"));
    }

    #[test]
    fn playlist_crud_round_trip() {
        let db = Db::open_in_memory().unwrap();
        let id = db.create_playlist("Chill").unwrap();
        assert_eq!(db.list_playlists().unwrap(), vec![(id, "Chill".to_string(), 0)]);

        db.rename_playlist(id, "Chill 2.0").unwrap();
        assert_eq!(db.list_playlists().unwrap()[0].1, "Chill 2.0");

        db.delete_playlist(id).unwrap();
        assert!(db.list_playlists().unwrap().is_empty());
    }

    #[test]
    fn adding_tracks_to_playlist_assigns_increasing_positions() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "A", "Artist", "Album", 1))
            .unwrap();
        db.upsert_track(&sample("/b.mp3", "B", "Artist", "Album", 2))
            .unwrap();
        let tracks = db.list_tracks().unwrap();
        let playlist_id = db.create_playlist("Mix").unwrap();

        db.add_track_to_playlist(playlist_id, tracks[0].id).unwrap();
        db.add_track_to_playlist(playlist_id, tracks[1].id).unwrap();

        let playlist_tracks = db.playlist_tracks(playlist_id).unwrap();
        assert_eq!(
            playlist_tracks.iter().map(|t| t.path.clone()).collect::<Vec<_>>(),
            vec!["/a.mp3", "/b.mp3"]
        );
        assert_eq!(db.list_playlists().unwrap()[0].2, 2, "track_count should reflect membership");
    }

    #[test]
    fn adding_same_track_twice_is_a_no_op() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "A", "Artist", "Album", 1))
            .unwrap();
        let track_id = db.list_tracks().unwrap()[0].id;
        let playlist_id = db.create_playlist("Mix").unwrap();

        db.add_track_to_playlist(playlist_id, track_id).unwrap();
        db.add_track_to_playlist(playlist_id, track_id).unwrap();

        assert_eq!(db.playlist_tracks(playlist_id).unwrap().len(), 1);
    }

    #[test]
    fn removing_track_from_playlist_leaves_others_untouched() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "A", "Artist", "Album", 1))
            .unwrap();
        db.upsert_track(&sample("/b.mp3", "B", "Artist", "Album", 2))
            .unwrap();
        let tracks = db.list_tracks().unwrap();
        let playlist_id = db.create_playlist("Mix").unwrap();
        db.add_track_to_playlist(playlist_id, tracks[0].id).unwrap();
        db.add_track_to_playlist(playlist_id, tracks[1].id).unwrap();

        db.remove_track_from_playlist(playlist_id, tracks[0].id).unwrap();

        let remaining = db.playlist_tracks(playlist_id).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].path, "/b.mp3");
    }

    #[test]
    fn reorder_playlist_tracks_changes_playback_order() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "A", "Artist", "Album", 1))
            .unwrap();
        db.upsert_track(&sample("/b.mp3", "B", "Artist", "Album", 2))
            .unwrap();
        let tracks = db.list_tracks().unwrap();
        let playlist_id = db.create_playlist("Mix").unwrap();
        db.add_track_to_playlist(playlist_id, tracks[0].id).unwrap();
        db.add_track_to_playlist(playlist_id, tracks[1].id).unwrap();

        db.reorder_playlist_tracks(playlist_id, &[tracks[1].id, tracks[0].id])
            .unwrap();

        let reordered = db.playlist_tracks(playlist_id).unwrap();
        assert_eq!(
            reordered.iter().map(|t| t.path.clone()).collect::<Vec<_>>(),
            vec!["/b.mp3", "/a.mp3"]
        );
    }

    #[test]
    fn deleting_playlist_cascades_to_playlist_tracks() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "A", "Artist", "Album", 1))
            .unwrap();
        let track_id = db.list_tracks().unwrap()[0].id;
        let playlist_id = db.create_playlist("Mix").unwrap();
        db.add_track_to_playlist(playlist_id, track_id).unwrap();

        db.delete_playlist(playlist_id).unwrap();

        // The playlist_tracks row must go with it (ON DELETE CASCADE), not linger as an
        // orphan referencing a playlist id that no longer exists.
        assert!(db.playlist_tracks(playlist_id).unwrap().is_empty());
    }

    #[test]
    fn deleting_track_cascades_out_of_playlists() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_track(&sample("/a.mp3", "A", "Artist", "Album", 1))
            .unwrap();
        let track_id = db.list_tracks().unwrap()[0].id;
        let playlist_id = db.create_playlist("Mix").unwrap();
        db.add_track_to_playlist(playlist_id, track_id).unwrap();

        db.apply_scan_results(&[], &["/a.mp3".to_string()]).unwrap();

        assert!(db.playlist_tracks(playlist_id).unwrap().is_empty());
    }
}
