// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK's DMABUF renderer crashes on transparent windows with proprietary
    // Nvidia drivers under Wayland (GBM/"Error 71"). Our Now Playing view relies on
    // a transparent window for the blurred cover-art background, so default this off;
    // an explicit override (e.g. to force it back on for testing) is respected.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    muzon_lib::run()
}
