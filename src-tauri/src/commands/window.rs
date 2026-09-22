use crate::domain::window_chrome;

/// Whether the frontend should draw its own window controls, decided from the
/// session the app was launched into. See `domain::window_chrome`.
#[tauri::command]
#[specta::specta]
pub fn get_window_controls_visible() -> bool {
    let xdg_current_desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    let desktop_session = std::env::var("DESKTOP_SESSION").ok();
    let hyprland_signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();

    window_chrome::draws_own_controls(
        xdg_current_desktop.as_deref(),
        desktop_session.as_deref(),
        hyprland_signature.as_deref(),
    )
}
