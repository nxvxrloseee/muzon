use crate::data::s3::client::S3Client;
use crate::data::s3::{creds, sync};
use crate::domain::S3Config;
use crate::state::AppState;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
#[specta::specta]
pub fn get_s3_config(state: State<AppState>) -> S3Config {
    state.s3_config.lock().unwrap().clone()
}

#[tauri::command]
#[specta::specta]
pub fn set_s3_config(state: State<AppState>, config: S3Config) -> Result<(), String> {
    state
        .s3_config_store
        .save(&config)
        .map_err(|e| e.to_string())?;
    *state.s3_config.lock().unwrap() = config;
    Ok(())
}

/// Keys go to the system keyring, never to a config file.
#[tauri::command]
#[specta::specta]
pub fn set_s3_credentials(access_key: String, secret_key: String) -> Result<(), String> {
    creds::save(&access_key, &secret_key).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn forget_s3_credentials() {
    creds::forget();
}

#[tauri::command]
#[specta::specta]
pub fn has_s3_credentials() -> bool {
    creds::are_set()
}

/// One cheap request: proves the endpoint, the bucket and the keys work.
#[tauri::command]
#[specta::specta]
pub async fn check_s3_connection(app: AppHandle) -> Result<(), String> {
    let config = app.state::<AppState>().s3_config.lock().unwrap().clone();
    let credentials = creds::load().map_err(|e| e.to_string())?;
    let client = S3Client::new(config, credentials).map_err(|e| e.to_string())?;
    client.check_access().await.map_err(|e| e.to_string())
}

/// Runs the whole sync. Progress arrives as `sync-progress` events.
#[tauri::command]
#[specta::specta]
pub async fn sync_now(app: AppHandle) -> Result<sync::SyncOutcome, String> {
    let config = app.state::<AppState>().s3_config.lock().unwrap().clone();
    sync::run(app.clone(), config)
        .await
        .map_err(|e| e.to_string())
}
