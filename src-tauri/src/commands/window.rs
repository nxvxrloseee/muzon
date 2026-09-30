use crate::domain::{window_chrome, Appearance};
use crate::state::AppState;
use tauri::State;

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

#[tauri::command]
#[specta::specta]
pub fn get_appearance(state: State<AppState>) -> Appearance {
    *state.appearance.lock().unwrap()
}

/// Saves the settings and returns them as stored (opacity clamped). Opacity is
/// applied by the frontend straight away; transparency only on the next launch.
#[tauri::command]
#[specta::specta]
pub fn set_appearance(
    state: State<AppState>,
    appearance: Appearance,
) -> Result<Appearance, String> {
    let appearance = appearance.normalized();
    state
        .appearance_store
        .save(&appearance)
        .map_err(|e| e.to_string())?;
    *state.appearance.lock().unwrap() = appearance;
    Ok(appearance)
}

/// Whether this window has an alpha channel, i.e. whether background opacity
/// can do anything right now.
#[tauri::command]
#[specta::specta]
pub fn window_is_transparent(state: State<AppState>) -> bool {
    state.window_transparent
}
