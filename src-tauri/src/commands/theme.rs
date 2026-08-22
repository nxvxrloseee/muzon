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
    Ok(())
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
    state.theme_store.save(&theme).map_err(|e| e.to_string())?;
    *state.theme.lock().unwrap() = theme.clone();
    Ok(theme)
}
