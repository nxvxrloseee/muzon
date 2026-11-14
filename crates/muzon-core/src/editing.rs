//! Explicit file edits. Writes use a sibling temporary file and atomic replacement.
use crate::{
    data::{
        db::{Db, NewTrack},
        tag_writer, tags,
    },
    domain::{library::TrackEditInput, Track},
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::UNIX_EPOCH,
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
#[derive(Debug, Clone)]
pub struct EditorSource {
    pub track: Track,
    pub lyrics: String,
    pub lyrics_in_tag: bool,
}
#[derive(Debug, Clone)]
pub enum EditRequest {
    Tags(TrackEditInput),
    Lyrics { text: String, in_tag: bool },
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn temporary(target: &Path) -> anyhow::Result<Temporary> {
    let name = target
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("invalid file name"))?
        .to_string_lossy();
    for _ in 0..32 {
        let path = target.with_file_name(format!(
            ".{name}.muzon-{}-{}.tmp",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return Ok(Temporary(path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("cannot allocate temporary edit file")
}
pub fn save(db: &Db, id: i32, request: EditRequest) -> anyhow::Result<Track> {
    let track = db
        .track_by_id(id)?
        .ok_or_else(|| anyhow::anyhow!("Трек больше не существует"))?;
    let original = Path::new(&track.path);
    let (target, sidecar) = match &request {
        EditRequest::Lyrics { in_tag: false, .. } => {
            let sidecar = original.with_extension("lrc");
            let target = if sidecar.exists() {
                sidecar.canonicalize()?
            } else {
                sidecar
            };
            (target, true)
        }
        _ => (original.canonicalize()?, false),
    };
    let before = std::fs::metadata(&target)
        .ok()
        .map(|m| (m.len(), m.modified().ok()));
    let temp = temporary(&target)?;
    if sidecar {
        let EditRequest::Lyrics { text, .. } = &request else {
            unreachable!()
        };
        anyhow::ensure!(text.len() <= 1024 * 1024, "Текст превышает 1 МиБ");
        std::fs::write(&temp.0, text)?;
        if let Ok(metadata) = std::fs::metadata(&target) {
            std::fs::set_permissions(&temp.0, metadata.permissions())?;
        }
    } else {
        std::fs::copy(&target, &temp.0)?;
        match &request {
            EditRequest::Tags(edit) => {
                anyhow::ensure!(!edit.title.trim().is_empty(), "Укажите название трека");
                anyhow::ensure!(
                    edit.track_no.is_none_or(|n| n > 0)
                        && edit.year.is_none_or(|n| (1..=9999).contains(&n)),
                    "Некорректный номер трека или год"
                );
                let cover = edit.cover_path.as_ref().map(std::fs::read).transpose()?;
                let mime = if let Some(bytes) = &cover {
                    anyhow::ensure!(bytes.len() <= 16 * 1024 * 1024, "Обложка превышает 16 МиБ");
                    image::load_from_memory(bytes)?;
                    match image::guess_format(bytes)? {
                        image::ImageFormat::Png => "image/png",
                        image::ImageFormat::Jpeg => "image/jpeg",
                        _ => anyhow::bail!("Используйте обложку PNG или JPEG"),
                    }
                } else {
                    "image/png"
                };
                tag_writer::write_tags(
                    &temp.0,
                    &tag_writer::TagEdit {
                        title: &edit.title,
                        artist: edit.artist.as_deref(),
                        album: edit.album.as_deref(),
                        track_no: edit.track_no,
                        genre: edit.genre.as_deref(),
                        year: edit.year,
                        cover: cover.as_ref().map(|bytes| (mime, bytes.as_slice())),
                    },
                )?;
            }
            EditRequest::Lyrics { text, .. } => {
                anyhow::ensure!(text.len() <= 1024 * 1024, "Текст превышает 1 МиБ");
                tag_writer::write_lyrics_to_tag(&temp.0, text)?;
            }
        }
        let saved = tags::read_tags(&temp.0)?;
        match &request {
            EditRequest::Tags(edit) => {
                anyhow::ensure!(
                    saved.title == edit.title
                        && saved.artist == edit.artist
                        && saved.album == edit.album
                        && saved.track_no == edit.track_no
                        && saved.genre == edit.genre
                        && saved.year == edit.year,
                    "Формат файла не сохранил все поля тегов; исходный файл не изменён"
                );
            }
            EditRequest::Lyrics { text, .. } => {
                anyhow::ensure!(
                    embedded_text(&temp.0).as_deref() == Some(text.as_str()),
                    "Формат не поддерживает встроенный текст; выберите файл .lrc"
                );
            }
        }
    }
    anyhow::ensure!(
        before
            == std::fs::metadata(&target)
                .ok()
                .map(|m| (m.len(), m.modified().ok())),
        "Файл изменён другой программой; откройте редактор заново"
    );
    std::fs::File::open(&temp.0)?.sync_all()?;
    std::fs::rename(&temp.0, &target)?;
    if !sidecar {
        let data = tags::read_tags(original)?;
        let mtime = std::fs::metadata(original)?
            .modified()?
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64;
        db.upsert_track(&NewTrack {
            path: track.path.clone(),
            title: data.title,
            artist: data.artist,
            album: data.album,
            duration_secs: data.duration_secs,
            track_no: data.track_no,
            genre: data.genre,
            year: data.year,
            mtime,
        })
        .map_err(|e| anyhow::anyhow!("Файл сохранён, но обновить библиотеку не удалось: {e}"))?;
    }
    db.track_by_id(id)?
        .ok_or_else(|| anyhow::anyhow!("Записано в файл, но трек отсутствует в библиотеке"))
}
fn embedded_text(path: &Path) -> Option<String> {
    use lofty::{file::TaggedFileExt, tag::ItemKey};
    let file = lofty::probe::Probe::open(path)
        .ok()?
        .guess_file_type()
        .ok()?
        .read()
        .ok()?;
    file.primary_tag()
        .or_else(|| file.first_tag())?
        .get_string(&ItemKey::Lyrics)
        .map(str::to_owned)
}
pub fn load(db: &Db, id: i32) -> anyhow::Result<EditorSource> {
    let track = db
        .track_by_id(id)?
        .ok_or_else(|| anyhow::anyhow!("Трек больше не существует"))?;
    use lofty::{file::TaggedFileExt, tag::ItemKey};
    let path = Path::new(&track.path);
    let file = lofty::probe::Probe::open(path)?.read()?;
    let embedded = file
        .primary_tag()
        .or_else(|| file.first_tag())
        .and_then(|tag| tag.get_string(&ItemKey::Lyrics))
        .filter(|text| !text.is_empty())
        .map(str::to_owned);
    let lyrics_in_tag = embedded.is_some();
    let lyrics = if let Some(embedded) = embedded {
        embedded
    } else {
        let sidecar = path.with_extension("lrc");
        match std::fs::metadata(&sidecar) {
            Ok(metadata) => {
                anyhow::ensure!(metadata.len() <= 1024 * 1024, "Текст превышает 1 МиБ");
                std::fs::read_to_string(&sidecar)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        }
    };
    anyhow::ensure!(lyrics.len() <= 1024 * 1024, "Текст превышает 1 МиБ");
    Ok(EditorSource {
        track,
        lyrics,
        lyrics_in_tag,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::{lrc_source, scanner},
        testing::{scratch_dir, write_tone},
    };
    fn fixture(name: &str) -> (PathBuf, Db, i32) {
        let dir = scratch_dir(name);
        let music = dir.join("music");
        write_tone(&music, "song.wav", 1.0);
        let db = Db::open(&dir.join("test.sqlite")).unwrap();
        db.add_root(&music).unwrap();
        scanner::scan_roots(&db, &db.list_roots().unwrap()).unwrap();
        let id = db.list_tracks().unwrap()[0].id;
        (dir, db, id)
    }
    fn tags() -> TrackEditInput {
        TrackEditInput {
            title: "Edited title".into(),
            artist: Some("Artist".into()),
            album: Some("Album".into()),
            track_no: None,
            genre: None,
            year: None,
            cover_path: None,
        }
    }
    #[test]
    fn tags_replace_atomically_and_preserve_library_identity_and_preferences() {
        let (dir, db, id) = fixture("edit-tags");
        db.toggle_favorite(id).unwrap();
        db.set_track_tempo(id, 1.2).unwrap();
        db.record_play(id).unwrap();
        let track = save(&db, id, EditRequest::Tags(tags())).unwrap();
        assert_eq!(track.id, id);
        assert_eq!(track.title, "Edited title");
        assert!(track.is_favorite);
        assert_eq!(track.tempo, 1.2);
        assert_eq!(track.play_count, 1);
        assert_eq!(
            tags::read_tags(Path::new(&track.path))
                .unwrap()
                .artist
                .as_deref(),
            Some("Artist")
        );
        assert_eq!(std::fs::read_dir(dir.join("music")).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn invalid_cover_keeps_original_bytes_and_cleans_temporary() {
        let (dir, db, id) = fixture("edit-invalid-cover");
        let path = db.track_by_id(id).unwrap().unwrap().path;
        let before = std::fs::read(&path).unwrap();
        let cover = dir.join("invalid.png");
        std::fs::write(&cover, "invalid").unwrap();
        let mut edit = tags();
        edit.cover_path = Some(cover.to_string_lossy().into());
        assert!(save(&db, id, EditRequest::Tags(edit)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(db.track_by_id(id).unwrap().unwrap().title, "song");
        assert_eq!(std::fs::read_dir(dir.join("music")).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn sidecar_roundtrips_without_touching_audio() {
        let (dir, db, id) = fixture("edit-sidecar");
        let path = db.track_by_id(id).unwrap().unwrap().path;
        let before = std::fs::read(&path).unwrap();
        let text = "[00:00.00]First line\n[00:00.50]Second line\n";
        save(
            &db,
            id,
            EditRequest::Lyrics {
                text: text.into(),
                in_tag: false,
            },
        )
        .unwrap();
        let source = load(&db, id).unwrap();
        assert_eq!(source.lyrics, text);
        assert!(!source.lyrics_in_tag);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(
            lrc_source::load_for_track(Path::new(&path))
                .unwrap()
                .lines
                .len(),
            2
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(all(test, unix))]
mod symlink_tests {
    use super::*;
    use crate::testing::{scratch_dir, write_tone};
    #[test]
    fn editing_a_symlink_preserves_the_link_and_updates_its_target() {
        let dir = scratch_dir("edit-symlink");
        let target = write_tone(&dir.join("outside"), "real.wav", 1.0);
        let music = dir.join("music");
        std::fs::create_dir_all(&music).unwrap();
        let link = music.join("link.wav");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let db = Db::open(&dir.join("test.sqlite")).unwrap();
        // Existing profiles can contain symlink paths even though the scanner
        // deliberately skips links during discovery.
        db.upsert_track(&NewTrack {
            path: link.to_string_lossy().into(),
            title: "real".into(),
            artist: None,
            album: None,
            duration_secs: Some(1.0),
            track_no: None,
            genre: None,
            year: None,
            mtime: 0,
        })
        .unwrap();
        let id = db.list_tracks().unwrap()[0].id;
        save(
            &db,
            id,
            EditRequest::Tags(TrackEditInput {
                title: "Linked edit".into(),
                artist: None,
                album: None,
                track_no: None,
                genre: None,
                year: None,
                cover_path: None,
            }),
        )
        .unwrap();
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(tags::read_tags(&target).unwrap().title, "Linked edit");
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    use crate::{
        data::scanner,
        testing::{scratch_dir, write_tone},
    };
    #[test]
    fn unreadable_text_is_an_error_instead_of_an_empty_draft() {
        let dir = scratch_dir("edit-invalid-text");
        let path = write_tone(&dir.join("music"), "track.wav", 1.0);
        let db = Db::open(&dir.join("db.sqlite")).unwrap();
        db.add_root(&dir.join("music")).unwrap();
        scanner::scan_roots(&db, &db.list_roots().unwrap()).unwrap();
        let id = db.list_tracks().unwrap()[0].id;
        std::fs::write(path.with_extension("lrc"), [0xff, 0xfe, 0xfd]).unwrap();
        assert!(load(&db, id).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
