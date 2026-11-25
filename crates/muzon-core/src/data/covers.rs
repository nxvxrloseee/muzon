use base64::Engine;
use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use std::io::Cursor;
use std::path::{Path, PathBuf};

const THUMBNAIL_MAX_DIM: u32 = 200;

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
    let (mime, bytes) = read_cover_source(path)?;
    let (mime, bytes) = downscale_cover(&bytes).unwrap_or((mime, bytes));
    Some(to_data_url(&mime, &bytes))
}

/// Thumbnail bytes for the `muzon-cover://` protocol, memoized on disk under
/// `cache_dir`. Decoding a full-size embedded JPEG per list row was the
/// expensive part of scrolling the library; once a track's thumbnail has been
/// written here, every later request is a plain file read.
pub fn read_cover_thumbnail(cache_dir: &Path, track_path: &Path) -> Option<(String, Vec<u8>)> {
    let key = cache_key(track_path);
    if let Some((mime, path)) = cached_thumbnail(cache_dir, &key) {
        if let Ok(bytes) = std::fs::read(&path) {
            return Some((mime, bytes));
        }
    }
    write_thumbnail(cache_dir, &key, track_path).map(|(mime, bytes, _)| (mime, bytes))
}

/// Filesystem path of the same cached thumbnail, generating it if it isn't
/// there yet. MPRIS wants a `file://` URI the desktop shell can open for
/// itself, not the bytes - and it should get the artwork the app already
/// decoded for its own list rows rather than decoding the cover a second time.
pub fn ensure_thumbnail_path(cache_dir: &Path, track_path: &Path) -> Option<PathBuf> {
    let key = cache_key(track_path);
    if let Some((_, path)) = cached_thumbnail(cache_dir, &key) {
        return Some(path);
    }
    // Unlike the byte-returning path above, a failed cache write is fatal here:
    // the whole point is to hand out a location something else can read.
    let (_, _, path) = write_thumbnail(cache_dir, &key, track_path)?;
    path.is_file().then_some(path)
}

fn cached_thumbnail(cache_dir: &Path, key: &str) -> Option<(String, PathBuf)> {
    for (ext, mime) in [("jpg", "image/jpeg"), ("png", "image/png")] {
        let candidate = thumbnail_path(cache_dir, key, ext);
        if candidate.is_file() {
            return Some((mime.to_string(), candidate));
        }
    }
    None
}

/// Decodes, downscales and memoizes the cover, returning what it wrote and where.
fn write_thumbnail(
    cache_dir: &Path,
    key: &str,
    track_path: &Path,
) -> Option<(String, Vec<u8>, PathBuf)> {
    let (mime, bytes) = read_cover_source(track_path)?;
    let (mime, bytes) = downscale_cover(&bytes).unwrap_or((mime, bytes));

    // Best-effort: a failed cache write just means we recompute next time.
    let ext = if mime == "image/png" { "png" } else { "jpg" };
    let path = thumbnail_path(cache_dir, key, ext);
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(&path, &bytes);

    Some((mime, bytes, path))
}

fn thumbnail_path(cache_dir: &Path, key: &str, ext: &str) -> PathBuf {
    cache_dir.join(format!("{key}.{ext}"))
}

/// Path + mtime, so retagging a file (which rewrites its embedded art) yields a
/// different key instead of serving the stale thumbnail forever.
fn cache_key(track_path: &Path) -> String {
    let mtime = std::fs::metadata(track_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in track_path
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .chain(mtime.to_le_bytes().iter())
    {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn read_cover_source(path: &Path) -> Option<(String, Vec<u8>)> {
    read_embedded_cover(path).or_else(|| read_folder_cover(path))
}

fn downscale_cover(bytes: &[u8]) -> Option<(String, Vec<u8>)> {
    let img = image::load_from_memory(bytes).ok()?;
    let thumb = if img.width() > THUMBNAIL_MAX_DIM || img.height() > THUMBNAIL_MAX_DIM {
        img.thumbnail(THUMBNAIL_MAX_DIM, THUMBNAIL_MAX_DIM)
    } else {
        img
    };

    let mut out = Vec::new();
    if thumb.color().has_alpha() {
        thumb
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .ok()?;
        Some(("image/png".to_string(), out))
    } else {
        let rgb = thumb.to_rgb8();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
        encoder.encode_image(&rgb).ok()?;
        Some(("image/jpeg".to_string(), out))
    }
}

fn read_embedded_cover(path: &Path) -> Option<(String, Vec<u8>)> {
    let tagged_file = Probe::open(path).ok()?.read().ok()?;
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())?;
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

/// Explicit refresh for changes to a sibling cover, whose mtime is not the audio mtime.
pub fn invalidate_thumbnail(cache_dir: &Path, track_path: &Path) {
    let key = cache_key(track_path);
    for extension in ["png", "jpg"] {
        let _ = std::fs::remove_file(thumbnail_path(cache_dir, &key, extension));
    }
}

/// Dominant opaque RGB bucket from a tiny thumbnail; no per-frame image decoding.
pub fn dominant_rgb(encoded: &[u8]) -> Option<[u8; 3]> {
    let image = image::load_from_memory(encoded)
        .ok()?
        .thumbnail(24, 24)
        .to_rgba8();
    let mut buckets = [[0u32; 4]; 512];
    for pixel in image.pixels().filter(|p| p[3] >= 128) {
        let index = ((pixel[0] as usize >> 5) << 6)
            | ((pixel[1] as usize >> 5) << 3)
            | (pixel[2] as usize >> 5);
        let bucket = &mut buckets[index];
        bucket[0] += pixel[0] as u32;
        bucket[1] += pixel[1] as u32;
        bucket[2] += pixel[2] as u32;
        bucket[3] += 1;
    }
    let bucket = buckets.iter().max_by_key(|b| b[3])?;
    (bucket[3] > 0).then(|| {
        [
            (bucket[0] / bucket[3]) as u8,
            (bucket[1] / bucket[3]) as u8,
            (bucket[2] / bucket[3]) as u8,
        ]
    })
}

#[cfg(test)]
mod dominant_tests {
    use super::*;
    #[test]
    fn dominant_color_ignores_transparency_and_rejects_broken_images() {
        let mut image = image::RgbaImage::from_pixel(24, 24, image::Rgba([220, 60, 80, 255]));
        for x in 0..8 {
            for y in 0..24 {
                image.put_pixel(x, y, image::Rgba([0, 255, 0, 0]));
            }
        }
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(dominant_rgb(bytes.get_ref()), Some([220, 60, 80]));
        assert_eq!(dominant_rgb(b"not an image"), None);
    }
}

#[cfg(test)]
mod cache_revision_tests {
    use super::*;
    #[test]
    fn edits_in_one_second_get_distinct_cover_cache_keys() {
        let dir = crate::testing::scratch_dir("cover-subsecond-key");
        let path = dir.join("track.wav");
        std::fs::write(&path, b"test").unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        let stamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(12345);
        file.set_times(
            std::fs::FileTimes::new().set_modified(stamp + std::time::Duration::from_nanos(1)),
        )
        .unwrap();
        let first = cache_key(&path);
        file.set_times(
            std::fs::FileTimes::new().set_modified(stamp + std::time::Duration::from_nanos(2)),
        )
        .unwrap();
        let second = cache_key(&path);
        assert_ne!(first, second);
        let _ = std::fs::remove_dir_all(dir);
    }
}
