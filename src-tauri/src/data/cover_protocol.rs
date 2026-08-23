use crate::data::covers;
use base64::Engine;
use std::path::{Path, PathBuf};
use tauri::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

/// Cover art used to travel to the frontend as a base64 data URL, one `invoke`
/// per visible row - which on Linux/webkitgtk dispatches through the GTK main
/// thread and hands WebKit an un-cacheable image every time. Serving thumbnails
/// over a real URL instead lets WebKit cache and decode them off the main
/// thread, so scrolling a large library stops touching the IPC layer at all.
pub const SCHEME: &str = "muzon-cover";

/// URL path is the track path, base64url-encoded: unlike percent-encoding it
/// survives the round trip through the webview's URL parser untouched.
pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let cache_dir = ctx
        .app_handle()
        .path()
        .app_cache_dir()
        .ok()
        .map(|dir| dir.join("covers"));
    let encoded = request.uri().path().trim_start_matches('/').to_string();

    // Reading tags and decoding/resizing an image is blocking work; the
    // protocol handler itself runs on the UI thread.
    tauri::async_runtime::spawn_blocking(move || {
        let response = build_response(cache_dir, &encoded).unwrap_or_else(not_found);
        responder.respond(response);
    });
}

fn build_response(cache_dir: Option<PathBuf>, encoded: &str) -> Option<Response<Vec<u8>>> {
    let cache_dir = cache_dir?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .ok()?;
    let track_path = String::from_utf8(decoded).ok()?;
    let (mime, bytes) = covers::read_cover_thumbnail(&cache_dir, Path::new(&track_path))?;

    Response::builder()
        .header(CONTENT_TYPE, mime)
        // The cache key already includes the file's mtime, so a given URL's
        // bytes can never change - safe to let WebKit hold onto them.
        .header(CACHE_CONTROL, "max-age=31536000, immutable")
        .body(bytes)
        .ok()
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Vec::new())
        .expect("static 404 response is always valid")
}
