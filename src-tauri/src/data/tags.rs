use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;
use std::path::Path;

pub struct TagData {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
    pub track_no: Option<i32>,
}

pub fn read_tags(path: &Path) -> anyhow::Result<TagData> {
    let tagged_file = Probe::open(path)?.read()?;
    let duration_secs = Some(tagged_file.properties().duration().as_secs_f64());
    let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());

    let title = tag
        .and_then(|t| t.title())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string())
        });
    let artist = tag.and_then(|t| t.artist()).map(|s| s.to_string());
    let album = tag.and_then(|t| t.album()).map(|s| s.to_string());
    let track_no = tag.and_then(|t| t.track()).map(|n| n as i32);

    Ok(TagData {
        title,
        artist,
        album,
        duration_secs,
        track_no,
    })
}
