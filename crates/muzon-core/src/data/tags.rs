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
    pub genre: Option<String>,
    pub year: Option<i32>,
}

pub fn read_tags(path: &Path) -> anyhow::Result<TagData> {
    let tagged_file = Probe::open(path)?.guess_file_type()?.read()?;
    let duration_secs = Some(tagged_file.properties().duration().as_secs_f64());
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

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
    let genre = tag
        .and_then(|t| t.genre())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    // lofty falls back to the recording date's first four digits, which is
    // where most formats actually keep the year
    let year = tag
        .and_then(|t| t.year())
        .filter(|y| *y > 0)
        .map(|y| y as i32);

    Ok(TagData {
        title,
        artist,
        album,
        duration_secs,
        track_no,
        genre,
        year,
    })
}

/// Source metadata for library badges; missing rate/depth stay absent (e.g. MP3).
/// This is source information, not a statement about negotiated output.
pub fn read_audio_format(path: &Path) -> anyhow::Result<String> {
    let file = Probe::open(path)?
        .options(lofty::config::ParseOptions::new().read_tags(false))
        .guess_file_type()?
        .read()?;
    let props = file.properties();
    let mut label = format!("{:?}", file.file_type()).to_uppercase();
    if let Some(rate) = props.sample_rate() {
        label.push_str(&format!(" {} кГц", rate as f64 / 1000.0));
    }
    if let Some(depth) = props.bit_depth() {
        label.push_str(&format!(" / {depth} бит"));
    }
    Ok(label)
}

#[cfg(test)]
mod format_tests {
    use super::*;
    #[test]
    fn format_badges_use_source_properties_and_report_unreadable_sources() {
        let dir = crate::testing::scratch_dir("source-format-label");
        let path = crate::testing::write_tone(&dir, "tone.wav", 0.1);
        let format = read_audio_format(&path).unwrap();
        assert!(format.starts_with("WAV"), "{format}");
        assert!(format.contains("8 кГц"), "{format}");
        assert!(format.contains("16 бит"), "{format}");
        std::fs::remove_file(&path).unwrap();
        assert!(read_audio_format(&path).is_err());
    }
}
