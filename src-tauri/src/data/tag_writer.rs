use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag, TagExt};
use std::path::Path;

pub struct TagEdit<'a> {
    pub title: &'a str,
    pub artist: Option<&'a str>,
    pub album: Option<&'a str>,
    pub track_no: Option<i32>,
    /// (mime type, image bytes) - replaces any existing embedded cover(s).
    pub cover: Option<(&'a str, &'a [u8])>,
}

pub fn write_tags(path: &Path, edit: &TagEdit) -> anyhow::Result<()> {
    let mut tagged_file = Probe::open(path)?.read()?;
    let tag_type = tagged_file.primary_tag_type();

    if tagged_file.primary_tag().is_none() {
        tagged_file.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged_file
        .primary_tag_mut()
        .expect("a primary tag was just inserted if missing");

    tag.set_title(edit.title.to_string());

    match edit.artist {
        Some(a) if !a.is_empty() => tag.set_artist(a.to_string()),
        _ => tag.remove_artist(),
    }
    match edit.album {
        Some(a) if !a.is_empty() => tag.set_album(a.to_string()),
        _ => tag.remove_album(),
    }
    match edit.track_no {
        Some(n) if n > 0 => tag.set_track(n as u32),
        _ => tag.remove_track(),
    }

    if let Some((mime, bytes)) = edit.cover {
        while !tag.pictures().is_empty() {
            tag.remove_picture(0);
        }
        let mime_type = if mime == "image/png" {
            MimeType::Png
        } else {
            MimeType::Jpeg
        };
        tag.push_picture(Picture::new_unchecked(
            PictureType::CoverFront,
            Some(mime_type),
            None,
            bytes.to_vec(),
        ));
    }

    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

/// Writes LRC text into the track's tag (ID3v2 USLT / Vorbis Comment LYRICS /
/// MP4 ©lyr, all exposed by lofty as the generic `ItemKey::Lyrics`), preserving
/// every other tag field untouched.
pub fn write_lyrics_to_tag(path: &Path, lrc_text: &str) -> anyhow::Result<()> {
    let mut tagged_file = Probe::open(path)?.read()?;
    let tag_type = tagged_file.primary_tag_type();

    if tagged_file.primary_tag().is_none() {
        tagged_file.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged_file
        .primary_tag_mut()
        .expect("a primary tag was just inserted if missing");

    tag.insert_text(ItemKey::Lyrics, lrc_text.to_string());
    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}
