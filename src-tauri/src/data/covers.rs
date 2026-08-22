use base64::Engine;
use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use std::path::Path;

const FOLDER_COVER_NAMES: &[&str] = &[
    "cover.jpg",
    "cover.jpeg",
    "cover.png",
    "folder.jpg",
    "folder.jpeg",
    "folder.png",
    "front.jpg",
    "front.png",
];

pub fn read_cover_data_url(path: &Path) -> Option<String> {
    let (mime, bytes) = read_embedded_cover(path).or_else(|| read_folder_cover(path))?;
    Some(to_data_url(&mime, &bytes))
}

fn read_embedded_cover(path: &Path) -> Option<(String, Vec<u8>)> {
    let tagged_file = Probe::open(path).ok()?.read().ok()?;
    let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag())?;
    let picture = tag.pictures().first()?;
    let mime = picture
        .mime_type()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "image/jpeg".to_string());
    Some((mime, picture.data().to_vec()))
}

fn read_folder_cover(path: &Path) -> Option<(String, Vec<u8>)> {
    let dir = path.parent()?;
    for name in FOLDER_COVER_NAMES {
        let candidate = dir.join(name);
        if candidate.is_file() {
            let bytes = std::fs::read(&candidate).ok()?;
            let mime = if name.ends_with(".png") {
                "image/png"
            } else {
                "image/jpeg"
            };
            return Some((mime.to_string(), bytes));
        }
    }
    None
}

fn to_data_url(mime: &str, bytes: &[u8]) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime};base64,{encoded}")
}
