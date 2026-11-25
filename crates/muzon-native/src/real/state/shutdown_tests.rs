use super::*;
use muzon_core::testing::{gst_lock, scratch_dir};

#[test]
fn audio_discard_still_flushes_other_settings() {
    let dir = scratch_dir("discard-keeps-other-settings");
    let mut hotkeys = crate::real::hotkeys::Hotkeys::new(&dir);
    hotkeys.reset();
    let mut appearance = crate::real::appearance::Appearance::new(&dir);
    appearance.mark_dirty();
    CloseSettings {
        audio: None,
        hotkeys: hotkeys.flush(),
        appearance: appearance.flush(),
    }
    .save()
    .unwrap();
    assert!(dir.join("config/hotkeys.json").is_file());
    assert!(dir.join("config/appearance.json").is_file());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn audio_save_failure_does_not_skip_unrelated_settings() {
    let _gst = gst_lock();
    let dir = scratch_dir("failed-audio-keeps-other-settings");
    std::fs::create_dir_all(dir.join("config/playback_settings.json")).unwrap();
    let mut core = Controller::spawn_test(CoreConfig {
        db: Arc::new(Db::open(&dir.join("library.sqlite3")).unwrap()),
        session_store: SessionStore::new(&dir.join("config")),
        playback_settings_store: PlaybackSettingsStore::new(&dir.join("config")),
        cover_dir: dir.join("covers"),
    })
    .unwrap();
    core.preview_playback(muzon_core::domain::playback_settings::PlaybackEdit::Crossfade(2.));
    core.barrier().unwrap();
    let mut hotkeys = crate::real::hotkeys::Hotkeys::new(&dir);
    hotkeys.reset();
    let mut appearance = crate::real::appearance::Appearance::new(&dir);
    appearance.mark_dirty();
    let settings = CloseSettings {
        audio: Some(core.client()),
        hotkeys: hotkeys.flush(),
        appearance: appearance.flush(),
    };
    assert!(settings.save().is_err());
    assert!(dir.join("config/hotkeys.json").is_file());
    assert!(dir.join("config/appearance.json").is_file());
    core.shutdown();
    std::fs::remove_dir_all(dir).unwrap();
}
