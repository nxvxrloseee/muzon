use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Hotkeys {
    pub play_pause: String,
    pub seek_forward: String,
    pub seek_backward: String,
    pub volume_up: String,
    pub volume_down: String,
    pub next_track: String,
    pub previous_track: String,
    pub close_now_playing: String,
}

impl Hotkeys {
    pub fn default_bindings() -> Self {
        Self {
            play_pause: " ".into(),
            seek_forward: "ArrowRight".into(),
            seek_backward: "ArrowLeft".into(),
            volume_up: "ArrowUp".into(),
            volume_down: "ArrowDown".into(),
            next_track: "n".into(),
            previous_track: "p".into(),
            close_now_playing: "Escape".into(),
        }
    }
}
