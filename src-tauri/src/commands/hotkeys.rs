use crate::domain::Hotkeys;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub fn get_hotkeys(state: State<AppState>) -> Hotkeys {
    state.hotkeys.lock().unwrap().clone()
}

#[tauri::command]
#[specta::specta]
pub fn set_hotkeys(state: State<AppState>, hotkeys: Hotkeys) -> Result<(), String> {
    state
        .hotkeys_store
        .save(&hotkeys)
        .map_err(|e| e.to_string())?;
    *state.hotkeys.lock().unwrap() = hotkeys;
    Ok(())
}
