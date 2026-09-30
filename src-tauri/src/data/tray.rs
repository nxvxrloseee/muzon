//! The tray icon: what's playing, the transport controls, and a way back to
//! a window closed into the tray.
//!
//! On Linux the icon is a StatusNotifierItem (through libayatana-appindicator),
//! which shows only where the panel hosts a tray; clicking it always opens
//! the menu, as appindicator has no click of its own to report.

use crate::state::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

/// Asks the frontend to save everything and close for good - the tray's and
/// MPRIS's "quit", which must not be turned into a hide by close-to-tray.
pub const QUIT_REQUESTED: &str = "app-quit-requested";

/// The menu entries that change as playback does.
pub struct TrayItems {
    now_playing: MenuItem<tauri::Wry>,
    toggle: MenuItem<tauri::Wry>,
}

const NOTHING_PLAYING: &str = "Ничего не играет";

pub fn install(app: &AppHandle) -> tauri::Result<TrayItems> {
    let now_playing = MenuItem::with_id(app, "now", NOTHING_PLAYING, false, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Играть", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &now_playing,
            &PredefinedMenuItem::separator(app)?,
            &toggle,
            &MenuItem::with_id(app, "next", "Следующий", true, None::<&str>)?,
            &MenuItem::with_id(app, "previous", "Предыдущий", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "show", "Показать окно", true, None::<&str>)?,
            &MenuItem::with_id(app, "quit", "Выйти", true, None::<&str>)?,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("muzon")
        .menu(&menu)
        .tooltip("Muzon")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle" => {
                let player = &app.state::<AppState>().player;
                let _ = if player.status().is_playing {
                    player.pause()
                } else {
                    player.play()
                };
            }
            // The queue lives in the frontend; these reach it the same way the
            // media keys do
            "next" => {
                let _ = app.emit("mpris-next", ());
            }
            "previous" => {
                let _ = app.emit("mpris-previous", ());
            }
            "show" => show_window(app),
            "quit" => {
                let _ = app.emit(QUIT_REQUESTED, ());
            }
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;

    Ok(TrayItems {
        now_playing,
        toggle,
    })
}

/// Brings the window back, whether it was minimised or closed into the tray.
pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// `label` is "Title — Artist", or `None` when nothing is loaded.
pub fn update(app: &AppHandle, label: Option<String>, playing: bool) {
    let state = app.state::<AppState>();
    let guard = state.tray.lock().unwrap();
    let Some(items) = guard.as_ref() else {
        return;
    };
    let _ = items
        .now_playing
        .set_text(label.as_deref().unwrap_or(NOTHING_PLAYING));
    let _ = items.toggle.set_text(if playing {
        "Пауза"
    } else {
        "Играть"
    });
    let _ = items.toggle.set_enabled(label.is_some());
}

/// Menu label for a track; the artist only when there is one.
pub fn track_label(title: &str, artist: Option<&str>) -> String {
    // Long titles would stretch the whole menu
    const MAX: usize = 60;
    let full = match artist {
        Some(a) if !a.is_empty() => format!("{title} — {a}"),
        _ => title.to_string(),
    };
    if full.chars().count() > MAX {
        let cut: String = full.chars().take(MAX - 1).collect();
        format!("{cut}…")
    } else {
        full
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_names_the_artist_when_known_and_stays_short() {
        assert_eq!(
            track_label("So What", Some("Miles Davis")),
            "So What — Miles Davis"
        );
        assert_eq!(track_label("So What", None), "So What");
        assert_eq!(track_label("So What", Some("")), "So What");
        let long = track_label(&"я".repeat(100), None);
        assert_eq!(long.chars().count(), 60);
        assert!(long.ends_with('…'));
    }
}
