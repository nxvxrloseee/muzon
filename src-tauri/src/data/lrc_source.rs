use crate::domain::lyrics::{self, Lyrics};
use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::ItemKey;
use std::path::Path;

/// LRC lookup order: embedded tag lyrics first (ID3v2 USLT / Vorbis Comment LYRICS /
/// MP4 ©lyr, all exposed by lofty as the generic `ItemKey::Lyrics` string - taggers
/// commonly stuff full `[mm:ss.xx]`-timed LRC text into this field), then a sibling
/// `.lrc` file. Untimed plain-text lyrics in the tag parse to zero lines, so we fall
/// through to the file instead of treating that as "found".
pub fn load_for_track(track_path: &Path) -> Option<Lyrics> {
    load_embedded(track_path).or_else(|| load_sidecar_file(track_path))
}

fn load_embedded(track_path: &Path) -> Option<Lyrics> {
    let tagged_file = Probe::open(track_path).ok()?.read().ok()?;
    let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag())?;
    let text = tag.get_string(&ItemKey::Lyrics)?;
    let lyrics = lyrics::parse(text);
    (!lyrics.lines.is_empty()).then_some(lyrics)
}

fn load_sidecar_file(track_path: &Path) -> Option<Lyrics> {
    let content = read_sidecar_text(track_path)?;
    Some(lyrics::parse(&content))
}

fn read_sidecar_text(track_path: &Path) -> Option<String> {
    let lrc_path = track_path.with_extension("lrc");
    std::fs::read_to_string(&lrc_path).ok()
}

/// Raw LRC text for the editor to prefill, using the same tag-then-file
/// priority as `load_for_track`. Falls back to an empty string (a fresh
/// document) rather than `None` so the editor always has something to work
/// with, since the intent of opening the editor is usually to *start* writing
/// lyrics for a track that doesn't have any yet.
pub fn load_raw_text_for_editing(track_path: &Path) -> String {
    let embedded = Probe::open(track_path)
        .ok()
        .and_then(|p| p.read().ok())
        .and_then(|f| {
            f.primary_tag()
                .or_else(|| f.first_tag())
                .and_then(|t| t.get_string(&ItemKey::Lyrics))
                .map(|s| s.to_string())
        });

    embedded
        .filter(|text| !lyrics::parse(text).lines.is_empty())
        .or_else(|| read_sidecar_text(track_path))
        .unwrap_or_default()
}

pub fn save_sidecar_file(track_path: &Path, content: &str) -> anyhow::Result<()> {
    let lrc_path = track_path.with_extension("lrc");
    std::fs::write(&lrc_path, content)?;
    Ok(())
}
