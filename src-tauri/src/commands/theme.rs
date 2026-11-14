use crate::data::system_theme;
use crate::data::theme_store::ThemeSource;
use crate::domain::Theme;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub fn get_theme(state: State<AppState>) -> Theme {
    state.theme.lock().unwrap().clone()
}

#[tauri::command]
#[specta::specta]
pub fn set_theme(state: State<AppState>, theme: Theme) -> Result<(), String> {
    theme.validate()?;
    state.theme_store.save(&theme).map_err(|e| e.to_string())?;
    *state.theme.lock().unwrap() = theme;
    // Editing a colour by hand means the user no longer wants the shell's
    // palette - otherwise the next wallpaper change would undo the edit.
    if *state.theme_source.lock().unwrap() == ThemeSource::System {
        state
            .theme_store
            .save_source(ThemeSource::Manual)
            .map_err(|e| e.to_string())?;
        *state.theme_source.lock().unwrap() = ThemeSource::Manual;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_theme_source(state: State<AppState>) -> ThemeSource {
    *state.theme_source.lock().unwrap()
}

/// Switch between the user's own palette and the desktop shell's. Switching to
/// the system palette doesn't touch the manual one, so coming back restores it.
#[tauri::command]
#[specta::specta]
pub fn set_theme_source(state: State<AppState>, source: ThemeSource) -> Result<Theme, String> {
    let theme = match source {
        ThemeSource::System => {
            system_theme::load().unwrap_or_else(|_| state.theme.lock().unwrap().clone())
        }
        ThemeSource::Manual => state.theme_store.load_or_default(),
    };
    theme.validate()?;

    state
        .theme_store
        .save_source(source)
        .map_err(|e| e.to_string())?;
    *state.theme_source.lock().unwrap() = source;
    *state.theme.lock().unwrap() = theme.clone();
    Ok(theme)
}

/// Whether a valid desktop palette is available right now. The user can
/// enable following before it appears; until then the last theme stays.
#[tauri::command]
#[specta::specta]
pub fn system_theme_available() -> bool {
    system_theme::load().is_ok()
}

#[tauri::command]
#[specta::specta]
pub fn get_default_theme(mode: String) -> Theme {
    if mode == "light" {
        Theme::default_light()
    } else {
        Theme::default_dark()
    }
}

#[tauri::command]
#[specta::specta]
pub fn export_theme(state: State<AppState>, path: String) -> Result<(), String> {
    let theme = state.theme.lock().unwrap().clone();
    let json = serde_json::to_string_pretty(&theme).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn import_theme(state: State<AppState>, path: String) -> Result<Theme, String> {
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let theme: Theme = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    theme.validate()?;
    // Importing is an explicit choice of a personal palette, just like
    // editing a colour: the watcher must stop applying the shell's theme.
    set_theme(state, theme.clone())?;
    Ok(theme)
}

/// Selected desktop palette path, including a path whose file has not appeared yet.
#[tauri::command]
#[specta::specta]
pub fn system_theme_path() -> Option<String> {
    system_theme::scheme_path().map(|p| p.to_string_lossy().into_owned())
}
